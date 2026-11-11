// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The browser's retained records, the goal run over them, and the durable
//! tag-suggestion flow, on `MemoryBackend` standing in for IndexedDB: both are
//! the same `Backend` seam, and the app opens the same way (`open_local`).

use super::suggest::{self, Accepted, Review};
use super::*;
use crate::app::GraphshellApp;
use crate::local_edit::{metadata, save_metadata};
use crate::local_intake::{create_address, persist_intake};
use crate::local_profile::{Browser, LocalProfile};
use eidetic::PageTextStore;
use graphshell_host::goal_surface::{GraphFacts, Origin, RetainedPages, record_text, source_id};
use eidetic::records::SuggestionLedger;
use muniment::{Backend, MemoryBackend, StoreError, TransactFn, WriteOp};
use uuid::Uuid;

const OWNER: &str = "graphshell-local-test";
const A: &str = "https://notes.example/starter-basics";
const B: &str = "https://notes.example/away-for-a-month";
const C: &str = "https://notes.example/oven-steam";

/// A store handle that reads and refuses every write, as the browser's
/// `IndexedDbBackend::read_only` does for a follower.
#[derive(Clone)]
struct ReadOnly(MemoryBackend);

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Backend for ReadOnly {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.0.get(key).await
    }
    async fn put(&self, _: &str, _: &[u8]) -> Result<(), StoreError> {
        Err(StoreError::Backend("read-only".into()))
    }
    async fn delete(&self, _: &str) -> Result<(), StoreError> {
        Err(StoreError::Backend("read-only".into()))
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        self.0.list(prefix).await
    }
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        self.0.scan(start, end).await
    }
    async fn apply(&self, _: &[WriteOp]) -> Result<(), StoreError> {
        Err(StoreError::Backend("read-only".into()))
    }
    async fn transact(&self, _: TransactFn) -> Result<(), StoreError> {
        Err(StoreError::Backend("read-only".into()))
    }
}

fn profile() -> LocalProfile {
    LocalProfile::new(Uuid::from_u128(77), Browser::Chromium)
}

/// A saved profile with three captured objects. `A` carries the "sourdough
/// starter" tag the graph then holds as a concept; `B` and `C` are untagged.
async fn saved() -> (MemoryBackend, GraphshellApp<MemoryBackend>, Uuid, Uuid, Uuid) {
    let store = MemoryBackend::new();
    let mut app = GraphshellApp::open_local(store.clone(), &profile(), 1).await.unwrap();
    let mut add = |app: &mut GraphshellApp<MemoryBackend>, url: &str, title: &str| {
        create_address(app, url, title).unwrap()
    };
    let a = add(&mut app, A, "Sourdough starter basics");
    let b = add(&mut app, B, "Keeping a sourdough starter alive while away for a month");
    let c = add(&mut app, C, "Steam in the oven");
    persist_intake(&mut app, c, 2).await.unwrap();
    save_metadata(&mut app, a, "Sourdough starter basics", "sourdough, oven", 3).await.unwrap();
    (store, app, a, b, c)
}

async fn pages_of(store: &MemoryBackend, app: &GraphshellApp<MemoryBackend>) -> (RetainedPages, GraphFacts) {
    let facts = GraphFacts::read(app.host.graph());
    let pages = RetainedPages::load_facts(OWNER, store, &facts, 1 << 20, true).await.unwrap();
    (pages, facts)
}

fn tags_of(app: &GraphshellApp<MemoryBackend>, member: Uuid) -> Vec<String> {
    metadata(app, member).unwrap().tags
}

#[test]
fn a_browser_capture_retains_titles_and_addresses_and_no_page_text() {
    pollster::block_on(async {
        let (store, app, ..) = saved().await;
        let facts = GraphFacts::read(app.host.graph());
        let none = RetainedPages::load_facts(OWNER, &store, &facts, 1 << 20, false).await.unwrap();
        assert!(none.sources().is_empty(), "nothing writes PageTextStore unless the person keeps page text");
        let (pages, _) = pages_of(&store, &app).await;
        let sources = pages.sources();
        assert_eq!(sources.len(), 3);
        for (_, page) in &sources {
            assert_eq!(page.origin, Origin::Record);
            let text = &page.versions[&page.current];
            assert_eq!(*text, record_text(&page.url, page.title.as_deref()));
            assert_eq!(page.current, eidetic::Hash::of(text.as_bytes()).to_hex());
        }
        // Stored page text, when something does write it, wins for that page.
        PageTextStore::new(store.clone()).put(A, "Real body text about starter care.", 9).await.unwrap();
        let (pages, _) = pages_of(&store, &app).await;
        let by_url = |u: &str| pages.sources().into_iter().find(|(_, p)| p.url == u).unwrap().1.origin;
        assert_eq!((by_url(A), by_url(B)), (Origin::PageText, Origin::Record));
    });
}

