// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::collections::VecDeque;

use mere_surface_api::settings::{
    CommittedSettingsWrite, RevisionedSettingSpec, RevisionedSettingsProvider,
    RevisionedSettingsSnapshot, RevisionedSettingsWriteRequest, RevisionedWriteOutcome,
    SettingControl, SettingMovement, SettingMutability, SettingOption, SettingScope,
    SettingSecurity, SettingSpec, SettingValue, SettingsDomainRevision, SettingsOwnerIncarnation,
    SettingsRevisionDomain, SettingsRevisionGuard, SettingsUnavailable, SettingsValidationFailure,
};
use workbench::SettingsRef;

use super::*;

const REF: &str = "product/appearance";
const SETTING: &str = "chrome.enabled";

fn guard(owner: &str, domain: &str, revision: u64) -> SettingsRevisionGuard {
    SettingsRevisionGuard {
        owner: SettingsOwnerIncarnation(owner.into()),
        domain: SettingsRevisionDomain(domain.into()),
        revision,
    }
}

fn snapshot(owner: &str, revision: u64) -> RevisionedSettingsSnapshot {
    let guard = guard(owner, "appearance", revision);
    RevisionedSettingsSnapshot {
        reference: SettingsRef(REF.into()),
        owner: guard.owner.clone(),
        revisions: vec![SettingsDomainRevision {
            domain: guard.domain.clone(),
            revision,
        }],
        specs: vec![RevisionedSettingSpec {
            spec: SettingSpec {
                id: SETTING.into(),
                label: "Enabled".into(),
                scope: SettingScope::Application,
                movement: SettingMovement::LocalOnly,
                mutability: SettingMutability::Live,
                security: SettingSecurity::Ordinary,
                control: SettingControl::Toggle,
                value: SettingValue::Boolean(false),
            },
            guard,
        }],
    }
}

fn commit(request: &RevisionedSettingsWriteRequest, revision: u64) -> RevisionedWriteOutcome {
    RevisionedWriteOutcome::Committed(CommittedSettingsWrite {
        request_id: request.request_id.clone(),
        reference: request.reference.clone(),
        setting_id: request.setting_id.clone(),
        value: request.value.clone(),
        guard: SettingsRevisionGuard {
            revision,
            ..request.expected.clone()
        },
    })
}

#[derive(Default)]
struct FakeProvider {
    requests: Vec<RevisionedSettingsWriteRequest>,
    outcomes: VecDeque<RevisionedWriteOutcome>,
}

impl RevisionedSettingsProvider for FakeProvider {
    fn snapshot(&self, _: &SettingsRef) -> Result<RevisionedSettingsSnapshot, SettingsUnavailable> {
        unreachable!("binding tests do not ask the receiver to refresh")
    }

    fn apply_revisioned(
        &mut self,
        request: RevisionedSettingsWriteRequest,
    ) -> RevisionedWriteOutcome {
        self.requests.push(request.clone());
        self.outcomes
            .pop_front()
            .unwrap_or_else(|| commit(&request, request.expected.revision + 1))
    }
}

fn binding() -> SettingsBinding {
    SettingsBinding::from_snapshot(&snapshot("owner-1", 7), SETTING, 4).unwrap()
}

#[test]
fn snapshot_requires_each_setting_guard_to_match_owner_and_revision_vector() {
    let mut bad_owner = snapshot("owner-1", 7);
    bad_owner.specs[0].guard.owner = SettingsOwnerIncarnation("other-owner".into());
    assert!(matches!(
        SettingsBinding::from_snapshot(&bad_owner, SETTING, 4),
        Err(SettingsBindingError::InvalidSnapshot(_))
    ));

    let mut missing_domain = snapshot("owner-1", 7);
    missing_domain.revisions.clear();
    assert!(matches!(
        SettingsBinding::from_snapshot(&missing_domain, SETTING, 4),
        Err(SettingsBindingError::InvalidSnapshot(_))
    ));
}

