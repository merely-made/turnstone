// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{ALL_COMMANDS_ROW, DEFAULT_KEPT_COMMANDS};
use crate::action::{Action, Effect};
use crate::app::App;
use crate::observe::AppEvent;
use crate::ui::Suggestion;

fn labels(rows: &[(String, Action)]) -> Vec<&str> {
    rows.iter().map(|(label, _)| label.as_str()).collect()
}

/// The lane for `query`, bare or expanded, as the omnibar's session asks.
fn lane(app: &App, query: &str, all: bool, rows: usize) -> Vec<(String, Action)> {
    let mut menu = cambium::MenuSession::open(Some(super::CONTEXT_COMMANDS));
    menu.set_query(query);
    if all {
        menu.expand_all();
    }
    app.command_lane(&menu, rows)
}

fn contextual(app: &App) -> Vec<String> {
    let catalog = app.available_actions();
    let statics = crate::action::palette_actions().len();
    catalog[..catalog.len() - statics]
        .iter()
        .map(|(label, _)| label.clone())
        .collect()
}

#[test]
fn offered_actions_carry_the_hosts_effect_and_destructive_declarations() {
    use cambium::{EffectClass, catalogue::ids};
    let app = App::test_stub();
    let catalog = app.command_catalog();
    // Exercise actual static and contextual registrations, including the
    // fallback constructor for Turnstone-owned ids.
    assert!(!catalog.actions.is_empty());
    for (id, action) in &catalog.actions {
        let offered = catalog.set.get(id).expect("each action is offered");
        assert_eq!(
            (offered.effect, offered.destructive),
            crate::action::command_metadata(action),
            "metadata must follow the exact offered Action: {id}"
        );
    }
    let delete = catalog.set.get(ids::NODE_DELETE).unwrap();
    assert_eq!(
        (delete.effect, delete.destructive),
        (EffectClass::DomainTruth, true)
    );
    let fit = catalog.set.get(ids::VIEW_FIT).unwrap();
    assert_eq!(
        (fit.effect, fit.destructive),
        (EffectClass::Curation, false)
    );
    let save = catalog.set.get(ids::SESSION_SAVE).unwrap();
    assert_eq!(
        (save.effect, save.destructive),
        (EffectClass::ExternalEffect, false)
    );
    assert_eq!(
        crate::action::command_metadata(&Action::EmptyRecycleBin),
        (EffectClass::DomainTruth, true)
    );
    assert_eq!(
        crate::action::command_metadata(&Action::CloseSession),
        (EffectClass::DomainTruth, true)
    );
}

#[test]
fn a_bare_lane_shows_context_then_kept_then_all_commands() {
    let app = App::test_stub();
    let lane = lane(&app, "", false, 100);
    let mut expected = contextual(&app);
    expected.extend(
        DEFAULT_KEPT_COMMANDS
            .iter()
            .map(|id| cambium::catalogue::label(id).unwrap_or(*id).to_string()),
    );
    expected.push(ALL_COMMANDS_ROW.to_string());
    assert_eq!(labels(&lane), expected);
    assert!(
        !labels(&lane).contains(&"Reseed layout"),
        "a command that is not kept waits for search or All commands"
    );
    assert_eq!(lane.last().unwrap().1, Action::OmnibarShowAllCommands);
}

#[test]
fn the_all_commands_row_survives_a_short_lane() {
    let app = App::test_stub();
    let lane = lane(&app, "", false, 4);
    assert_eq!(lane.len(), 4);
    assert_eq!(lane.last().unwrap().0, ALL_COMMANDS_ROW);
}

#[test]
fn search_reaches_every_command() {
    let app = App::test_stub();
    let lane = lane(&app, "reseed", false, 100);
    assert_eq!(labels(&lane), ["Reseed layout"]);
    assert!(
        !labels(&lane).contains(&ALL_COMMANDS_ROW),
        "the expanding row belongs to the bare lane"
    );
}