#[test]
fn a_goal_runs_over_the_records_and_cites_exact_versions() {
    pollster::block_on(async {
        let (store, app, ..) = saved().await;
        let (pages, _) = pages_of(&store, &app).await;
        let pages = std::rc::Rc::new(pages);
        let mut surface = GoalSurface::new(pages.clone());
        surface.goal.insert_str("keep a sourdough starter alive");
        surface.run();
        let run = surface.run.as_ref().expect("a run");
        let hits: Vec<_> = run.matches().collect();
        assert!(hits.iter().any(|s| s.address == B), "{:?}", surface.notice);
        let Outcome::Visited(v) = &hits.iter().find(|s| s.address == B).unwrap().outcome else {
            panic!("visited")
        };
        assert!(pages.text_of(&source_id(OWNER, chartulary::resource_id(B)), &v.retained_version).is_some());
        assert!(!v.passages.is_empty());
    });
}

#[test]
fn suggestions_come_from_existing_concepts_and_skip_pages_that_hold_them() {
    pollster::block_on(async {
        let (store, app, ..) = saved().await;
        let (pages, facts) = pages_of(&store, &app).await;
        let produced = suggest::produce(&pages, &facts, None, "run-1").unwrap();
        assert_eq!(produced.concepts, 2, "sourdough and oven");
        let rows = suggest::rows(
            &suggest::Held { suggestions: produced.suggestions.clone(), reviews: vec![] },
            &pages,
            &facts,
        );
        let pairs: Vec<_> = rows.iter().map(|r| (r.address.as_str(), r.concept.as_str())).collect();
        // A already holds sourdough and oven; B names sourdough; C names oven.
        assert_eq!(pairs.len(), 2, "{pairs:?}");
        assert!(pairs.contains(&(B, "sourdough")) && pairs.contains(&(C, "oven")), "{pairs:?}");
        for row in &rows {
            assert_eq!((row.run.as_str(), row.keeping), ("run-1", "retained"));
            assert!(row.producer.contains("label-terms"));
            assert!(!row.evidence.is_empty() && row.review == Review::Unreviewed);
        }
        let scoped: std::collections::BTreeSet<_> =
            [source_id(OWNER, chartulary::resource_id(B))].into();
        let one = suggest::produce(&pages, &facts, Some(&scoped), "run-1").unwrap();
        assert_eq!(one.suggestions.len(), 1);
    });
}

#[test]
fn the_ledger_survives_a_cold_reopen_with_its_review_states() {
    pollster::block_on(async {
        let (store, mut app, _, b, c) = saved().await;
        let (pages, facts) = pages_of(&store, &app).await;
        let batch = suggest::produce(&pages, &facts, None, "run-1").unwrap().suggestions;
        assert_eq!(suggest::record(store.clone(), batch.clone()).await.unwrap(), 2);
        assert_eq!(suggest::record(store.clone(), batch).await.unwrap(), 0, "idempotent");
        let held = suggest::read(store.clone()).await.unwrap();
        let rows = suggest::rows(&held, &pages, &facts);
        let id_of = |url: &str| rows.iter().find(|r| r.address == url).unwrap().id.clone();
        suggest::accept_one(&mut app, store.clone(), OWNER, &id_of(B), 10, 4).await.unwrap();
        suggest::reject_one(store.clone(), &id_of(C), &suggest::reviewer(&app), 11).await.unwrap();

        // Cold reopen: new handles over the same bytes.
        let reopened = GraphshellApp::open_local(store.clone(), &profile(), 9).await.unwrap();
        let (pages, facts) = pages_of(&store, &reopened).await;
        let rows = suggest::rows(&suggest::read(store.clone()).await.unwrap(), &pages, &facts);
        let state = |url: &str| rows.iter().find(|r| r.address == url).unwrap().review.clone();
        assert!(matches!(state(B), Review::Accepted { .. }), "{:?}", state(B));
        assert!(matches!(state(C), Review::Rejected { .. }), "{:?}", state(C));
        assert_eq!(tags_of(&reopened, b), vec!["sourdough"], "the accepted tag was saved");
        assert!(tags_of(&reopened, c).is_empty(), "a rejection adds no tag");
    });
}