#[test]
fn adoption_detection_ignores_label_and_revision_only_changes() {
    let binding = binding();
    let mut newer_revision = snapshot("owner-1", 8);
    newer_revision.specs[0].spec.label = "Renamed".into();
    assert_eq!(binding.needs_snapshot_adoption(&newer_revision), Ok(false));

    let mut changed_value = snapshot("owner-1", 8);
    changed_value.specs[0].spec.value = SettingValue::Boolean(true);
    assert_eq!(binding.needs_snapshot_adoption(&changed_value), Ok(true));
    assert_eq!(
        binding.needs_snapshot_adoption(&snapshot("owner-2", 0)),
        Ok(true)
    );
}

#[test]
fn rebind_or_dispose_refuses_old_ticket_without_calling_provider() {
    for dispose in [false, true] {
        let mut binding = binding();
        let ticket = binding
            .prepare("request", SettingValue::Boolean(true), 1)
            .unwrap();
        if dispose {
            binding.dispose();
        } else {
            binding
                .adopt_snapshot(&snapshot("owner-2", 0), SETTING)
                .unwrap();
        }
        let mut provider = FakeProvider::default();
        assert!(binding.execute(&ticket, &mut provider).is_err());
        assert!(provider.requests.is_empty());
    }
}

#[test]
fn malformed_foreign_ack_is_not_admitted_and_valid_ack_is_single_use() {
    let mut binding = binding();
    let ticket = binding
        .prepare("request", SettingValue::Boolean(true), 3)
        .unwrap();
    let mut provider = FakeProvider::default();
    let request_result = binding.execute(&ticket, &mut provider).unwrap();
    let mut foreign = match request_result.clone() {
        RevisionedWriteOutcome::Committed(committed) => committed,
        _ => unreachable!(),
    };
    foreign.guard.owner = SettingsOwnerIncarnation("foreign-owner".into());
    assert_eq!(
        binding.complete(&ticket, RevisionedWriteOutcome::Committed(foreign), 3),
        Err(SettingsBindingError::MalformedOutcome)
    );
    assert_eq!(binding.baseline_guard().revision, 7);

    let accepted = request_result_from_owner(&provider.requests[0]);
    assert_eq!(
        binding.complete(&ticket, accepted.clone(), 3),
        Ok(WriteDisposition::Clean)
    );
    assert_eq!(
        binding.complete(&ticket, accepted, 3),
        Err(SettingsBindingError::UnknownTicket)
    );
}

fn request_result_from_owner(request: &RevisionedSettingsWriteRequest) -> RevisionedWriteOutcome {
    commit(request, request.expected.revision + 1)
}

#[test]
fn acknowledgement_for_old_edit_keeps_newer_draft_and_advances_baseline() {
    let mut binding = binding();
    let ticket = binding
        .prepare("edit-1", SettingValue::Boolean(true), 11)
        .unwrap();
    let mut provider = FakeProvider::default();
    let outcome = binding.execute(&ticket, &mut provider).unwrap();
    assert_eq!(
        binding.complete(&ticket, outcome, 12),
        Ok(WriteDisposition::KeepNewerDraft)
    );
    assert_eq!(binding.baseline_guard().revision, 8);
}

#[test]
fn out_of_order_commit_ack_does_not_regress_guard_or_committed_value() {
    let mut binding = binding();
    let older = binding
        .prepare("older", SettingValue::Boolean(true), 1)
        .unwrap();
    let newer = binding
        .prepare("newer", SettingValue::Boolean(false), 2)
        .unwrap();
    let mut provider = FakeProvider::default();
    let older_outcome = binding.execute(&older, &mut provider).unwrap();
    let newer_outcome = binding.execute(&newer, &mut provider).unwrap();

    let mut newer_commit = match newer_outcome {
        RevisionedWriteOutcome::Committed(committed) => committed,
        _ => unreachable!(),
    };
    newer_commit.guard.revision = 9;
    assert_eq!(
        binding.complete(&newer, RevisionedWriteOutcome::Committed(newer_commit), 2),
        Ok(WriteDisposition::Clean)
    );
    assert_eq!(binding.guard().revision, 9);

    let mut older_commit = match older_outcome {
        RevisionedWriteOutcome::Committed(committed) => committed,
        _ => unreachable!(),
    };
    older_commit.guard.revision = 8;
    assert_eq!(
        binding.complete(&older, RevisionedWriteOutcome::Committed(older_commit), 1),
        Ok(WriteDisposition::KeepDraft)
    );
    assert_eq!(binding.guard().revision, 9);

    let mut latest = snapshot("owner-1", 9);
    latest.specs[0].spec.value = SettingValue::Boolean(false);
    assert_eq!(binding.needs_snapshot_adoption(&latest), Ok(false));
    let mut stale = snapshot("owner-1", 8);
    stale.specs[0].spec.value = SettingValue::Boolean(true);
    assert_eq!(binding.needs_snapshot_adoption(&stale), Ok(true));
}

