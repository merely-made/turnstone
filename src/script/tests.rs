// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use crate::behaviors::TriggerContext;

#[test]
fn a_behavior_reports_exactly_one_signed_output() {
    let app = App::test_stub();
    let run = run_behavior(
        &app,
        "mere.output(-42)",
        ScriptCapabilities::control(),
        200,
        &TriggerContext::default(),
    )
    .unwrap();
    assert_eq!(run.output, Some(-42));
    assert!(run.actions.is_empty());

    let error = run_behavior(
        &app,
        "mere.output(1); mere.output(2)",
        ScriptCapabilities::control(),
        200,
        &TriggerContext::default(),
    )
    .unwrap_err();
    assert!(error.contains("only once"), "refused by name: {error}");
}

/// A body can read what woke it, and act only when something did.
#[test]
fn a_woken_body_reads_its_trigger_and_a_hand_run_sees_an_empty_one() {
    use crate::behaviors::TriggerEntry;

    let app = App::test_stub();
    // The same script both times: dispatch only if something woke us.
    // Compared against the empty wire form rather than searched, because
    // the sandbox carries no `string` library; the exact form is pinned by
    // `an_unwoken_context_is_empty_rather_than_absent`, so the two tests
    // fail together if it ever changes.
    let source = "local t = mere.trigger()
                      if t ~= '{\"woken_by\":[]}' then mere.dispatch('save_session') end";

    let woken = TriggerContext {
        woken_by: vec![TriggerEntry {
            seq: 7,
            author: "user".into(),
            nodes: vec!["n1".into()],
        }],
    };
    let acted =
        run(&app, source, ScriptCapabilities::control(), 500, &woken).expect("the woken run");
    assert_eq!(acted.len(), 1, "the body acted on what woke it");

    let idle = run(
        &app,
        source,
        ScriptCapabilities::control(),
        500,
        &TriggerContext::default(),
    )
    .expect("the hand-invoked run");
    assert!(
        idle.is_empty(),
        "invoked by hand, the same body finds nothing to answer"
    );
}

/// A body cannot read wall time. Scheduling is the host's to decide and
/// feed in, so a replayed session wakes the same behaviors at the same
/// points; a body sampling its own clock would make that untrue and
/// nothing would catch it.
#[test]
fn a_body_has_no_way_to_read_the_clock() {
    let app = App::test_stub();
    for probe in ["os.time()", "os.clock()", "mere.now()"] {
        let err = run(
            &app,
            probe,
            ScriptCapabilities::control(),
            200,
            &TriggerContext::default(),
        )
        .unwrap_err();
        assert!(
            err.contains("nil") || err.contains("failed"),
            "{probe} should not resolve: {err}"
        );
    }
}

/// The digest is gated like the snapshot: it describes the graph.
#[test]
fn reading_the_trigger_needs_the_read_capability() {
    let app = App::test_stub();
    let err = run(
        &app,
        "mere.trigger()",
        // Every capability except reading the app.
        ScriptCapabilities::without_read(),
        200,
        &TriggerContext::default(),
    )
    .unwrap_err();
    assert!(err.contains("app.read"), "refused by name: {err}");
}
use super::*;

/// B2: capabilities derive from the participant's grant. A subject granted
/// only its own world evaluates without the app classes (the denial is
/// the capability system's, by name); a subject granted `app/` runs.
#[test]
fn grant_derived_capabilities_deny_the_ungranted() {
    use servitor::{Cap, Grant, GrantTable, Mode, Subject};
    let app = App::test_stub();
    let subject = Subject::new([9; 32]);

    let world_only = GrantTable::new().with_grant(Grant::new(
        subject,
        Cap::scope("scenario").unwrap(),
        Mode::Write,
    ));
    let caps = capabilities_from_grant(&world_only, subject);
    let err = run(
        &app,
        "mere.open('mere://x')",
        caps,
        500,
        &TriggerContext::default(),
    )
    .unwrap_err();
    assert!(
        err.contains("navigation.open"),
        "the ungranted class denies by name: {err}"
    );

    // Every ring at once: the fully-granted script. Since rings became
    // powers there is nothing above them to hold, so a total grant is
    // spelled by enumerating them, which is exactly the property that
    // stops a later ring from widening an earlier grant.
    let mut control = GrantTable::new();
    for ring in crate::ring::GRANTABLE_RINGS {
        control.grant(Grant::new(subject, ring.cap().unwrap(), Mode::Write));
    }
    let caps = capabilities_from_grant(&control, subject);
    let actions = run(
        &app,
        "mere.open('mere://x')",
        caps,
        500,
        &TriggerContext::default(),
    )
    .unwrap();
    assert_eq!(actions.len(), 1, "the granted surface runs");
}

