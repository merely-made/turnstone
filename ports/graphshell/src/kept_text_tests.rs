// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Kept page text read back by the goal surface as stored page text. The
//! records' own behaviour is tested in `eidetic::kept_text` (engine split D71).

use crate::app::GraphshellApp;
use crate::capture::{BrowserHistoryCapture, BrowserVisit, HistoryCapturePolicy};
use crate::goal_surface::{GoalSurface, Outcome, Passage};
use graphshell_host::goal_surface::{GraphFacts, Origin, RetainedPages};
use crate::local_profile::{Browser, LocalProfile};
use eidetic::Hash;
use eidetic::kept_text::{KeepTextPolicy, KeptBatch, KeptPage, record_kept_pages};
use muniment::MemoryBackend;
use std::collections::BTreeSet;
use std::rc::Rc;
use uuid::Uuid;

const BODY: &str = "Turn the pile weekly. Browns balance greens, and a damp, warm heap \
    breaks down faster than a dry one.";
fn policy() -> KeepTextPolicy {
    KeepTextPolicy {
        enabled: true,
        max_bytes: 0,
        excluded_origins: BTreeSet::new(),
    }
}
fn page(url: &str, text: &str, at_ms: u64) -> KeptPage {
    KeptPage {
        url: url.into(),
        text: text.into(),
        at_ms,
        private: false,
    }
}
fn batch(pages: Vec<KeptPage>) -> KeptBatch {
    KeptBatch {
        policy: policy(),
        pages,
    }
}
fn visit(id: &str, url: &str, title: &str, at_ms: u64) -> BrowserVisit {
    BrowserVisit {
        source: "chromium".into(),
        visit_id: Some(id.into()),
        source_scope: None,
        parent: eidetic::import::encounters::BrowserParent::Unknown,
        original_unix_micros: None,
        source_is_local: None,
        url: url.into(),
        title: Some(title.into()),
        favicon_url: None,
        referrer_url: None,
        transition: "link".into(),
        at_ms,
        private: false,
    }
}

/// The browser path end to end: a visit becomes a graph object with a title and
/// address, kept text is recorded, and a goal that only the body can match
/// finds it and cites the body passage; without the text the same goal finds
/// nothing, since titles and addresses do not contain the words.
#[test]
fn a_goal_that_only_page_bodies_match_cites_the_kept_passage() {
    pollster::block_on(async {
        const OWNER: &str = "graphshell-local-test";
        const URL: &str = "https://garden.example/compost";
        let store = MemoryBackend::new();
        let profile = LocalProfile::new(Uuid::from_u128(5), Browser::Chromium);
        let mut app = GraphshellApp::open_local(store.clone(), &profile, 1)
            .await
            .unwrap();
        let mut writes = store.clone();
        let mut capture =
            BrowserHistoryCapture::load(&mut writes, HistoryCapturePolicy::consented())
                .await
                .unwrap();
        capture
            .ingest_batch(
                &mut app.host,
                &mut writes,
                vec![
                    visit("1", URL, "Garden notes", 1000),
                    visit("2", "https://garden.example/other", "Other notes", 1001),
                ],
                "personae://persona/capture",
                "personae://device/browser",
                2,
            )
            .await
            .unwrap();
        let load = |app: &GraphshellApp<MemoryBackend>| {
            let facts = GraphFacts::read(app.host.graph());
            let store = store.clone();
            async move {
                RetainedPages::load_facts(OWNER, &store, &facts, 1 << 20, true)
                    .await
                    .unwrap()
            }
        };
        let run = |pages: RetainedPages, goal: &str| {
            let mut surface = GoalSurface::new(Rc::new(pages));
            surface.goal.insert_str(goal);
            surface.run();
            let run = surface.run.take().expect("a run");
            run.matches().cloned().collect::<Vec<_>>()
        };
        let goal = "damp warm heap breaks down faster";

        let before = load(&app).await;
        assert!(
            before
                .sources()
                .iter()
                .all(|(_, p)| p.origin == Origin::Record)
        );
        assert!(
            run(before, goal).is_empty(),
            "titles and addresses do not hold these words"
        );

        let kept = record_kept_pages(&store, &batch(vec![page(URL, BODY, 5)])).await;
        assert_eq!((kept.kept, kept.error), (1, None));
        let after = load(&app).await;
        let origin = |url: &str| {
            after
                .sources()
                .into_iter()
                .find(|(_, p)| p.url == url)
                .unwrap()
                .1
                .origin
        };
        assert_eq!(
            (origin(URL), origin("https://garden.example/other")),
            (Origin::PageText, Origin::Record)
        );
        let hits = run(after, goal);
        let hit = hits
            .iter()
            .find(|s| s.address == URL)
            .expect("the kept page matches");
        let Outcome::Visited(visited) = &hit.outcome else {
            panic!("visited")
        };
        assert!(visited.retained_version == Hash::of(BODY.as_bytes()).to_hex());
        let passage: String = visited
            .passages
            .iter()
            .map(Passage::text)
            .collect();
        assert!(passage.contains("breaks down faster"), "{passage}");
    });
}