#[test]
fn conflict_from_replaced_owner_is_admitted_but_does_not_rebind_baseline() {
    let mut binding = binding();
    let ticket = binding
        .prepare("replaced", SettingValue::Boolean(true), 1)
        .unwrap();
    let mut provider = FakeProvider::default();
    provider
        .outcomes
        .push_back(RevisionedWriteOutcome::Conflict {
            request_id: "replaced".into(),
            current: guard("owner-2", "appearance", 0),
        });
    let outcome = binding.execute(&ticket, &mut provider).unwrap();
    assert_eq!(
        binding.complete(&ticket, outcome, 1),
        Ok(WriteDisposition::KeepDraft)
    );
    assert_eq!(binding.guard().owner.0, "owner-1");
    assert_eq!(binding.guard().revision, 7);
}

fn reject_bad_committed_value(
    control: SettingControl,
    initial: SettingValue,
    requested: SettingValue,
    returned: SettingValue,
) {
    let mut snapshot = snapshot("owner-1", 7);
    snapshot.specs[0].spec.control = control;
    snapshot.specs[0].spec.value = initial;
    let mut binding = SettingsBinding::from_snapshot(&snapshot, SETTING, 2).unwrap();
    let ticket = binding.prepare("bad-commit", requested, 1).unwrap();
    let mut provider = FakeProvider::default();
    let outcome = binding.execute(&ticket, &mut provider).unwrap();
    let RevisionedWriteOutcome::Committed(mut committed) = outcome else {
        unreachable!();
    };
    committed.value = returned;

    assert_eq!(
        binding.complete(&ticket, RevisionedWriteOutcome::Committed(committed), 1),
        Err(SettingsBindingError::MalformedOutcome)
    );
    assert_eq!(binding.guard().revision, 7);
}

#[test]
fn malformed_committed_values_fail_control_range_and_choice_validation() {
    let number_control = SettingControl::Number {
        min: Some(0.5),
        max: Some(3.0),
        step: Some(0.05),
    };
    reject_bad_committed_value(
        number_control.clone(),
        SettingValue::Number(1.0),
        SettingValue::Number(1.5),
        SettingValue::Number(f64::NAN),
    );
    reject_bad_committed_value(
        number_control,
        SettingValue::Number(1.0),
        SettingValue::Number(1.5),
        SettingValue::Number(3.01),
    );
    reject_bad_committed_value(
        SettingControl::Choice {
            options: vec![
                SettingOption {
                    value: "alpha".into(),
                    label: "Alpha".into(),
                },
                SettingOption {
                    value: "beta".into(),
                    label: "Beta".into(),
                },
            ],
        },
        SettingValue::Text("alpha".into()),
        SettingValue::Text("beta".into()),
        SettingValue::Text("missing-option".into()),
    );
}