#[test]
fn script_can_read_snapshot_without_mutation() {
    let app = App::test_stub();
    let actions = run(
        &app,
        "assert(type(mere.snapshot()) == 'string')",
        ScriptCapabilities::read_only(),
        200,
        &TriggerContext::default(),
    )
    .unwrap();
    assert!(actions.is_empty());
}

#[test]
fn snapshot_json_contains_graph_and_focus_fields() {
    let snapshot = AppSnapshot::from_app(&App::test_stub()).to_json();
    assert!(snapshot.contains("\"graph_nodes\""));
    assert!(snapshot.contains("\"focus\""));
}

#[test]
fn script_emits_typed_navigation_and_pane_actions() {
    let app = App::test_stub();
    let actions = run(
        &app,
        "mere.open('https://example.test'); mere.summon('roster'); mere.dispatch('save_session')",
        ScriptCapabilities::control(),
        500,
        &TriggerContext::default(),
    )
    .unwrap();
    assert_eq!(
        actions,
        vec![
            Action::OpenAddress("https://example.test".into()),
            Action::SummonPane(PaneKindId::new(kind::ROSTER)),
            Action::SaveSession,
        ]
    );
}

#[test]
fn denied_capability_does_not_emit_an_action() {
    let app = App::test_stub();
    let err = run(
        &app,
        "mere.dispatch('save_session')",
        ScriptCapabilities::read_only(),
        200,
        &TriggerContext::default(),
    )
    .unwrap_err();
    assert!(err.contains("action.dispatch"), "unexpected error: {err}");
}

/// One description, two runners (the deletion-matrix row): a Piccolo
/// script and the equivalent keyboard-Action sequence, both lowered
/// through `App::update`, reach the SAME observed state. This is the
/// automation runner proving it lowers to the same spine as the
/// keyboard/scenario runner — doctrine 2 in the small.
#[test]
fn the_automation_runner_matches_the_keyboard_runner() {
    use crate::observe::snapshot;

    // Runner A: the keyboard/scenario path — ordinary Actions.
    let mut by_keyboard = App::test_stub();
    for action in [
        Action::OpenAddress("mere://alpha".into()),
        Action::SummonPane(PaneKindId::new(kind::ROSTER)),
        Action::ToggleIsometric,
    ] {
        by_keyboard.update(action);
    }

    // Runner B: the Piccolo automation path — a script emitting the same
    // intents, applied through the identical spine.
    let mut by_script = App::test_stub();
    let actions = run_control(
        &by_script,
        "mere.open('mere://alpha'); mere.summon('roster'); \
             mere.dispatch('toggle_isometric')",
        500,
    )
    .expect("the control script runs");
    for action in actions {
        by_script.update(action);
    }

    // The two runners agree on the observable state (focused node, panes,
    // view mode) — the whole point of "one description, two runners".
    let a = snapshot(&by_keyboard);
    let b = snapshot(&by_script);
    assert_eq!(a.focused.map(|f| f.url), b.focused.map(|f| f.url));
    assert_eq!(a.panes, b.panes);
    assert_eq!(
        by_keyboard.graph_runtimes.is_isometric(),
        by_script.graph_runtimes.is_isometric()
    );
    assert!(
        by_script.graph_runtimes.is_isometric(),
        "both reached isometric"
    );
}

/// The new lanes are reachable from automation: nav, workbench, window.
#[test]
fn automation_reaches_nav_workbench_and_window_actions() {
    let app = App::test_stub();
    let actions = run_control(
        &app,
        "mere.dispatch('back'); mere.dispatch('reload'); \
             mere.dispatch('open_in_workbench'); mere.dispatch('new_window')",
        500,
    )
    .unwrap();
    assert_eq!(
        actions,
        vec![
            Action::NavBack,
            Action::Reload,
            Action::OpenInWorkbench,
            Action::NewWindow,
        ]
    );
}

#[test]
fn runaway_script_hits_the_step_budget() {
    let app = App::test_stub();
    let err = run(
        &app,
        "while true do end",
        ScriptCapabilities::read_only(),
        20,
        &TriggerContext::default(),
    )
    .unwrap_err();
    assert!(err.contains("budget"), "unexpected error: {err}");
}
