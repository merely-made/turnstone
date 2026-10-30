// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::action::{Action, Effect, ResearchSearchAction as Command};
use crate::research_search::Request;
use crate::ui::Suggestion;

fn app_with_search(root: &std::path::Path) -> App {
    let mut app = App::test_stub_at(root.to_path_buf());
    let custody = crate::trail_memory::custody_test_fixture::custody();
    app.research.set_available(true);
    app.research.access(Some(crate::research::ClipAccess {
        root: root.to_path_buf(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    app
}
fn request(effects: Vec<Effect>) -> Request {
    effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::ResearchSearch(request) => Some(request),
            _ => None,
        })
        .expect("search requested")
}
#[test]
fn model_free_one_row_controls_and_private_query_reach_options_and_back() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_search(root.path());
    app.update(Action::SearchSavedSources);
    app.recompute_saved_source_suggestions(1);
    assert!(matches!(
        &app.omnibar.suggestions[0],
        Suggestion::Act {
            action: Action::ResearchSearch(Command::Options),
            ..
        }
    ));
    app.update(Action::OmnibarCommit);
    assert_eq!(
        app.research_search.view,
        super::research_search_state::View::Options
    );
    app.recompute_saved_source_suggestions(1);
    for _ in 0..6 {
        app.update(Action::OmnibarMove(1));
        app.recompute_saved_source_suggestions(1);
    }
    assert!(matches!(
        &app.omnibar.suggestions[0],
        Suggestion::Act {
            action: Action::ResearchSearch(Command::Results),
            ..
        }
    ));
    app.update(Action::OmnibarCommit);
    assert_eq!(
        app.research_search.view,
        super::research_search_state::View::Results
    );
}
#[test]
fn model_free_stale_result_and_custody_loss_are_discarded() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_search(root.path());
    let old = request(app.update(Action::SearchSavedSources));
    let snapshot = eidetic_search::query::ResearchSnapshot {
        generation: "test".into(),
        scope: vec![],
        sources: vec![],
        coverage: eidetic_search::query::Coverage::Complete,
    };
    let result = eidetic_search::query::search(&snapshot, &old.query, &old.settings).unwrap();
    let mut removed = old.context.clone();
    removed.generation = uuid::Uuid::new_v4();
    app.research.project(removed);
    app.research.project(old.context.clone());
    app.research_search_finished(old.clone(), Ok(result.clone()));
    assert!(app.research_search.result.is_none());
    let next = request(app.issue_research_search());
    app.research_search_finished(next, Ok(result));
    assert!(app.research_search.result.is_some());
    app.research.set_available(false);
    let _ = app.invalidate_research_search();
    assert!(app.research_search.result.is_none());
}
#[test]
fn model_free_settings_and_dates_validate_and_preserve_bad_file() {
    use super::research_search_state::{Preferences, State};
    use super::research_search_values::apply_value;
    let root = tempfile::tempdir().unwrap();
    let mut state = State::default();
    apply_value(&mut state, root.path(), "limit", "7").unwrap();
    apply_value(&mut state, root.path(), "aliases", "bird = avian, winged").unwrap();
    let bytes = std::fs::read_to_string(root.path().join("research-search-settings.json")).unwrap();
    assert!(!bytes.contains("offset"));
    assert_eq!(Preferences::load(root.path()).unwrap().settings.limit, 7);
    apply_value(&mut state, root.path(), "compare_max_diff_work", "42").unwrap();
    assert_eq!(
        Preferences::load(root.path())
            .unwrap()
            .comparison
            .max_diff_work,
        42
    );
    let path = root.path().join("research-search-settings.json");
    let mut old: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    old.as_object_mut().unwrap().remove("comparison");
    std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(
        Preferences::load(root.path()).unwrap().comparison,
        eidetic_search::compare::ComparisonSettings::default()
    );
    apply_value(&mut state, root.path(), "date_from", "2026-02-03").unwrap();
    apply_value(&mut state, root.path(), "date_to", "2026-02-03").unwrap();
    assert_eq!(
        state.query.date_to.unwrap() - state.query.date_from.unwrap(),
        86_399_999
    );
    assert!(apply_value(&mut state, root.path(), "date_to", "2026-02-30").is_err());
    assert!(apply_value(&mut state, root.path(), "weight_text", "NaN").is_err());
    assert!(apply_value(&mut state, root.path(), "aliases", "bird = ").is_err());
    let malformed = b"{ not valid";
    std::fs::write(root.path().join("research-search-settings.json"), malformed).unwrap();
    assert!(Preferences::load(root.path()).is_err());
    assert!(apply_value(&mut state, root.path(), "limit", "9").is_err());
    assert_eq!(
        std::fs::read(root.path().join("research-search-settings.json")).unwrap(),
        malformed
    );
    assert!(
        apply_value(
            &mut state,
            root.path(),
            "storage_recovery_value_bytes",
            "8192"
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), malformed);

    old["version"] = serde_json::json!(2);
    let newer = serde_json::to_vec(&old).unwrap();
    std::fs::write(&path, &newer).unwrap();
    assert!(
        Preferences::load(root.path())
            .err()
            .unwrap()
            .contains("unsupported")
    );
    assert!(
        apply_value(
            &mut state,
            root.path(),
            "storage_recovery_value_bytes",
            "8192"
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), newer);
}

#[test]
fn model_free_lifecycle_clears_private_query_and_prompt_before_refusal_or_lock() {
    for prompt in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_search(root.path());
        app.update(Action::SearchSavedSources);
        app.update(Action::OmnibarInsert("private query canary".into()));
        if prompt {
            app.update(Action::ResearchSearch(Command::Input("aliases".into())));
            app.update(Action::OmnibarInsert("private value canary".into()));
        }
        app.retire_private_custody();
        assert!(!app.omnibar.open);
        assert!(app.omnibar.text.is_empty() && app.omnibar.suggestions.is_empty());
        assert!(app.research_search.original_text.is_empty());
        assert!(app.research_search.rows.is_empty());
    }
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_search(root.path());
    app.update(Action::SearchSavedSources);
    app.update(Action::OmnibarInsert("previous session canary".into()));
    app.update(Action::ResearchSearch(Command::Input("aliases".into())));
    app.update(Action::OmnibarInsert("previous prompt canary".into()));
    let session = crate::panes::SessionId::new();
    let directory = crate::session::session_dir(root.path(), session);
    let _held = crate::session_persistence::SessionWriteLease::acquire(&directory).unwrap();
    app.adopt_session(session);
    assert!(app.session_load_refused());
    assert!(!app.omnibar.open);
    assert!(app.omnibar.text.is_empty() && app.omnibar.suggestions.is_empty());
    assert!(app.research_search.original_text.is_empty());
}

#[test]
fn model_free_compare_control_reachable_with_one_visible_row() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_search(root.path());
    app.update(Action::SearchSavedSources);
    app.recompute_saved_source_suggestions(1);
    let mut found = false;
    for _ in 0..app.research_search.rows.len() {
        if let Some(Suggestion::Act { label, .. }) = app.omnibar.suggestions.first() {
            found |= label == "Compare sources…";
        }
        app.update(Action::OmnibarMove(1));
        app.recompute_saved_source_suggestions(1);
    }
    assert!(
        found,
        "Saved Sources must offer comparison with one visible row"
    );
}