#[test]
fn keeping_and_dropping_change_the_lane_and_save() {
    let mut app = App::test_stub();
    let effects = app.toggle_command_kept("Reseed layout");
    assert!(effects.contains(&Effect::SaveSession));
    assert!(app.events.iter().any(|event| matches!(
        event,
        AppEvent::CommandKept { label, kept: true } if label == "Reseed layout"
    )));
    assert!(labels(&lane(&app, "", false, 100)).contains(&"Reseed layout"));
    assert_eq!(
        app.command_choices.added,
        ["Reseed layout"],
        "a host-owned id is its label"
    );

    app.toggle_command_kept("Fit to view");
    let shown = lane(&app, "", false, 100);
    assert!(
        !labels(&shown).contains(&"Fit to view"),
        "a default can be dropped"
    );
    assert_eq!(
        app.command_choices.removed,
        ["view:fit"],
        "stored by its shared id"
    );

    app.toggle_command_kept("Fit to view");
    assert!(labels(&lane(&app, "", false, 100)).contains(&"Fit to view"));
    assert!(app.command_choices.removed.is_empty());

    let before = app.command_choices.clone();
    assert_eq!(
        app.toggle_command_kept("Not a command"),
        vec![Effect::Redraw]
    );
    assert_eq!(
        app.command_choices, before,
        "only registered commands are kept"
    );
}

#[test]
fn a_command_run_from_the_palette_becomes_recent() {
    let mut app = App::test_stub();
    app.update(Action::OmnibarOpen { command: true });
    app.update(Action::OmnibarInsert("reseed".into()));
    let selected = app
        .omnibar
        .suggestions
        .iter()
        .position(|row| matches!(row, Suggestion::Act { label, .. } if label == "Reseed layout"))
        .expect("search finds it");
    app.update(Action::OmnibarCommitRow(selected));
    assert_eq!(app.command_choices.recent, ["Reseed layout"]);
    let lane = lane(&app, "", false, 100);
    let position = |label: &str| labels(&lane).iter().position(|row| *row == label);
    assert!(
        position("Reseed layout") > position("Save session"),
        "recent commands follow the kept ones"
    );
}

#[test]
fn all_commands_expands_the_lane_and_keeps_the_palette_open() {
    let mut app = App::test_stub();
    app.update(Action::OmnibarOpen { command: true });
    let last = app.omnibar.suggestions.len() - 1;
    assert!(matches!(
        &app.omnibar.suggestions[last],
        Suggestion::Act { label, .. } if label == ALL_COMMANDS_ROW
    ));
    app.update(Action::OmnibarCommitRow(last));
    assert!(app.omnibar.open, "expanding is not a command");
    assert!(app.omnibar.menu.all);
    // The whole catalog in its composed order, bounded by the row limit as
    // the bare lane always was; typing narrows it.
    let shown: Vec<String> = app
        .omnibar
        .suggestions
        .iter()
        .map(|row| match row {
            Suggestion::Act { label, .. } => label.clone(),
            other => panic!("an expanded lane lists commands: {other:?}"),
        })
        .collect();
    let catalog: Vec<String> = app
        .available_actions()
        .into_iter()
        .map(|(label, _)| label)
        .take(shown.len())
        .collect();
    assert_eq!(shown, catalog);
    app.update(Action::OmnibarInsert("reseed".into()));
    assert!(
        app.omnibar
            .suggestions
            .iter()
            .any(|row| matches!(row, Suggestion::Act { label, .. } if label == "Reseed layout"))
    );
    app.update(Action::OmnibarClose);
    app.update(Action::OmnibarOpen { command: true });
    assert!(
        !app.omnibar.menu.all,
        "closing returns to the person's commands"
    );
}

#[test]
fn the_highlighted_command_toggles_with_ctrl_d() {
    let mut app = App::test_stub();
    app.update(Action::OmnibarOpen { command: true });
    app.update(Action::OmnibarInsert("reseed".into()));
    app.omnibar.selected = app
        .omnibar
        .suggestions
        .iter()
        .position(|row| matches!(row, Suggestion::Act { label, .. } if label == "Reseed layout"))
        .unwrap();
    app.update(Action::OmnibarToggleKeepSelected);
    assert_eq!(app.command_choices.added, ["Reseed layout"]);
    assert!(app.omnibar.open, "keeping does not run or close");
}

