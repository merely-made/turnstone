// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::path::Path;

use apparatus_core::{RetentionLimits, RunId, SourceBootId};
use cambium::reactive_diagnostics::{DiagnosticKind, ReactiveDiagnosticEvent};
use mere_surface_api::settings::{
    RevisionedSettingsProvider, RevisionedSettingsSnapshot, RevisionedSettingsWriteRequest,
    RevisionedWriteOutcome, SettingValue, SettingsRevisionGuard,
};
use pandect::{application_settings_path, save_application_settings};
use tracing_subscriber::layer::SubscriberExt;

fn settings_snapshot(owner: &SharedApplicationSettingsOwner) -> RevisionedSettingsSnapshot {
    owner
        .snapshot(&SettingsRef(APPLICATION_REFERENCE.into()))
        .unwrap()
}

fn spec_guard(snapshot: &RevisionedSettingsSnapshot, id: &str) -> SettingsRevisionGuard {
    snapshot
        .specs
        .iter()
        .find(|entry| entry.spec.id == id)
        .unwrap_or_else(|| panic!("missing setting {id}"))
        .guard
        .clone()
}

fn domain_revision(snapshot: &RevisionedSettingsSnapshot, name: &str) -> u64 {
    snapshot
        .revisions
        .iter()
        .find(|entry| entry.domain.0 == name)
        .unwrap_or_else(|| panic!("missing domain {name}"))
        .revision
}

fn write_request(
    snapshot: &RevisionedSettingsSnapshot,
    request_id: &str,
    setting_id: &str,
    value: SettingValue,
) -> RevisionedSettingsWriteRequest {
    RevisionedSettingsWriteRequest {
        request_id: request_id.into(),
        reference: snapshot.reference.clone(),
        setting_id: setting_id.into(),
        expected: spec_guard(snapshot, setting_id),
        value,
    }
}

fn make_owner(root: &Path) -> SharedApplicationSettingsOwner {
    SharedApplicationSettingsOwner::new(ApplicationSettingsProvider::from_settings(
        root,
        ApplicationSettings::default(),
    ))
}

#[test]
fn owner_clones_share_committed_values_and_guards() {
    let temporary = tempfile::tempdir().unwrap();
    let mut owner = make_owner(temporary.path());
    let reader = owner.clone();
    let before = settings_snapshot(&owner);
    let outcome = owner.apply_revisioned(write_request(
        &before,
        "zoom-1",
        "ui.zoom",
        SettingValue::Number(1.5),
    ));
    let RevisionedWriteOutcome::Committed(committed) = outcome else {
        panic!("current guard should commit");
    };
    assert_eq!(committed.request_id, "zoom-1");
    assert_eq!(
        committed.guard.revision,
        spec_guard(&before, "ui.zoom").revision + 1
    );
    let after = settings_snapshot(&reader);
    let zoom = after
        .specs
        .iter()
        .find(|entry| entry.spec.id == "ui.zoom")
        .unwrap();
    assert_eq!(zoom.spec.value, SettingValue::Number(1.5));
    assert_eq!(zoom.guard, committed.guard);
}

#[test]
fn owner_retains_revision_advance_cause_for_the_committed_domain() {
    let temporary = tempfile::tempdir().unwrap();
    let mut owner = make_owner(temporary.path());
    let before = settings_snapshot(&owner);
    assert!(owner.latest_revision_causes().is_empty());
    let outcome = owner.apply_revisioned(write_request(
        &before,
        "cause-check",
        "ui.zoom",
        SettingValue::Number(1.5),
    ));
    let RevisionedWriteOutcome::Committed(committed) = outcome else {
        panic!("current guard should commit");
    };
    let causes = owner.latest_revision_causes();
    let cause = causes
        .iter()
        .find(|cause| cause.domain == SHELL_CHROME)
        .expect("committed domain has a causal head");
    assert_eq!(cause.revision, committed.guard.revision);
    assert_ne!(cause.cause.producer_id, 0);
    assert!(causes.iter().all(|cause| cause.domain.len() <= 64));
}

