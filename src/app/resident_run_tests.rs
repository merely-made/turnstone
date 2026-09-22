// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

use crate::action::Action;
use crate::observe::AppEvent;

fn installed_app(name: &str, source: &str) -> (App, uuid::Uuid) {
    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!(
        "turnstone-resident-run-app-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = app.data_root.join(format!("{name}.lua"));
    std::fs::write(&pack, source).unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    let member = *app.denizens.residents.keys().next().unwrap();
    (app, member)
}

#[test]
fn malformed_run_store_refuses_before_manual_body_mutation() {
    let (mut app, member) = installed_app("corrupt", "mere.open('mere://corrupt')");
    let session = app.session_id;
    let seed = app.graph_runtimes.visit("mere://seed");
    crate::session::save_session_graph(&app.session_dir(), app.graph_runtimes.graph());
    let run_path = crate::resident_runs::path(&app.session_dir());
    std::fs::create_dir_all(run_path.parent().unwrap()).unwrap();
    std::fs::write(&run_path, b"{ malformed resident run state").unwrap();

    app.adopt_session(session);
    let before = app.graph_runtimes.graph().node_count();
    app.update(Action::RunDenizen { member });

    assert_eq!(app.graph_runtimes.graph().node_count(), before);
    assert!(app.take_events().into_iter().any(|event| {
        matches!(event, AppEvent::DenizenRefused(reason) if reason.contains("resident run store"))
    }));
    assert!(app.graph_runtimes.graph().get_node(seed).is_some());
    let _ = std::fs::remove_dir_all(&app.data_root);
}

#[test]
fn adopting_foreign_run_records_clears_the_live_table_without_rewriting_them() {
    let (mut app, member) = installed_app("foreign-session", "mere.open('mere://foreign-session')");
    app.update(Action::RunDenizen { member });
    assert!(app.resident_runs.reducer(servitor::RunId(1)).is_ok());
    let path = crate::resident_runs::path(&app.session_dir());
    let mut disk: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    disk["runs"][0]["session"] = serde_json::json!(uuid::Uuid::new_v4());
    let corrupt = serde_json::to_vec(&disk).unwrap();
    std::fs::write(&path, &corrupt).unwrap();
    app.adopt_session(app.session_id);
    assert!(app.resident_run_error.as_ref().unwrap().contains("another session"));
    assert!(app.resident_runs.reducer(servitor::RunId(1)).is_err());
    assert_eq!(std::fs::read(path).unwrap(), corrupt);
    let _ = std::fs::remove_dir_all(&app.data_root);
}

#[cfg(feature = "piccolo")]
#[test]
fn default_policy_records_two_completed_invocations_without_replaying_them() {
    let (mut app, member) = installed_app("repeat", "mere.open('https://resident.example/repeat')");
    let subject = app.denizens.residents[&member].subject;
    let before_seq = app.journal.lock().unwrap().entries().len() as u64;
    app.take_events();
    for _ in 0..2 {
        let effects = app.update(Action::RunDenizen { member });
        assert!(effects.iter().any(|effect| matches!(effect, crate::action::Effect::SaveSession)));
        assert!(app.take_events().iter().any(|event| matches!(event, AppEvent::DenizenRan(_))));
    }
    let before = app.graph_runtimes.graph().node_count();
    let stored = crate::resident_runs::load(&app.session_dir()).unwrap();
    for id in [servitor::RunId(1), servitor::RunId(2)] {
        let run = stored.reducer(id).unwrap();
        assert_eq!(run.phase(), servitor::RunPhase::Terminal(servitor::TerminalOutcome::Completed));
        assert_eq!(run.ticket().binding.id.0, *member.as_bytes());
        assert!(run.event_sequence() >= 4, "evaluation and lowering each need an intent and a result");
    }
    assert!(app.graph_runtimes.graph().get_node_by_url("https://resident.example/repeat").is_some());
    assert!(crate::behaviors::entries_since(&app, before_seq).iter()
        .any(|entry| entry.author == subject.to_hex()), "local graph edits retain their resident author");
    assert_eq!(app.graph_runtimes.graph().node_count(), before);
    assert!(app.take_events().is_empty(), "reading and replaying records never invokes the app");
    let _ = std::fs::remove_dir_all(&app.data_root);
}

#[cfg(feature = "piccolo")]
#[test]
fn strict_handoff_blocks_overlap_and_cancel_preserves_uncertainty() {
    let (mut app, member) = installed_app("strict", "mere.open('https://resident.example/strict')");
    app.resident_run_effect_policy = crate::resident_runs::ExternalEffectPolicy::StrictReconciliation;
    app.take_events();
    let session = app.session_id;
    let effects = app.update(Action::RunDenizen { member });
    assert!(effects.iter().any(|effect| matches!(effect, crate::action::Effect::FetchPage { .. })));
    let id = servitor::RunId(1);
    let pending = app.resident_runs.reducer(id).unwrap();
    let correlation = match pending.phase() {
        servitor::RunPhase::Awaiting { correlation, effect }
        | servitor::RunPhase::Reconciling { correlation, effect } => {
            assert_eq!(effect.kind, servitor::EffectKind::NeedsReconciliation);
            correlation
        }
        phase => panic!("strict external handoff must remain unresolved: {phase:?}"),
    };
    app.take_events();
    app.update(Action::RunDenizen { member });
    assert!(app.take_events().iter().any(|event| {
        matches!(event, AppEvent::DenizenRefused(reason) if reason.contains("unresolved"))
    }));
    assert!(app.resident_runs.reducer(servitor::RunId(2)).is_err());

    app.cancel_resident_run(session, id).unwrap();
    assert!(matches!(app.resident_runs.reducer(id).unwrap().phase(), servitor::RunPhase::Reconciling { .. }));
    assert!(app.reconcile_resident_run(
        crate::panes::SessionId::new(), id, correlation, servitor::ResultKind::Completed,
    ).is_err());
    assert!(app.reconcile_resident_run(
        session, id, servitor::Correlation { attempt: correlation.attempt + 1, ..correlation },
        servitor::ResultKind::Completed,
    ).is_err());
    let disk = crate::resident_runs::load(&app.session_dir()).unwrap();
    assert!(matches!(disk.reducer(id).unwrap().phase(), servitor::RunPhase::Reconciling { .. }));
    app.reconcile_resident_run(session, id, correlation, servitor::ResultKind::Completed).unwrap();
    assert_eq!(app.resident_runs.reducer(id).unwrap().phase(),
        servitor::RunPhase::Terminal(servitor::TerminalOutcome::Cancelled));
    assert!(app.reconcile_resident_run(session, id, correlation, servitor::ResultKind::Completed).is_err());
    app.resident_run_effect_policy = crate::resident_runs::ExternalEffectPolicy::HandoffUntracked;
    app.update(Action::RunDenizen { member });
    assert_eq!(app.resident_runs.reducer(servitor::RunId(2)).unwrap().phase(),
        servitor::RunPhase::Terminal(servitor::TerminalOutcome::Completed));
    let _ = std::fs::remove_dir_all(&app.data_root);
}

#[cfg(feature = "piccolo")]
#[test]
fn evaluation_failure_is_recorded_and_does_not_strand_the_resident() {
    let (mut app, member) = installed_app("eval-failure", "mere.open('')");
    let before = app.graph_runtimes.graph().node_count();
    for id in [servitor::RunId(1), servitor::RunId(2)] {
        app.take_events();
        app.update(Action::RunDenizen { member });
        let run = app.resident_runs.reducer(id).unwrap();
        assert_eq!(run.phase(), servitor::RunPhase::Terminal(servitor::TerminalOutcome::Failed));
        assert_eq!(run.usage().consecutive_failures, 1);
        assert!(app.take_events().iter().any(|event| matches!(event, AppEvent::DenizenRefused(_))));
    }
    assert_eq!(app.graph_runtimes.graph().node_count(), before);
    let _ = std::fs::remove_dir_all(&app.data_root);
}

#[cfg(feature = "piccolo")]
#[test]
fn exhausted_decision_budget_refuses_before_body_or_local_actions() {
    let (mut app, member) = installed_app("budget", "mere.open('https://resident.example/budget')");
    app.resident_run_limits.decisions = 0;
    let before = app.graph_runtimes.graph().node_count();
    app.take_events();
    let effects = app.update(Action::RunDenizen { member });
    assert!(!effects.iter().any(|effect| matches!(effect, crate::action::Effect::FetchPage { .. })));
    assert_eq!(app.graph_runtimes.graph().node_count(), before);
    assert_eq!(app.resident_runs.reducer(servitor::RunId(1)).unwrap().phase(),
        servitor::RunPhase::Terminal(servitor::TerminalOutcome::BudgetExhausted));
    assert!(app.take_events().iter().any(|event| matches!(event, AppEvent::DenizenRefused(_))));
    app.resident_run_limits.decisions = 1;
    app.update(Action::RunDenizen { member });
    assert_eq!(app.resident_runs.reducer(servitor::RunId(2)).unwrap().phase(),
        servitor::RunPhase::Terminal(servitor::TerminalOutcome::BudgetExhausted));
    assert!(app.resident_run_error.is_none(), "ordinary exhaustion is not corrupt storage");
    assert_eq!(app.graph_runtimes.graph().node_count(), before);
    app.resident_run_limits.decisions = 100;
    app.update(Action::RunDenizen { member });
    assert_eq!(app.resident_runs.reducer(servitor::RunId(3)).unwrap().phase(),
        servitor::RunPhase::Terminal(servitor::TerminalOutcome::Completed));
    let _ = std::fs::remove_dir_all(&app.data_root);
}
