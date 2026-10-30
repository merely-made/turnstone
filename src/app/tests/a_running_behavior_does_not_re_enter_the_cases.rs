// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A body's Actions lower through `update`, which ends in the drain, so
/// running a behavior re-enters it. The guard makes that a no-op rather than
/// letting the clock and app tiers fire in the middle of a graph cascade,
/// outside its rounds and outside its budget.
#[cfg(feature = "piccolo")]
#[test]
fn a_running_behavior_does_not_re_enter_the_drain() {
    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!("turnstone-reentry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    app.now_ms = Some(0);

    // A schedule that is already due, and a graph behavior whose run would
    // re-enter the drain. Without the guard the schedule fires nested.
    let sched = app.data_root.join("ticker.lua");
    std::fs::write(
        &sched,
        "-- @watch every/minute
mere.open('mere://ticked')",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: sched.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);

    let watcher = app.data_root.join("watcher.lua");
    std::fs::write(
        &watcher,
        "-- @watch https://example.com/w/
mere.open('mere://watched')",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: watcher.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);

    // Time has not advanced, so the schedule is not due; the graph behavior
    // is. Its run must not drag the clock tier in with it.
    app.update(Action::OpenAddress("https://example.com/w/one".to_string()));
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://watched")
            .is_some(),
        "the graph behavior ran"
    );
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://ticked")
            .is_none(),
        "and the schedule did not fire inside its cascade"
    );
    assert!(!app.draining, "the flag is cleared when the drain returns");
}

/// A behavior installed today still wakes tomorrow. Watches are registered at
/// install and live in memory, so without persistence a reload silently
/// leaves every table empty and every behavior inert.
///
/// Persisted rather than re-derived from the pack source: re-deriving would
/// restart a graph watch's cursor at zero (re-waking on history it had already
/// considered) and restart a schedule's period (so a daily behavior never
/// fires for anyone who reopens their session each morning).
#[cfg(feature = "piccolo")]
#[test]
fn watches_survive_a_session_reload() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-watch-reload-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    app.now_ms = Some(1_000);

    let pack = app.data_root.join("filer.lua");
    std::fs::write(
        &pack,
        "-- @watch file:///notes/
mere.open('mere://filed')",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    assert_eq!(app.watches.watches().len(), 1);
    let cursor_before = app.watches.watches()[0].cursor;
    let scope_before = app.watches.watches()[0].scope.clone();

    // The reload: the tables come back from disk, not from the pack.
    let (graph, app_tier, time, _) = crate::denizen::load_watches(&app.session_dir());
    assert_eq!(graph.watches().len(), 1, "the graph watch came back");
    assert!(app_tier.is_empty());
    assert!(time.is_empty());
    assert_eq!(graph.watches()[0].scope, scope_before);
    assert_eq!(
        graph.watches()[0].cursor,
        cursor_before,
        "with its cursor, so it does not re-wake on history it already considered"
    );
    assert_eq!(
        graph.watches()[0].self_author,
        app.watches.watches()[0].self_author,
        "and its author label, so the no-self-wake refusal still holds"
    );

    // And uninstalling clears the persisted copy, not just the live one.
    let member = *app.denizens.residents.keys().next().unwrap();
    app.update(Action::UninstallDenizen { member });
    let (graph, _, _, _) = crate::denizen::load_watches(&app.session_dir());
    assert!(
        graph.is_empty(),
        "a removed denizen leaves no watch on disk"
    );
}

/// A schedule's phase survives too, which is the half re-deriving would lose.
#[cfg(feature = "piccolo")]
#[test]
fn a_schedules_phase_survives_a_reload() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-phase-reload-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    app.now_ms = Some(500_000);

    let pack = app.data_root.join("gardener.lua");
    std::fs::write(
        &pack,
        "-- @watch every/day
mere.snapshot()",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);

    let (_, _, time, _) = crate::denizen::load_watches(&app.session_dir());
    assert_eq!(time.watches().len(), 1);
    assert_eq!(
        time.watches()[0].last_fired_ms,
        500_000,
        "the period is measured from when it was installed, not from the reload"
    );
    assert_eq!(time.watches()[0].period, servitor::Period::Day);
}

/// The host receipt for target 3: a declared output deadband sits after body
/// evaluation and before Action lowering. A second run proposes a different
/// note body after the interval has elapsed, but its output did not move, so
/// the behavior is named and the journal stays exactly where it was.
#[cfg(feature = "piccolo")]
#[test]
fn a_declared_deadband_refuses_actuation_and_survives_reload() {
    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!("turnstone-deadband-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    app.now_ms = Some(1_000);

    let pack = app.data_root.join("controller.lua");
    std::fs::write(
        &pack,
        "-- @deadband 2 1000\n\
         mere.output(0)\n\
         mere.write('mere://deadband-note', mere.trigger())",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    if let Some(pending) = app.pending_install.as_mut() {
        pending.rings.push(crate::ring::Ring::Author);
    }
    app.update(Action::ConfirmInstallDenizen);
    let (&member, resident) = app.denizens.residents.iter().next().unwrap();
    let subject = resident.subject;
    let _ = app.take_events();

    let first = crate::behaviors::TriggerContext {
        woken_by: vec![crate::behaviors::TriggerEntry {
            seq: 1,
            author: "user".into(),
            nodes: vec!["first".into()],
        }],
    };
    app.run_denizen_for_cascade(member, &first, &[]);
    let after_first = app.journal.lock().unwrap().entries().len();
    assert!(after_first > 0, "the baseline actuation landed");
    let (_, _, _, restored) = crate::denizen::load_watches(&app.session_dir());
    assert_eq!(restored.get(subject).unwrap().minimum_change(), 2);
    assert!(
        restored
            .check(subject, servitor::Actuation::new(0, 3_000))
            .is_err(),
        "the accepted output survived the wire round trip"
    );

    app.now_ms = Some(3_000);
    let second = crate::behaviors::TriggerContext {
        woken_by: vec![crate::behaviors::TriggerEntry {
            seq: 2,
            author: "user".into(),
            nodes: vec!["second".into()],
        }],
    };
    app.run_denizen_for_cascade(member, &second, &[]);
    assert_eq!(
        app.journal.lock().unwrap().entries().len(),
        after_first,
        "the refused output added no journal history"
    );
    let described: Vec<_> = app.take_events().iter().map(AppEvent::describe).collect();
    assert!(
        described
            .iter()
            .any(|event| event.contains("controller") && event.contains("deadband")),
        "the refusal names the behavior and the policy: {described:?}"
    );

    app.update(Action::UninstallDenizen { member });
    let (_, _, _, restored) = crate::denizen::load_watches(&app.session_dir());
    assert!(
        restored.get(subject).is_none(),
        "uninstall removes the bound"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// W5, the flagship: a summarizer watching a container writes a note when its
/// members change, attributed to itself, and stops when its grant is revoked
/// while the note it already wrote stays standing.
///
/// The digest is plain text. The intel seam (esp) would replace the body of
/// the summary without changing the shape of any of this: what the slice is
/// proving is the loop, the attribution, and the revocation, not the prose.
#[cfg(feature = "piccolo")]
#[test]
fn a_summarizer_writes_a_note_when_its_neighborhood_changes() {
    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!("turnstone-summary-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();

    // Directory form, per the containment rule.
    let pack = app.data_root.join("summarizer.lua");
    // The digest is what woke it, not a constant. A constant would be written
    // once and then be a no-op forever, because an unchanged body is not a
    // change and correctly journals nothing: the first version of this test
    // asserted against exactly that.
    std::fs::write(
        &pack,
        "-- @watch https://example.com/notes/
         mere.write('mere://summary', 'neighborhood: ' .. mere.trigger())",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });

    // Authoring is never preselected, so the review must be widened on
    // purpose before the summarizer can write anything.
    if let Some(pending) = app.pending_install.as_mut() {
        pending.rings.push(crate::ring::Ring::Author);
    }
    app.update(Action::ConfirmInstallDenizen);
    let (_, resident) = app.denizens.residents.iter().next().unwrap();
    let subject = resident.subject;
    let expected_author = participant_author(resident);

    app.update(Action::ReseedLayout);
    let before = app.journal.lock().unwrap().entries().len();

    // A member appears in the watched neighborhood.
    app.update(Action::OpenAddress(
        "https://example.com/notes/one".to_string(),
    ));

    let summary = app
        .graph_runtimes
        .graph()
        .get_node_by_url("mere://summary")
        .map(|(_, node)| node.clone());
    assert!(summary.is_some(), "the summary note was written");

    let wrote: Vec<_> = {
        let journal = app.journal.lock().unwrap();
        journal
            .entries()
            .iter()
            .skip(before)
            .filter(|entry| entry.author == expected_author)
            .map(|entry| entry.delta.clone())
            .collect()
    };
    assert!(
        wrote.iter().any(|delta| matches!(
            delta,
            mere::kernel::graph::capture::CapturedDelta::ReplaySetNodeBodyById { .. }
        )),
        "and the body write is in the journal under the summarizer"
    );

    // Revoking the grant stops the updates. The note it already wrote is
    // untouched: revoking authority destroys nothing.
    app.denizens.authority.revoke_root_grants(subject);
    app.watches.remove_subject(subject);
    let after_revoke = app.journal.lock().unwrap().entries().len();
    app.update(Action::OpenAddress(
        "https://example.com/notes/two".to_string(),
    ));
    let still_writing = {
        let journal = app.journal.lock().unwrap();
        journal
            .entries()
            .iter()
            .skip(after_revoke)
            .any(|entry| entry.author == expected_author)
    };
    assert!(
        !still_writing,
        "a revoked summarizer writes nothing further"
    );
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://summary")
            .is_some(),
        "and the note it already wrote is still standing"
    );
}

/// Authoring is a grant of its own: a body holding the ordinary control rings
/// cannot write content, which is what keeps `Dispatch` from quietly meaning
/// "may rewrite your notes".
#[cfg(feature = "piccolo")]
#[test]
fn writing_content_needs_the_author_ring() {
    let app = App::test_stub();
    let err = crate::script::run(
        &app,
        "mere.write('mere://x', 'hello')",
        crate::script::ScriptCapabilities::control(),
        200,
        &crate::behaviors::TriggerContext::default(),
    )
    .unwrap_err();
    assert!(err.contains("content.author"), "refused by name: {err}");
}

/// W4: a behavior fires on the clock, and fires identically on replay because
/// the clock is fed in rather than sampled.
#[cfg(feature = "piccolo")]
#[test]
fn a_scheduled_behavior_fires_on_a_fed_clock_and_replays() {
    fn run() -> Vec<bool> {
        let mut app = App::test_stub();
        app.data_root =
            std::env::temp_dir().join(format!("turnstone-schedule-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&app.data_root);
        std::fs::create_dir_all(app.session_dir()).unwrap();
        app.now_ms = Some(1_000);

        let pack = app.data_root.join("gardener.lua");
        std::fs::write(
            &pack,
            "-- @watch every/hour
mere.open('mere://swept')",
        )
        .unwrap();
        app.update(Action::InstallDenizen {
            path: pack.display().to_string(),
        });
        app.update(Action::ConfirmInstallDenizen);
        assert_eq!(app.time_watches.watches().len(), 1, "the schedule stands");

        const HOUR: u64 = 3_600_000;
        [1_000, 1_000 + HOUR / 2, 1_000 + HOUR]
            .into_iter()
            .map(|now| {
                app.now_ms = Some(now);
                app.update(Action::ReseedLayout);
                app.graph_runtimes
                    .graph()
                    .get_node_by_url("mere://swept")
                    .is_some()
            })
            .collect()
    }
    let first = run();
    assert_eq!(
        first,
        vec![false, false, true],
        "install is not a tick; half a period is not a period; the hour is"
    );
    assert_eq!(first, run(), "the same instants fire the same way");
}

/// A host that supplies no clock fires no schedule, rather than reading "no
/// time" as time zero and running everything at once.
#[cfg(feature = "piccolo")]
#[test]
fn a_session_without_a_clock_never_fires_a_schedule() {
    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!("turnstone-noclock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    app.now_ms = None;

    let pack = app.data_root.join("gardener.lua");
    std::fs::write(
        &pack,
        "-- @watch every/minute
mere.open('mere://swept')",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    app.update(Action::ReseedLayout);

    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://swept")
            .is_none()
    );
}

/// The review names the period, because granting a schedule is granting
/// resource and the reviewer is owed the sentence.
#[cfg(feature = "piccolo")]
#[test]
fn the_review_names_the_period_a_schedule_asks_for() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-sched-review-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = app.data_root.join("gardener.lua");
    std::fs::write(
        &pack,
        "-- @watch every/day
mere.snapshot()",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    let review = crate::denizen::review_line(app.pending_install.as_ref().expect("staged"));
    assert!(review.contains("wakes on: every day"), "{review}");
}