#[test]
fn accepting_cites_the_suggestion_and_is_idempotent() {
    pollster::block_on(async {
        let (store, mut app, _, b, _) = saved().await;
        let (pages, facts) = pages_of(&store, &app).await;
        suggest::record(store.clone(), suggest::produce(&pages, &facts, None, "r").unwrap().suggestions)
            .await
            .unwrap();
        let rows = suggest::rows(&suggest::read(store.clone()).await.unwrap(), &pages, &facts);
        let row = rows.iter().find(|r| r.address == B).unwrap();
        let (admission, member) = suggest::accept_one(&mut app, store.clone(), OWNER, &row.id, 10, 4).await.unwrap();
        assert_eq!((admission, member), (Accepted::Added, b));
        assert_eq!(tags_of(&app, b), vec!["sourdough"]);
        let (again, _) = suggest::accept_one(&mut app, store.clone(), OWNER, &row.id, 11, 5).await.unwrap();
        assert_eq!(again, Accepted::AlreadyHeld);
        // The statement cites the suggestion.
        let cited = format!("urn:eidetic:tag-suggestion:{}", row.id);
        let resource = app.host.graph().get_node_by_id(b).and_then(|(k, _)| app.host.graph().shown_resource_id(k)).unwrap();
        let found = app.host.graph().resource_edges().any(|(from, _, payload)| {
            from.id() == resource
                && payload.held_semantic_statements().any(|s| {
                    s.basis.method.as_ref().is_some_and(|m| m.iri == "urn:mere:method:accepted-tag-suggestion")
                        && s.basis.evidence == vec![cited.clone()]
                })
        });
        assert!(found, "a personal assertion citing the suggestion");
        // An unknown id is refused and changes nothing.
        let bad = suggest::accept_one(&mut app, store.clone(), OWNER, &"0".repeat(64), 12, 6).await;
        assert!(bad.is_err());
    });
}

#[test]
fn reject_and_regenerate_never_touch_personal_tags() {
    pollster::block_on(async {
        let (store, mut app, a, b, c) = saved().await;
        let before = (tags_of(&app, a), tags_of(&app, b), tags_of(&app, c));
        let (pages, facts) = pages_of(&store, &app).await;
        suggest::record(store.clone(), suggest::produce(&pages, &facts, None, "run-1").unwrap().suggestions)
            .await
            .unwrap();
        let held = suggest::read(store.clone()).await.unwrap();
        let first: Vec<_> = suggest::rows(&held, &pages, &facts).iter().map(|r| r.id.clone()).collect();
        for id in &first {
            suggest::reject_one(store.clone(), id, &suggest::reviewer(&app), 20).await.unwrap();
        }
        // Regeneration: the same inputs under a new run are new suggestions.
        let again = suggest::produce(&pages, &facts, None, "run-2").unwrap().suggestions;
        assert_eq!(suggest::record(store.clone(), again).await.unwrap(), 2);
        let held = suggest::read(store.clone()).await.unwrap();
        let rows = suggest::rows(&held, &pages, &facts);
        assert_eq!(rows.len(), 4);
        for row in &rows {
            let first_run = first.contains(&row.id);
            assert_eq!(matches!(row.review, Review::Rejected { .. }), first_run, "run-1 stays rejected, run-2 is open");
        }
        assert_eq!((tags_of(&app, a), tags_of(&app, b), tags_of(&app, c)), before);
        // A rejection after an acceptance changes the review only, not the tag.
        suggest::accept_one(&mut app, store.clone(), OWNER, &rows[2].id, 30, 7).await.unwrap();
        let tagged = tags_of(&app, b);
        suggest::reject_one(store.clone(), &rows[2].id, &suggest::reviewer(&app), 31).await.unwrap();
        assert_eq!(tags_of(&app, b), tagged);
    });
}

#[test]
fn a_follower_reads_the_ledger_and_cannot_write_it() {
    pollster::block_on(async {
        let (store, app, ..) = saved().await;
        let (pages, facts) = pages_of(&store, &app).await;
        suggest::record(store.clone(), suggest::produce(&pages, &facts, None, "r").unwrap().suggestions)
            .await
            .unwrap();
        let read_only = ReadOnly(store.clone());
        assert_eq!(suggest::read(read_only.clone()).await.unwrap().suggestions.len(), 2);
        let (pages, facts) = {
            let f = GraphFacts::read(app.host.graph());
            (RetainedPages::load_facts(OWNER, &read_only, &f, 1 << 20, true).await.unwrap(), f)
        };
        assert_eq!(pages.sources().len(), 3, "the goal surface reads through a read-only handle");
        let more = suggest::produce(&pages, &facts, None, "r2").unwrap().suggestions;
        assert!(suggest::record(read_only.clone(), more).await.is_err(), "writes are refused");
        let ledger = SuggestionLedger::open(store).await.unwrap();
        assert_eq!(ledger.suggestions().len(), 2, "and nothing was stored");
    });
}
