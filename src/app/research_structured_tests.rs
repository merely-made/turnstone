// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::research_structured_state::{Draft, parse_interval, parse_value};
use eidetic::records::{PropertyValue, ValueType};

#[test]
fn model_free_structured_unknown_interval_is_not_current() {
    assert_eq!(parse_interval("", "").unwrap(), None);
    assert!(parse_interval("2026-01-01T00:00:00", "2026-01-02T00:00:00Z").is_err());
    assert!(parse_interval("2026-01-01T00:00:00Z", "").is_err());
}

#[test]
fn model_free_structured_values_are_typed_and_explicit() {
    assert_eq!(
        parse_value(&ValueType::Boolean, "false", "").unwrap(),
        PropertyValue::Boolean(false)
    );
    assert!(parse_value(&ValueType::Boolean, "maybe", "").is_err());
    assert_eq!(
        parse_value(&ValueType::Decimal, "-12.50", "kg").unwrap(),
        PropertyValue::Decimal {
            coefficient: -1250,
            scale: 2,
            unit: "kg".into()
        }
    );
    assert!(parse_value(&ValueType::Decimal, "1.5", "").is_err());
}

#[test]
fn model_free_structured_draft_requires_exact_identity_and_evidence() {
    let draft = Draft::default();
    assert!(draft.property().is_err());
}

#[test]
fn model_free_structured_settings_survive_and_old_settings_default() {
    use super::research_search_state::{Preferences, State};
    use super::research_search_values::apply_value;
    let root = tempfile::tempdir().unwrap();
    let mut state = State::default();
    apply_value(&mut state, root.path(), "record_max_records", "77").unwrap();
    assert_eq!(
        Preferences::load(root.path()).unwrap().records.max_records,
        77
    );
    let path = root.path().join("research-search-settings.json");
    let mut stored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    stored.as_object_mut().unwrap().remove("records");
    std::fs::write(&path, serde_json::to_vec(&stored).unwrap()).unwrap();
    assert_eq!(
        Preferences::load(root.path()).unwrap().records,
        eidetic::records::RecordSettings::default()
    );
}

