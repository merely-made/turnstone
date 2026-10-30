// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::{
    APPLICATION_REFERENCE, ApplicationSettingsProvider, SettingsBinding, SettingsBindingError,
    SharedApplicationSettingsOwner, WriteDisposition,
};
use mere_surface_api::settings::{
    RevisionedSettingsProvider, RevisionedSettingsSnapshot, RevisionedWriteOutcome, SettingValue,
};
use pandect::{ApplicationSettings, application_settings_path};
use workbench::SettingsRef;

fn settings_reference() -> SettingsRef {
    SettingsRef(APPLICATION_REFERENCE.into())
}

fn owner(root: &std::path::Path) -> SharedApplicationSettingsOwner {
    SharedApplicationSettingsOwner::new(ApplicationSettingsProvider::from_settings(
        root,
        ApplicationSettings::default(),
    ))
}

fn binding(snapshot: &RevisionedSettingsSnapshot) -> SettingsBinding {
    SettingsBinding::from_snapshot(snapshot, "ui.zoom", 4).unwrap()
}

fn zoom(snapshot: &RevisionedSettingsSnapshot) -> (&SettingValue, u64) {
    let row = snapshot
        .specs
        .iter()
        .find(|entry| entry.spec.id == "ui.zoom")
        .unwrap();
    (&row.spec.value, row.guard.revision)
}

#[test]
fn two_bindings_from_one_snapshot_admit_only_the_first_same_domain_write() {
    let root = tempfile::tempdir().unwrap();
    let mut owner = owner(root.path());
    let mut second_owner = owner.clone();
    let initial = owner.snapshot(&settings_reference()).unwrap();
    let (_, initial_revision) = zoom(&initial);
    let mut first = binding(&initial);
    let mut second = binding(&initial);
    let first_ticket = first
        .prepare("first-write", SettingValue::Number(1.5), 1)
        .unwrap();
    let second_ticket = second
        .prepare("second-write", SettingValue::Number(1.8), 1)
        .unwrap();

    let first_result = first.execute(&first_ticket, &mut owner).unwrap();
    let second_result = second.execute(&second_ticket, &mut second_owner).unwrap();
    assert!(matches!(first_result, RevisionedWriteOutcome::Committed(_)));
    assert!(matches!(
        second_result,
        RevisionedWriteOutcome::Conflict { .. }
    ));
    assert_eq!(
        first.complete(&first_ticket, first_result, 1),
        Ok(WriteDisposition::Clean)
    );
    assert_eq!(
        second.complete(&second_ticket, second_result, 1),
        Ok(WriteDisposition::KeepDraft)
    );

    let current = owner.snapshot(&settings_reference()).unwrap();
    assert_eq!(zoom(&current).0, &SettingValue::Number(1.5));
    assert_eq!(zoom(&current).1, initial_revision + 1);
}

#[test]
fn real_owner_canonical_number_values_are_admitted_as_the_baseline() {
    let root = tempfile::tempdir().unwrap();
    let mut owner = owner(root.path());
    let initial = owner.snapshot(&settings_reference()).unwrap();
    let mut zoom_binding = binding(&initial);
    let zoom_ticket = zoom_binding
        .prepare("zoom-canonical", SettingValue::Number(1.37), 1)
        .unwrap();
    let zoom_outcome = zoom_binding.execute(&zoom_ticket, &mut owner).unwrap();
    let RevisionedWriteOutcome::Committed(zoom_commit) = &zoom_outcome else {
        panic!("valid zoom should commit");
    };
    assert_ne!(zoom_commit.value, SettingValue::Number(1.37));
    assert_eq!(
        zoom_binding.complete(&zoom_ticket, zoom_outcome, 1),
        Ok(WriteDisposition::Clean)
    );
    let after_zoom = owner.snapshot(&settings_reference()).unwrap();
    assert_eq!(zoom_binding.needs_snapshot_adoption(&after_zoom), Ok(false));

    let mut budget_binding =
        SettingsBinding::from_snapshot(&after_zoom, "behaviors.cascade_budget", 2).unwrap();
    let budget_ticket = budget_binding
        .prepare("budget-canonical", SettingValue::Number(5.5), 2)
        .unwrap();
    let budget_outcome = budget_binding.execute(&budget_ticket, &mut owner).unwrap();
    let RevisionedWriteOutcome::Committed(budget_commit) = &budget_outcome else {
        panic!("valid cascade budget should commit");
    };
    assert_eq!(budget_commit.value, SettingValue::Number(5.0));
    assert_eq!(
        budget_binding.complete(&budget_ticket, budget_outcome, 2),
        Ok(WriteDisposition::Clean)
    );
    let after_budget = owner.snapshot(&settings_reference()).unwrap();
    assert_eq!(
        budget_binding.needs_snapshot_adoption(&after_budget),
        Ok(false)
    );
}

#[test]
fn rebind_and_dispose_refuse_tickets_before_real_owner_is_invoked() {
    let root = tempfile::tempdir().unwrap();
    let mut owner = owner(root.path());
    let before = owner.snapshot(&settings_reference()).unwrap();
    let (before_value, before_revision) = zoom(&before);

    let mut rebound = binding(&before);
    let stale_ticket = rebound
        .prepare("rebind-ticket", SettingValue::Number(1.7), 1)
        .unwrap();
    rebound.adopt_snapshot(&before, "ui.zoom").unwrap();
    assert_eq!(
        rebound.execute(&stale_ticket, &mut owner),
        Err(SettingsBindingError::StaleTicket)
    );

    let mut disposed = binding(&before);
    let disposed_ticket = disposed
        .prepare("disposed-ticket", SettingValue::Number(1.9), 1)
        .unwrap();
    disposed.dispose();
    assert_eq!(
        disposed.execute(&disposed_ticket, &mut owner),
        Err(SettingsBindingError::Disposed)
    );

    let after = owner.snapshot(&settings_reference()).unwrap();
    assert_eq!(zoom(&after).0, before_value);
    assert_eq!(zoom(&after).1, before_revision);
}

#[test]
fn storage_failure_from_real_owner_keeps_binding_guard_and_owned_bytes() {
    let root = tempfile::tempdir().unwrap();
    let mut owner = owner(root.path());
    let initial = owner.snapshot(&settings_reference()).unwrap();
    let mut binding = binding(&initial);
    let base_guard = binding.guard().clone();
    let ticket = binding
        .prepare("blocked-write", SettingValue::Number(1.9), 4)
        .unwrap();

    let target = application_settings_path(root.path());
    std::fs::create_dir_all(&target).unwrap();
    let sentinel = target.join("preserved");
    std::fs::write(&sentinel, b"pre-existing bytes").unwrap();

    let outcome = binding.execute(&ticket, &mut owner).unwrap();
    assert!(matches!(outcome, RevisionedWriteOutcome::Storage { .. }));
    assert_eq!(
        binding.complete(&ticket, outcome, 4),
        Ok(WriteDisposition::KeepDraft)
    );
    assert_eq!(binding.guard(), &base_guard);
    assert_eq!(
        owner.provider_snapshot().settings().ui_zoom,
        ApplicationSettings::default().ui_zoom
    );
    assert_eq!(std::fs::read(sentinel).unwrap(), b"pre-existing bytes");
}