#[test]
fn queued_commands_are_delivered_independently_and_failures_keep_baseline() {
    let mut binding = binding();
    let first = binding
        .prepare("first", SettingValue::Boolean(true), 1)
        .unwrap();
    let second = binding
        .prepare("second", SettingValue::Boolean(false), 2)
        .unwrap();
    let mut provider = FakeProvider::default();
    provider.outcomes.extend([
        RevisionedWriteOutcome::Validation {
            request_id: "first".into(),
            failure: SettingsValidationFailure {
                setting_id: SETTING.into(),
                message: "refused".into(),
            },
        },
        RevisionedWriteOutcome::Validation {
            request_id: "second".into(),
            failure: SettingsValidationFailure {
                setting_id: SETTING.into(),
                message: "refused".into(),
            },
        },
    ]);
    let first_outcome = binding.execute(&first, &mut provider).unwrap();
    let second_outcome = binding.execute(&second, &mut provider).unwrap();
    assert_eq!(provider.requests.len(), 2);
    assert_eq!(provider.requests[0].request_id, "first");
    assert_eq!(provider.requests[0].value, SettingValue::Boolean(true));
    assert_eq!(provider.requests[1].request_id, "second");
    assert_eq!(provider.requests[1].value, SettingValue::Boolean(false));
    assert_eq!(
        binding.complete(&first, first_outcome, 1),
        Ok(WriteDisposition::KeepDraft)
    );
    assert_eq!(
        binding.complete(&second, second_outcome, 2),
        Ok(WriteDisposition::KeepDraft)
    );
    assert_eq!(binding.baseline_guard().revision, 7);
}

#[test]
fn second_dispatch_and_duplicate_pending_ids_are_rejected() {
    let mut binding = binding();
    let ticket = binding
        .prepare("same", SettingValue::Boolean(true), 1)
        .unwrap();
    assert!(matches!(
        binding.prepare("same", SettingValue::Boolean(false), 2),
        Err(SettingsBindingError::DuplicateRequestId(_))
    ));
    let mut provider = FakeProvider::default();
    binding.execute(&ticket, &mut provider).unwrap();
    assert_eq!(
        binding.execute(&ticket, &mut provider),
        Err(SettingsBindingError::AlreadyExecuted)
    );
    assert_eq!(provider.requests.len(), 1);
}

#[test]
fn old_ticket_cannot_alias_reused_request_id_after_completion() {
    let mut binding = binding();
    let old = binding
        .prepare("reusable", SettingValue::Boolean(true), 1)
        .unwrap();
    let mut provider = FakeProvider::default();
    let old_outcome = binding.execute(&old, &mut provider).unwrap();
    binding.complete(&old, old_outcome, 1).unwrap();

    let _new = binding
        .prepare("reusable", SettingValue::Boolean(false), 2)
        .unwrap();
    assert_eq!(
        binding.execute(&old, &mut provider),
        Err(SettingsBindingError::StaleTicket)
    );
    assert_eq!(provider.requests.len(), 1);
}

#[test]
fn pending_queue_is_bounded_and_conflict_keeps_the_adopted_guard() {
    let mut binding = SettingsBinding::from_snapshot(&snapshot("owner-1", 7), SETTING, 1).unwrap();
    let ticket = binding
        .prepare("one", SettingValue::Boolean(true), 1)
        .unwrap();
    assert!(matches!(
        binding.prepare("two", SettingValue::Boolean(false), 2),
        Err(SettingsBindingError::PendingLimit)
    ));
    let mut provider = FakeProvider::default();
    provider
        .outcomes
        .push_back(RevisionedWriteOutcome::Conflict {
            request_id: "one".into(),
            current: guard("owner-1", "appearance", 8),
        });
    let outcome = binding.execute(&ticket, &mut provider).unwrap();
    assert_eq!(
        binding.complete(&ticket, outcome, 1),
        Ok(WriteDisposition::KeepDraft)
    );
    assert_eq!(binding.baseline_guard().revision, 7);
}

#[test]
fn late_response_after_rebind_cannot_advance_new_baseline() {
    let mut binding = binding();
    let ticket = binding
        .prepare("late", SettingValue::Boolean(true), 1)
        .unwrap();
    let mut provider = FakeProvider::default();
    let outcome = binding.execute(&ticket, &mut provider).unwrap();
    binding
        .adopt_snapshot(&snapshot("owner-2", 0), SETTING)
        .unwrap();
    assert_eq!(
        binding.complete(&ticket, outcome, 1),
        Err(SettingsBindingError::StaleTicket)
    );
    assert_eq!(binding.baseline_guard().owner.0, "owner-2");
    assert_eq!(binding.baseline_guard().revision, 0);
}