#[test]
fn guarded_revision_diagnostics_carry_numeric_domain_and_revision_facts() {
    let run = RunId::from("owner-domain-facts-run");
    let boot = SourceBootId::from("owner-domain-facts-boot");
    let (layer, receiver) = crate::reactive_trace::channel(run.clone(), boot.clone());
    let subscriber = tracing_subscriber::registry().with(layer);
    let temporary = tempfile::tempdir().unwrap();
    let before = {
        let mut captured = None;
        tracing::subscriber::with_default(subscriber, || {
            let mut owner = make_owner(temporary.path());
            let snapshot = settings_snapshot(&owner);
            let write = owner.apply_revisioned(write_request(
                &snapshot,
                "domain-fact-commit",
                "ui.zoom",
                SettingValue::Number(1.5),
            ));
            assert!(matches!(write, RevisionedWriteOutcome::Committed(_)));
            let conflict = owner.apply_revisioned(write_request(
                &snapshot,
                "domain-fact-conflict",
                "ui.zoom",
                SettingValue::Number(1.8),
            ));
            assert!(matches!(conflict, RevisionedWriteOutcome::Conflict { .. }));
            captured = Some(snapshot);
        });
        captured.unwrap()
    };

    let mut observations = crate::diagnostic_observations::DiagnosticObservations::new(
        run,
        RetentionLimits {
            max_records: 256,
            max_bytes: 1_048_576,
            max_age: std::time::Duration::from_secs(300),
        },
    );
    observations.attach_reactive(receiver, &boot).unwrap();
    let mut cursor = observations.cursor();
    let records = observations.read(&mut cursor, 256).unwrap();
    let mut advanced = None;
    let mut conflict = None;
    let serialized = serde_json::to_string(
        &records
            .records
            .iter()
            .filter_map(|record| record.payload.reactive.as_ref())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    for record in &records.records {
        let Some(crate::reactive_trace::ReactiveTracePayload {
            event:
                crate::reactive_trace::ReactiveDiagnosticPayload::Cambium(
                    ReactiveDiagnosticEvent::Decision { kind, details, .. },
                ),
            ..
        }) = record.payload.reactive.as_ref()
        else {
            continue;
        };
        match kind {
            DiagnosticKind::RevisionAdvanced if details.dependency_id == Some(2) => {
                advanced = Some(*details);
            },
            DiagnosticKind::SettingsConflict if details.dependency_id == Some(2) => {
                conflict = Some(*details);
            },
            _ => {},
        }
    }
    let advanced = advanced.expect("shell-chrome revision advance has numeric domain id");
    assert_eq!(advanced.revision, Some(1));
    let conflict = conflict.expect("guard conflict has numeric domain id");
    assert_eq!(conflict.expected_revision, Some(0));
    assert_eq!(conflict.current_revision, Some(1));
    assert_eq!(conflict.dependency_id, Some(2));
    assert_eq!(
        before
            .specs
            .iter()
            .find(|entry| entry.spec.id == "ui.zoom")
            .unwrap()
            .guard
            .revision,
        0
    );
    assert!(!serialized.contains("ui.zoom"));
    assert!(!serialized.contains("1.8"));
    assert!(!serialized.contains(temporary.path().to_string_lossy().as_ref()));
    assert_eq!(dependency_id_for_domain(APPEARANCE), Some(1));
    assert_eq!(dependency_id_for_domain(SHELL_CHROME), Some(2));
    assert_eq!(dependency_id_for_domain(BEHAVIORS), Some(3));
    assert_eq!(dependency_id_for_domain(THEME_CATALOG), Some(4));
}

#[test]
fn delayed_stale_same_domain_write_conflicts_without_overwrite() {
    let temporary = tempfile::tempdir().unwrap();
    let mut owner = make_owner(temporary.path());
    let stale = settings_snapshot(&owner);
    let first = owner.apply_revisioned(write_request(
        &stale,
        "first",
        "ui.zoom",
        SettingValue::Number(1.5),
    ));
    assert!(matches!(first, RevisionedWriteOutcome::Committed(_)));

    let delayed = owner.apply_revisioned(write_request(
        &stale,
        "delayed",
        "ui.zoom",
        SettingValue::Number(1.8),
    ));
    assert!(matches!(
        delayed,
        RevisionedWriteOutcome::Conflict { request_id, .. } if request_id == "delayed"
    ));
    let current = settings_snapshot(&owner);
    assert_eq!(
        current
            .specs
            .iter()
            .find(|entry| entry.spec.id == "ui.zoom")
            .unwrap()
            .spec
            .value,
        SettingValue::Number(1.5)
    );
}

#[test]
fn unrelated_domain_write_does_not_conflict_with_prior_guard() {
    let temporary = tempfile::tempdir().unwrap();
    let mut owner = make_owner(temporary.path());
    let before = settings_snapshot(&owner);
    let appearance_guard = spec_guard(&before, "theme.mode");

    let shell_write = owner.apply_revisioned(write_request(
        &before,
        "zoom",
        "ui.zoom",
        SettingValue::Number(1.6),
    ));
    assert!(matches!(shell_write, RevisionedWriteOutcome::Committed(_)));

    let mut appearance_request = write_request(
        &before,
        "appearance",
        "theme.mode",
        SettingValue::Text("system".into()),
    );
    appearance_request.expected = appearance_guard;
    let appearance_write = owner.apply_revisioned(appearance_request);
    assert!(matches!(
        appearance_write,
        RevisionedWriteOutcome::Committed(_)
    ));
    let after = settings_snapshot(&owner);
    assert_eq!(
        domain_revision(&after, SHELL_CHROME),
        domain_revision(&before, SHELL_CHROME) + 1
    );
    assert_eq!(
        domain_revision(&after, APPEARANCE),
        domain_revision(&before, APPEARANCE) + 1
    );
}

#[test]
fn old_owner_incarnation_is_refused_with_reused_numeric_revision() {
    let temporary = tempfile::tempdir().unwrap();
    let old_owner = make_owner(temporary.path());
    let old_snapshot = settings_snapshot(&old_owner);
    let mut new_owner = make_owner(temporary.path());
    let new_snapshot = settings_snapshot(&new_owner);
    assert_ne!(old_snapshot.owner, new_snapshot.owner);
    assert_eq!(
        spec_guard(&old_snapshot, "ui.zoom").revision,
        spec_guard(&new_snapshot, "ui.zoom").revision
    );

    let outcome = new_owner.apply_revisioned(write_request(
        &old_snapshot,
        "foreign-owner",
        "ui.zoom",
        SettingValue::Number(1.7),
    ));
    assert!(matches!(outcome, RevisionedWriteOutcome::Conflict { .. }));
    assert_eq!(
        settings_snapshot(&old_owner)
            .specs
            .iter()
            .find(|entry| entry.spec.id == "ui.zoom")
            .unwrap()
            .spec
            .value,
        SettingValue::Number(f64::from(ApplicationSettings::default().ui_zoom))
    );
}

#[test]
fn failed_filesystem_write_publishes_no_setting_or_revision() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let mut owner = make_owner(root);
    let before = settings_snapshot(&owner);
    let app_dir = application_settings_path(root)
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::create_dir_all(&app_dir).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&app_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let outcome = owner.apply_revisioned(write_request(
            &before,
            "blocked-write",
            "ui.zoom",
            SettingValue::Number(1.9),
        ));
        std::fs::set_permissions(&app_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            matches!(outcome, RevisionedWriteOutcome::Storage { request_id, .. } if request_id == "blocked-write")
        );
        let after = settings_snapshot(&owner);
        assert_eq!(
            domain_revision(&after, SHELL_CHROME),
            domain_revision(&before, SHELL_CHROME)
        );
        assert_eq!(
            after
                .specs
                .iter()
                .find(|entry| entry.spec.id == "ui.zoom")
                .unwrap()
                .spec
                .value,
            before
                .specs
                .iter()
                .find(|entry| entry.spec.id == "ui.zoom")
                .unwrap()
                .spec
                .value
        );
        assert!(!application_settings_path(root).exists());
    }
}

