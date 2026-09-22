// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use crate::resident_runs::{self, ExternalEffectPolicy, ResidentRuns, StorageLimits};
use servitor::{BodyRevision, Effect, EffectKind, Lifecycle, ResidentBinding, ResidentId,
    ResultKind, RunEvent, RunId, RunLimits, RunPhase, RunTicket, Subject, TerminalOutcome,
    Trigger, Usage};

fn pending_store(name: &str) -> (std::path::PathBuf, ResidentRuns, uuid::Uuid, RunId) {
    let dir = std::env::temp_dir().join(format!("turnstone-run-store-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let member = uuid::Uuid::from_bytes([1; 16]);
    let session = uuid::Uuid::from_bytes([2; 16]);
    let mut store = ResidentRuns::default();
    let id = store.begin(session, member, RunTicket {
        binding: ResidentBinding {
            id: ResidentId(*member.as_bytes()), subject: Subject::new([3; 32]),
            revision: BodyRevision([4; 32]), generation: 5, lifecycle: Lifecycle::Active,
        },
        trigger: Trigger::Manual, required: vec![],
    }, RunLimits { decisions: 8, tool_calls: 0, tokens: 0, elapsed_ms: 1000, consecutive_failures: 1 }, 10).unwrap();
    let correlation = store.reducer(id).unwrap().pending_correlation().unwrap();
    store.append(id, RunEvent::IntentRecorded {
        correlation, effect: Effect { kind: EffectKind::NeedsReconciliation, operation: 2 },
        usage: Usage { decisions: 1, ..Usage::default() }, at_ms: 11,
    }).unwrap();
    store.record_local_action_intent(id, correlation, "application actions").unwrap();
    resident_runs::save(&dir, &store).unwrap();
    (dir, store, session, id)
}

#[test]
fn reopening_recovers_uncertainty_even_after_clock_rollback() {
    let (dir, _, session, id) = pending_store("recovery");
    let mut reopened = resident_runs::load(&dir).unwrap();
    reopened.validate_session(session).unwrap();
    reopened.mark_interrupted(1).unwrap();
    let correlation = match reopened.reducer(id).unwrap().phase() {
        RunPhase::Reconciling { correlation, .. } => correlation,
        phase => panic!("pending consequential work must reconcile: {phase:?}"),
    };
    resident_runs::save(&dir, &reopened).unwrap();
    let mut cancelled = resident_runs::load(&dir).unwrap();
    cancelled.append(id, RunEvent::CancellationRequested { at_ms: 12 }).unwrap();
    assert!(matches!(cancelled.reducer(id).unwrap().phase(), RunPhase::Reconciling { .. }));
    cancelled.append(id, RunEvent::ReconciliationResolved {
        correlation, result: ResultKind::Completed, usage: Usage::default(), at_ms: 13,
    }).unwrap();
    assert_eq!(cancelled.reducer(id).unwrap().phase(), RunPhase::Terminal(TerminalOutcome::Cancelled));
    assert_eq!(cancelled.reducer(id).unwrap().event_sequence(), 4);
    assert_eq!(cancelled.reducer(id).unwrap().usage().decisions, 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn malformed_or_mismatched_persisted_facts_are_rejected() {
    let (dir, store, session, id) = pending_store("corruption");
    assert!(store.validate_session(uuid::Uuid::from_bytes([9; 16])).is_err());
    assert_eq!(store.metadata(id).unwrap().session, session);
    let target = resident_runs::path(&dir);
    let original: serde_json::Value = serde_json::from_slice(&std::fs::read(&target).unwrap()).unwrap();
    let mut wrong_run = original.clone();
    wrong_run["runs"][0]["events"][0]["run_id"] = serde_json::json!(id.0 + 1);
    let mut wrong_member = original.clone();
    wrong_member["runs"][0]["member"] = serde_json::json!(uuid::Uuid::from_bytes([9; 16]));
    let mut duplicate = original.clone();
    duplicate["runs"].as_array_mut().unwrap().push(original["runs"][0].clone());
    let mut forged_metadata = original.clone();
    forged_metadata["runs"][0]["actions"][0]["step"] = serde_json::json!(99);
    let mut unknown = original.clone();
    unknown["extra"] = serde_json::json!(true);
    let mut bad_version = original.clone();
    bad_version["version"] = serde_json::json!(999);
    let mut invalid_trigger = original.clone();
    invalid_trigger["runs"][0]["header"]["trigger"] = serde_json::json!({
        "kind": "journal", "source": "   ", "first": 0, "last": 0,
    });
    for corrupt in [wrong_run, wrong_member, duplicate, forged_metadata, unknown, bad_version, invalid_trigger] {
        std::fs::write(&target, serde_json::to_vec(&corrupt).unwrap()).unwrap();
        assert!(resident_runs::load(&dir).is_err());
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn configured_read_bound_and_repeated_atomic_save_preserve_receipts() {
    let (dir, mut store, _, id) = pending_store("bounds");
    let correlation = store.reducer(id).unwrap().pending_correlation().unwrap();
    store.record_local_action_outcome(id, correlation, "application actions", 2, true,
        ExternalEffectPolicy::StrictReconciliation).unwrap();
    resident_runs::save(&dir, &store).unwrap();
    let size = std::fs::metadata(resident_runs::path(&dir)).unwrap().len();
    assert!(resident_runs::load_with_limits(&dir, StorageLimits {
        max_bytes: size - 1, ..StorageLimits::default()
    }).is_err());
    let loaded = resident_runs::load_with_limits(&dir, StorageLimits {
        max_bytes: size, ..StorageLimits::default()
    }).unwrap();
    assert_eq!(loaded.reducer(id).unwrap(), store.reducer(id).unwrap());
    let disk: serde_json::Value = serde_json::from_slice(&std::fs::read(resident_runs::path(&dir)).unwrap()).unwrap();
    assert_eq!(disk["runs"][0]["actions"][0]["outcome"]["strict_reconciliation"]["local_refused"], true);
    let _ = std::fs::remove_dir_all(dir);
}
