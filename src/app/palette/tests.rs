// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn version(seed: u8) -> crate::place::PlaceCollectionVersion {
    crate::place::PlaceCollectionVersion {
        moot: crate::place::PlaceId([1; 32]),
        collection: crate::place::PlaceCollectionId([seed; 32]),
        frontier: vec![[seed.wrapping_add(1); 32]],
        membership_commitment: [seed.wrapping_add(2); 32],
    }
}

fn offline(snapshot: crate::place::OfflinePlaceSnapshot) -> crate::place::PlaceState {
    crate::place::PlaceState::Offline {
        binding: crate::place::PlaceBindingV1::new(
            crate::place::PlaceId([1; 32]),
            crate::place::SharedContainerId([2; 32]),
            crate::place::ChatSpaceId([3; 32]),
            "hall",
        )
        .unwrap(),
        generation: 1,
        snapshot,
    }
}

#[test]
fn captured_collection_rows_preserve_worker_versions_and_mark_selection() {
    let first = version(10);
    let second = version(20);
    let mut app = App::test_stub();
    app.place = offline(crate::place::OfflinePlaceSnapshot {
        captured_selection: crate::place::CapturedCollectionSelection::Collection {
            requested: first.clone(),
            status: crate::place::CapturedCollectionSelectionStatus::Ready {
                name: "field notes".into(),
                effective_contributions: 1,
                pending_facts: 0,
            },
        },
        collection_choices: vec![
            crate::place::PlaceCollectionChoice {
                name: "field notes".into(),
                version: first.clone(),
            },
            crate::place::PlaceCollectionChoice {
                name: "research".into(),
                version: second.clone(),
            },
        ],
        ..Default::default()
    });

    let rows = app.available_actions();
    assert!(rows.contains(&("Search captured pages".into(), Action::SearchCapturedPages,)));
    assert!(rows.contains(&(
        "Use captured collection: all shared captures".into(),
        Action::SetPlaceCollection(None),
    )));
    assert!(rows.contains(&(
        "Use captured collection: field notes (selected)".into(),
        Action::SetPlaceCollection(Some(first)),
    )));
    assert!(rows.contains(&(
        "Use captured collection: research".into(),
        Action::SetPlaceCollection(Some(second)),
    )));
}

#[test]
fn stale_captured_collection_selection_offers_the_latest_version() {
    let requested = version(30);
    let mut current = requested.clone();
    current.frontier = vec![[33; 32]];
    current.membership_commitment = [34; 32];
    let mut app = App::test_stub();
    app.place = offline(crate::place::OfflinePlaceSnapshot {
        captured_selection: crate::place::CapturedCollectionSelection::Collection {
            requested: requested.clone(),
            status: crate::place::CapturedCollectionSelectionStatus::Stale {
                current: current.clone(),
            },
        },
        collection_choices: vec![crate::place::PlaceCollectionChoice {
            name: "field notes".into(),
            version: current.clone(),
        }],
        ..Default::default()
    });

    assert!(app.available_actions().contains(&(
        "Use captured collection: field notes (use latest version)".into(),
        Action::SetPlaceCollection(Some(current)),
    )));
}

#[test]
fn duplicate_collection_names_have_stable_distinct_palette_labels() {
    let first = version(10);
    let mut second = version(10);
    second.collection.0[4] = 11;
    let mut app = App::test_stub();
    app.place = offline(crate::place::OfflinePlaceSnapshot {
        collection_choices: vec![
            crate::place::PlaceCollectionChoice {
                name: "notes".into(),
                version: first.clone(),
            },
            crate::place::PlaceCollectionChoice {
                name: "notes".into(),
                version: second.clone(),
            },
        ],
        ..Default::default()
    });

    assert!(app.available_actions().contains(&(
        "Use captured collection: all shared captures (selected)".into(),
        Action::SetPlaceCollection(None),
    )));
    assert!(app.available_actions().contains(&(
        "Use captured collection: notes (0a0a0a0a0a)".into(),
        Action::SetPlaceCollection(Some(first)),
    )));
    assert!(app.available_actions().contains(&(
        "Use captured collection: notes (0a0a0a0a0b)".into(),
        Action::SetPlaceCollection(Some(second)),
    )));
}

#[test]
fn committing_a_captured_collection_row_emits_its_exact_version() {
    let selected = version(10);
    let mut app = App::test_stub();
    app.place = offline(crate::place::OfflinePlaceSnapshot {
        collection_choices: vec![crate::place::PlaceCollectionChoice {
            name: "field notes".into(),
            version: selected.clone(),
        }],
        ..Default::default()
    });

    app.update(Action::OmnibarOpen { command: true });
    app.update(Action::OmnibarInsert("field notes".into()));
    assert!(app.omnibar.suggestions.iter().any(|row| {
        matches!(
            row,
            crate::ui::Suggestion::Act { label, action }
                if label == "Use captured collection: field notes"
                    && action == &Action::SetPlaceCollection(Some(selected.clone()))
        )
    }));
    let effects = app.update(Action::OmnibarCommit);

    assert!(effects.contains(&Effect::SetPlaceCollection {
        session: app.session_id,
        generation: 1,
        selection: Some(selected),
    }));
    assert!(!app.omnibar.open, "the palette closes on commit");
}