#[test]
fn externally_replaced_axes_advance_once_then_equal_reads_stay_stable() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let owner = make_owner(root);
    let before = settings_snapshot(&owner);
    let mut replacement = ApplicationSettings::default();
    replacement.shellbar_hidden = true;
    replacement.cascade_budget = 7;
    save_application_settings(root, &replacement).unwrap();

    let observed = settings_snapshot(&owner);
    assert_eq!(
        domain_revision(&observed, APPEARANCE),
        domain_revision(&before, APPEARANCE)
    );
    assert_eq!(
        domain_revision(&observed, SHELL_CHROME),
        domain_revision(&before, SHELL_CHROME) + 1
    );
    assert_eq!(
        domain_revision(&observed, BEHAVIORS),
        domain_revision(&before, BEHAVIORS) + 1
    );
    let repeated = settings_snapshot(&owner);
    assert_eq!(repeated.revisions, observed.revisions);
}

#[test]
fn same_id_library_refresh_advances_catalog_not_choice_and_repeated_reads_do_not_bump() {
    use tabard::{Theme, library::ThemeLibraryStore, theme::registry::ThemeRegistry};

    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("profile");
    let library = temporary.path().join("shared/themes.json");
    let mut theme = Theme::new(
        "theme:shared-owner-refresh",
        "Original name",
        ThemeRegistry::default().list()[0].seeds,
    );
    ThemeLibraryStore::load(&library)
        .unwrap()
        .save(&[theme.clone()])
        .unwrap();
    let provider =
        ApplicationSettingsProvider::from_settings(&root, ApplicationSettings::default())
            .with_theme_library(&library);
    let owner = SharedApplicationSettingsOwner::new(provider);
    let initial = settings_snapshot(&owner);

    theme.name = "Refreshed name".into();
    ThemeLibraryStore::load(&library)
        .unwrap()
        .save(&[theme])
        .unwrap();
    owner.reload_themes().unwrap();
    let refreshed = settings_snapshot(&owner);
    assert_eq!(
        domain_revision(&refreshed, APPEARANCE),
        domain_revision(&initial, APPEARANCE)
    );
    assert_eq!(
        domain_revision(&refreshed, THEME_CATALOG),
        domain_revision(&initial, THEME_CATALOG) + 1
    );
    assert_eq!(owner.provider_snapshot().settings().theme, None);
    owner.reload_themes().unwrap();
    assert_eq!(settings_snapshot(&owner).revisions, refreshed.revisions);
}

#[test]
fn same_value_explicit_theme_apply_advances_choice_revision_and_preserves_choice() {
    use tabard::theme::{choice::ThemeChoice, registry::Mode};

    let temporary = tempfile::tempdir().unwrap();
    let mut settings = ApplicationSettings::default();
    settings.theme = Some(ThemeChoice::new("theme:dark", Some(Mode::Dark)));
    let owner = SharedApplicationSettingsOwner::new(ApplicationSettingsProvider::from_settings(
        temporary.path(),
        settings.clone(),
    ));
    let before = settings_snapshot(&owner);
    let request = write_request(
        &before,
        "explicit-theme-apply",
        "theme.mode",
        SettingValue::Text("dark".into()),
    );
    let mut writer = owner.clone();
    let outcome = writer.apply_revisioned(request);
    assert!(matches!(outcome, RevisionedWriteOutcome::Committed(_)));
    let after = settings_snapshot(&owner);
    assert_eq!(
        domain_revision(&after, APPEARANCE),
        domain_revision(&before, APPEARANCE) + 1
    );
    assert_eq!(owner.provider_snapshot().settings().theme, settings.theme);
}
