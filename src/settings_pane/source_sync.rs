// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{
    arrangement::{SETTING_KIND, SettingSources},
    *,
};
use cambium::{SettingRowPhase, SettingRowSession, composition::ComponentKey};
use mere_surface_api::settings::{RevisionedSettingsSnapshot, SettingSpec};
use std::collections::BTreeMap;

pub(super) fn target(
    descriptor: &cambium::composition::ComponentDescriptor,
) -> Option<(SettingsRef, String)> {
    if descriptor.kind.0 != SETTING_KIND {
        return None;
    }
    Some((
        SettingsRef(descriptor.binding.get("reference")?.as_str()?.into()),
        descriptor.binding.get("setting_id")?.as_str()?.into(),
    ))
}

pub(super) fn current_spec<'a>(
    snapshot: &'a RevisionedSettingsSnapshot,
    setting_id: &str,
) -> Option<&'a SettingSpec> {
    snapshot
        .specs
        .iter()
        .find(|entry| entry.spec.id == setting_id)
        .map(|entry| &entry.spec)
}

pub(super) fn registered_specs(state: &SettingsState) -> SettingSources {
    state
        .snapshots
        .iter()
        .map(|(reference, snapshot)| {
            (
                reference.clone(),
                snapshot
                    .specs
                    .iter()
                    .map(|entry| (entry.spec.id.clone(), entry.spec.clone()))
                    .collect(),
            )
        })
        .collect()
}

pub(super) fn refresh_settings(state: &mut SettingsState) {
    let mut snapshots = BTreeMap::new();
    let mut unavailable = BTreeMap::new();
    for (reference, provider) in &state.sources {
        match provider.borrow().snapshot(&SettingsRef(reference.clone())) {
            Ok(snapshot) if snapshot.reference.0 == *reference => {
                snapshots.insert(reference.clone(), snapshot);
            },
            Ok(_) => {
                unavailable.insert(
                    reference.clone(),
                    "The source returned a different settings reference.".to_owned(),
                );
            },
            Err(error) => {
                unavailable.insert(reference.clone(), error.message);
            },
        }
    }
    state.snapshots = snapshots;
    update_revision_causes(state);
    state.arrangement.update_sources(registered_specs(state));
    reconcile_rows(state, &unavailable);
    if let Some(owner) = &state.owner {
        state.provider = owner.provider_snapshot();
        state.live_settings.publish_provider(&state.provider);
    }
}