/// The canvas, not Turnstone, decides what a right press was: a click
/// within the slop asks for the menu (naming the node under it), and a
/// right-drag past it selects and asks for nothing (SE26).
#[test]
fn a_right_click_asks_for_the_menu_and_a_right_drag_does_not() {
    use mere::canvas::PointerButton;
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("mere://alpha".into()));
    let pane = app.default_graph_pane();
    app.graph_pane_pointer_down(pane, PointerButton::Right, 40.0, 40.0);
    app.graph_pane_pointer_up(pane, PointerButton::Right, 41.0, 40.0);
    let request = app
        .graph_pane_take_context_request(pane)
        .expect("a right click within the slop asks for the menu");
    assert_eq!(request.at, (41.0, 40.0));
    assert!(
        app.graph_pane_take_context_request(pane).is_none(),
        "taken once"
    );

    app.graph_pane_pointer_down(pane, PointerButton::Right, 40.0, 40.0);
    app.graph_pane_cursor_moved(pane, 140.0, 120.0);
    app.graph_pane_pointer_up(pane, PointerButton::Right, 140.0, 120.0);
    assert!(
        app.graph_pane_take_context_request(pane).is_none(),
        "a right-drag is a selection"
    );
}

#[test]
fn palette_rows_and_their_keep_controls_are_accessible() {
    let mut app = App::test_stub();
    app.update(Action::OmnibarOpen { command: true });
    let lines = crate::a11y::tree_lines(&crate::a11y::project_app(&app));
    assert!(
        lines
            .iter()
            .any(|line| line.contains("button") && line.contains("Drop Back")),
        "a kept command offers Drop: {lines:#?}"
    );
    assert!(lines.iter().any(|line| line.contains(ALL_COMMANDS_ROW)));

    let (_, routes) = crate::a11y::project_app_with_routes(&app);
    let back = app
        .omnibar
        .suggestions
        .iter()
        .position(|row| matches!(row, Suggestion::Act { label, .. } if label == "Back"))
        .unwrap();
    let target = uxtree::node_id_for_path(&format!("turnstone/chrome/omnibar/row/{back}/keep"));
    crate::a11y::apply_route(&mut app, routes.get(&target), target);
    assert_eq!(
        app.command_choices.removed,
        ["nav:back"],
        "the assistive action drops it"
    );
}

/// The verbs the stack shares register under the catalogue's ids and
/// labels (SE49, SE50); Turnstone's own keep their label as their id.
#[test]
fn shared_verbs_register_under_catalogue_ids() {
    use cambium::catalogue::{self, ids};
    let app = App::test_stub();
    let catalog = app.command_catalog();
    for id in [
        ids::NAV_BACK,
        ids::NAV_FORWARD,
        ids::NAV_RELOAD,
        ids::NAV_STOP,
        ids::VIEW_FIT,
        ids::PHYSICS_TOGGLE,
        ids::NODE_DELETE,
        ids::SESSION_SAVE,
        ids::PANE_SETTINGS,
        ids::PANE_TRAIL,
        ids::PANE_WORKBENCH,
    ] {
        let command = catalog
            .set
            .get(id)
            .unwrap_or_else(|| panic!("{id} is offered"));
        assert_eq!(Some(command.label.as_str()), catalogue::label(id));
        assert_eq!(command.category, catalogue::namespace(id));
        assert_eq!(catalog.id_of(&command.label), Some(id));
    }
    assert_eq!(catalog.id_of("Reseed layout"), Some("Reseed layout"));
    assert!(
        app.available_actions()
            .iter()
            .any(|(label, action)| label == "Fit to view" && *action == Action::FitView),
        "the runner resolves the catalogue's label"
    );
}

/// A choice stored under a shared verb's old label falls away: no alias
/// table maps it (SE50).
#[test]
fn a_label_keyed_choice_for_a_shared_verb_falls_away() {
    let mut app = App::test_stub();
    app.command_choices.removed = vec!["Fit view".into()];
    assert!(labels(&lane(&app, "", false, 100)).contains(&"Fit to view"));
}

/// The highlight moves through the lane's menu session and wraps.
#[test]
fn the_lane_highlight_moves_through_its_menu_session() {
    let mut app = App::test_stub();
    app.update(Action::OmnibarOpen { command: true });
    let rows = app.omnibar.suggestions.len();
    assert!(rows > 1);
    app.update(Action::OmnibarMove(-1));
    assert_eq!(
        app.omnibar.selected,
        rows - 1,
        "up from the first row wraps"
    );
    assert_eq!(app.omnibar.menu.selected, Some(rows - 1));
    app.update(Action::OmnibarMove(1));
    assert_eq!(app.omnibar.selected, 0);
    app.update(Action::OmnibarInsert("reseed".into()));
    assert_eq!(app.omnibar.menu.query, "reseed");
    assert_eq!(
        app.omnibar.menu.selected, None,
        "a new query starts the highlight over"
    );
}