#[test]
fn model_free_structured_single_row_route_and_custody_clear() {
    use crate::action::{Action, ResearchSearchAction, ResearchStructuredAction};
    use crate::ui::Suggestion;
    let root = tempfile::tempdir().unwrap();
    let mut app = super::App::test_stub_at(root.path().to_path_buf());
    let custody = crate::trail_memory::custody_test_fixture::custody();
    app.research.set_available(true);
    app.research.access(Some(crate::research::ClipAccess {
        root: root.path().to_path_buf(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    app.update(Action::SearchSavedSources);
    app.update(Action::ResearchSearch(ResearchSearchAction::Structured));
    app.recompute_saved_source_suggestions(1);
    assert!(matches!(
        app.omnibar.suggestions.first(),
        Some(Suggestion::Act {
            action: Action::ResearchStructured(ResearchStructuredAction::Operations),
            ..
        })
    ));
    app.update(Action::OmnibarCommit);
    assert_eq!(
        app.research_search.structured.view,
        super::research_structured_state::View::Operations
    );
    app.update(Action::ResearchStructured(ResearchStructuredAction::Author));
    app.update(Action::ResearchStructured(ResearchStructuredAction::Input(
        "value".into(),
    )));
    app.update(Action::OmnibarInsert("private structured canary".into()));
    app.retire_private_custody();
    assert!(!app.omnibar.open);
    assert!(app.omnibar.text.is_empty());
    assert!(app.research_search.structured.draft.value.is_empty());
}

#[test]
fn model_free_group_rows_distinguish_entity_status_and_unknown_time() {
    use super::research_structured_rows::format::group_fields;
    use eidetic::records::{PropertyGroupKey, PropertyValue, TimeInterval};
    let first = PropertyGroupKey {
        entity: "entity-a".into(),
        property: "example.mass".into(),
        schema_revision: "1".into(),
        context: "deck".into(),
        status: None,
        valid_time: None,
        value: PropertyValue::Integer(12),
    };
    let mut second = first.clone();
    second.entity = "entity-b".into();
    second.status = Some("approved".into());
    second.valid_time = Some(TimeInterval { start: 10, end: 20 });
    let first_fields = group_fields(&first);
    let second_fields = group_fields(&second);
    assert_ne!(first_fields, second_fields);
    assert!(
        first_fields
            .iter()
            .any(|(k, v)| *k == "Status" && v == "unknown")
    );
    assert!(
        first_fields
            .iter()
            .any(|(k, v)| *k == "Valid time" && v == "unknown")
    );
    assert!(
        second_fields
            .iter()
            .any(|(k, v)| *k == "Entity" && v == "entity-b")
    );
    assert!(
        second_fields
            .iter()
            .any(|(k, v)| *k == "Status" && v == "approved")
    );
    assert!(
        second_fields
            .iter()
            .any(|(k, v)| *k == "Valid time" && v == "10 to 20 UTC seconds (end exclusive)")
    );
}

#[test]
fn model_free_recovery_is_reachable_and_refresh_is_explicit() {
    use crate::action::{Action, ResearchSearchAction, ResearchStructuredAction};
    use crate::ui::Suggestion;
    let root = tempfile::tempdir().unwrap();
    let mut app = super::App::test_stub_at(root.path().to_path_buf());
    let custody = crate::trail_memory::custody_test_fixture::custody();
    app.research.set_available(true);
    app.research.access(Some(crate::research::ClipAccess {
        root: root.path().to_path_buf(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    app.update(Action::SearchSavedSources);
    app.update(Action::ResearchSearch(ResearchSearchAction::Structured));
    app.update(Action::ResearchStructured(
        ResearchStructuredAction::Recovery,
    ));
    app.recompute_saved_source_suggestions(1);
    assert!(matches!(
        app.omnibar.suggestions.first(),
        Some(Suggestion::Act {
            action: Action::ResearchStructured(ResearchStructuredAction::RefreshRecovery),
            ..
        })
    ));
}

#[test]
fn model_free_only_unbound_orphans_offer_set_aside() {
    use super::research_structured_rows::format::recovery_rows;
    use crate::action::{Action, ResearchStructuredAction as Command};
    use crate::ui::Suggestion;
    let orphan = uuid::Uuid::new_v4();
    let bound = uuid::Uuid::new_v4();
    let mut rows = vec![];
    recovery_rows(
        &mut rows,
        [
            (orphan, "property", "unbound", true),
            (bound, "property", "pending", false),
        ]
        .into_iter(),
        0,
        10,
        2,
    );
    let set_aside: Vec<_> = rows
        .iter()
        .filter_map(|row| match row {
            Suggestion::Act {
                action: Action::ResearchStructured(Command::AbandonRecovery(nonce)),
                ..
            } => Some(*nonce),
            _ => None,
        })
        .collect();
    assert_eq!(set_aside, vec![orphan]);
}

#[test]
fn model_free_preview_reads_only_the_bounded_prefix() {
    use super::research_structured_rows::format::clip_chars;
    use std::cell::Cell;
    let visited = Cell::new(0);
    let characters =
        std::iter::repeat_n('x', 1_000_000).inspect(|_| visited.set(visited.get() + 1));
    assert_eq!(clip_chars(characters, 8), "xxxxxxxx…");
    assert_eq!(visited.get(), 9);
}

#[test]
fn model_free_structured_scope_distinguishes_all_from_empty_selection() {
    use crate::action::{Action, ResearchStructuredAction as Command};
    use crate::ui::Suggestion;
    let root = tempfile::tempdir().unwrap();
    let mut app = super::App::test_stub_at(root.path().to_path_buf());
    app.research_search.selected = None;
    app.research_structured_rows(100);
    assert!(app.research_search.rows.iter().any(|r| matches!(r, Suggestion::Act { label, action: Action::ResearchStructured(Command::Scope), .. } if label == "Scope: All admitted sources")));
    app.research_search.selected = Some(vec![]);
    app.research_structured_rows(100);
    assert!(app.research_search.rows.iter().any(|r| matches!(r, Suggestion::Act { label, action: Action::ResearchStructured(Command::Scope), .. } if label == "Scope: No sources chosen")));
    app.research_structured_command(Command::Scope);
    assert_eq!(app.research_search.selected, Some(vec![]));
    assert_eq!(
        app.research_search.view,
        super::research_search_state::View::Sources
    );
}
