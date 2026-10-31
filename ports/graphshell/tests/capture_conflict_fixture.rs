// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The extension's capture queue refuses a visit by the encounter ledger's
//! rule. This test judges visit pairs with `BrowserEncounterLedger::receive`
//! and holds `web/extension/fixtures/capture-conflicts.json` equal to the
//! result; `smoke-capture-model.mjs` replays the same file through
//! `mergeQueue`. Either side drifting fails its own test. After an intended
//! change, rewrite the file with `GRAPHSHELL_WRITE_FIXTURES=1`.

use eidetic::import::encounters::{
    BrowserEncounter, BrowserEncounterLedger, BrowserEncounterRef, BrowserParent,
    BrowserSourceScope,
};
use graphshell::capture::BrowserVisit;
use serde::Serialize;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/web/extension/fixtures/capture-conflicts.json"
);
const DUPLICATE: &str = "duplicate";
const CONFLICT: &str = "conflict";
const NEW: &str = "new";

fn first() -> BrowserEncounter {
    BrowserEncounter {
        identity: BrowserEncounterRef {
            scope: BrowserSourceScope {
                product: "firefox".into(),
                profile: "fixture-profile".into(),
                generation: "fixture-generation".into(),
            },
            visit_id: "visit-1".into(),
        },
        parent: BrowserParent::Visit {
            visit_id: "parent-1".into(),
        },
        url: "https://example.net/page".into(),
        title: Some("Page".into()),
        raw_transition: "link".into(),
        at_ms: 1_000,
        original_unix_micros: Some(1_000_000),
        source_is_local: Some(true),
        persona: "persona:fixture".into(),
        acquisition_device: "device:fixture".into(),
        intake_at_ms: 0,
    }
}

/// Each case changes one source fact of `first()` and names the ledger's verdict.
const CASES: &[(&str, fn(&mut BrowserEncounter), &str)] = &[
    ("identical_replay", |_| {}, DUPLICATE),
    (
        "title_differs",
        |e| e.title = Some("Another title".into()),
        DUPLICATE,
    ),
    ("title_absent", |e| e.title = None, DUPLICATE),
    (
        "url_differs",
        |e| e.url = "https://example.net/other".into(),
        CONFLICT,
    ),
    ("at_ms_differs", |e| e.at_ms = 1_001, CONFLICT),
    (
        "raw_transition_differs",
        |e| e.raw_transition = "reload".into(),
        CONFLICT,
    ),
    (
        "parent_visit_id_differs",
        |e| {
            e.parent = BrowserParent::Visit {
                visit_id: "parent-2".into(),
            }
        },
        CONFLICT,
    ),
    (
        "parent_outside_range_with_same_visit_id",
        |e| {
            e.parent = BrowserParent::OutsideRange {
                visit_id: "parent-1".into(),
            }
        },
        DUPLICATE,
    ),
    (
        "parent_unknown_for_visit",
        |e| e.parent = BrowserParent::Unknown,
        CONFLICT,
    ),
    (
        "parent_root_for_visit",
        |e| e.parent = BrowserParent::Root,
        CONFLICT,
    ),
    (
        "original_unix_micros_differs",
        |e| e.original_unix_micros = Some(1_000_001),
        CONFLICT,
    ),
    (
        "original_unix_micros_absent",
        |e| e.original_unix_micros = None,
        CONFLICT,
    ),
    (
        "source_is_local_differs",
        |e| e.source_is_local = Some(false),
        CONFLICT,
    ),
    (
        "source_is_local_unknown",
        |e| e.source_is_local = None,
        CONFLICT,
    ),
    (
        "other_visit_id_is_a_new_encounter",
        |e| e.identity.visit_id = "visit-2".into(),
        NEW,
    ),
];

#[derive(Serialize)]
struct Case {
    name: &'static str,
    first: BrowserVisit,
    second: BrowserVisit,
    verdict: &'static str,
}

/// The visit as the extension queue stores and `capture.pull` delivers it.
fn queued(encounter: &BrowserEncounter) -> BrowserVisit {
    BrowserVisit {
        source: encounter.identity.scope.product.clone(),
        visit_id: Some(encounter.identity.visit_id.clone()),
        source_scope: Some(encounter.identity.scope.clone()),
        parent: encounter.parent.clone(),
        original_unix_micros: encounter.original_unix_micros,
        source_is_local: encounter.source_is_local,
        url: encounter.url.clone(),
        title: encounter.title.clone(),
        favicon_url: None,
        referrer_url: None,
        transition: encounter.raw_transition.clone(),
        at_ms: encounter.at_ms,
        private: false,
    }
}

fn ledger_verdict(prior: &BrowserEncounter, later: &BrowserEncounter) -> &'static str {
    let mut ledger = BrowserEncounterLedger::default();
    assert_eq!(ledger.receive(prior.clone(), 16), Ok(true));
    match ledger.receive(later.clone(), 16) {
        Ok(false) => DUPLICATE,
        Ok(true) => NEW,
        Err("conflicting browser encounter source facts") => CONFLICT,
        Err(other) => panic!("the ledger refused for another reason: {other}"),
    }
}

#[test]
fn extension_queue_conflict_fixture_carries_the_ledgers_verdicts() {
    let cases: Vec<Case> = CASES
        .iter()
        .map(|(name, change, expected)| {
            let prior = first();
            let mut later = prior.clone();
            change(&mut later);
            let verdict = ledger_verdict(&prior, &later);
            assert_eq!(
                verdict, *expected,
                "{name}: the ledger's rule moved; update mergeQueue in capture-model.js"
            );
            Case {
                name,
                first: queued(&prior),
                second: queued(&later),
                verdict,
            }
        })
        .collect();
    let rendered = serde_json::to_string_pretty(&cases).unwrap() + "\n";
    if std::env::var_os("GRAPHSHELL_WRITE_FIXTURES").is_some() {
        std::fs::create_dir_all(std::path::Path::new(FIXTURE).parent().unwrap()).unwrap();
        std::fs::write(FIXTURE, &rendered).unwrap();
    }
    let checked_in =
        std::fs::read_to_string(FIXTURE).expect("capture-conflicts.json is checked in");
    assert!(
        checked_in == rendered,
        "capture-conflicts.json differs from the ledger's verdicts; rerun with GRAPHSHELL_WRITE_FIXTURES=1 and re-run smoke-capture-model.mjs"
    );
}