/// Dirty rows keep their captured guard when the source value advances.
pub(super) fn reconcile_rows(state: &mut SettingsState, unavailable: &BTreeMap<String, String>) {
    let descriptors = state.arrangement.accepted().components.clone();
    state.rows.retain(|key, row| {
        let keep = descriptors.iter().any(|descriptor| {
            &descriptor.key == key
                && target(descriptor).as_ref().is_some_and(|(reference, id)| {
                    reference == &row.reference && id == &row.setting_id
                })
        });
        if !keep {
            if let Some(binding) = &mut row.binding {
                binding.dispose();
            }
        }
        keep
    });
    for descriptor in descriptors {
        let Some((reference, setting_id)) = target(&descriptor) else {
            continue;
        };
        let snapshot = state.snapshots.get(&reference.0);
        let spec = snapshot.and_then(|snapshot| current_spec(snapshot, &setting_id));
        if !state.rows.contains_key(&descriptor.key) {
            let (Some(snapshot), Some(spec)) = (snapshot, spec) else {
                continue;
            };
            let binding = SettingsBinding::from_snapshot(snapshot, setting_id.clone(), 8).ok();
            let session = SettingRowSession::new(spec.clone());
            if binding.is_none() {
                session.set_unavailable("The source returned an invalid binding snapshot.");
            }
            state.rows.insert(
                descriptor.key.clone(),
                RowState {
                    reference: reference.clone(),
                    setting_id: setting_id.clone(),
                    binding,
                    session,
                    reviewed: None,
                },
            );
        }
        let Some(row) = state.rows.get_mut(&descriptor.key) else {
            continue;
        };
        let (Some(snapshot), Some(spec)) = (snapshot, spec) else {
            row.reviewed = None;
            row.session
                .set_unavailable(unavailable.get(&reference.0).cloned().unwrap_or_else(|| {
                    "This saved setting is unavailable. Its descriptor and draft are retained."
                        .into()
                }));
            continue;
        };
        let before = row.session.snapshot();
        let foreign_owner = row
            .binding
            .as_ref()
            .is_none_or(|binding| binding.guard().owner != snapshot.owner);
        row.session.observe(spec.clone());
        if (before.dirty || before.pending_generation.is_some()) && foreign_owner {
            row.reviewed = None;
            row.session.set_unavailable(
                "The settings owner was replaced. Discard this edit to revalidate the new owner.",
            );
            continue;
        }
        if !row.session.snapshot().apply_compatible {
            row.reviewed = None;
            row.session.set_unavailable("This control changed while you were editing. Discard the edit to use the new control.");
            continue;
        }
        if !before.dirty && before.pending_generation.is_none() {
            let needs_adoption = row.binding.as_ref().is_none_or(|binding| {
                snapshot
                    .specs
                    .iter()
                    .find(|entry| entry.spec.id == setting_id)
                    .is_none_or(|entry| binding.guard() != &entry.guard)
            });
            if needs_adoption {
                let result = if let Some(binding) = &mut row.binding {
                    binding.adopt_snapshot(snapshot, setting_id.clone())
                } else {
                    SettingsBinding::from_snapshot(snapshot, setting_id.clone(), 8).map(|binding| {
                        row.binding = Some(binding);
                    })
                };
                if let Err(error) = result {
                    row.session
                        .set_unavailable(format!("Could not revalidate this setting: {error:?}"));
                }
            }
        } else if before.phase == SettingRowPhase::Unavailable && !foreign_owner {
            row.session.resume_available();
        }
    }
}

pub(super) fn refresh_row_snapshot(
    state: &mut SettingsState,
    key: &ComponentKey,
) -> Result<RevisionedSettingsSnapshot, String> {
    let reference = state
        .rows
        .get(key)
        .ok_or("This component has no available setting binding.")?
        .reference
        .clone();
    let setting_id = state.rows.get(key).unwrap().setting_id.clone();
    let source = state
        .sources
        .get(&reference.0)
        .ok_or("The settings source is unavailable.")?;
    let snapshot = source
        .borrow()
        .snapshot(&reference)
        .map_err(|error| error.message)?;
    if snapshot.reference != reference {
        return Err("The settings source returned a different reference.".into());
    }
    if current_spec(&snapshot, &setting_id).is_none() {
        return Err("The setting is unavailable.".into());
    }
    state.snapshots.insert(reference.0, snapshot.clone());
    update_revision_causes(state);
    state.arrangement.update_sources(registered_specs(state));
    reconcile_rows(state, &Default::default());
    if let Some(owner) = &state.owner {
        state.provider = owner.provider_snapshot();
        state.live_settings.publish_provider(&state.provider);
    }
    Ok(snapshot)
}

fn update_revision_causes(state: &mut SettingsState) {
    let mut revision_causes = BTreeMap::new();
    if let (Some(owner), Some(snapshot), Some(source_id)) = (
        &state.owner,
        state.snapshots.get(APPLICATION_REFERENCE),
        state.source_ids.get(APPLICATION_REFERENCE),
    ) && let Ok(owner_token) =
        super::composition::owner_token(state, *source_id, &snapshot.owner.0)
    {
        let causes = owner.latest_revision_causes();
        for revision in &snapshot.revisions {
            if let Some(cause) = causes.iter().find(|cause| {
                cause.domain == revision.domain.0 && cause.revision == revision.revision
            }) {
                revision_causes.insert(
                    (
                        *source_id,
                        owner_token,
                        cause.domain.clone(),
                        cause.revision,
                    ),
                    cause.cause,
                );
            }
        }
    }
    state.revision_causes = revision_causes;
}
