// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Shared in-process owner for application settings.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use mere_surface_api::settings::{
    CommittedSettingsWrite, RevisionedSettingSpec, RevisionedSettingsProvider,
    RevisionedSettingsSnapshot, RevisionedSettingsWriteRequest, RevisionedWriteOutcome,
    SettingValue, SettingsDomainRevision, SettingsError, SettingsOwnerIncarnation,
    SettingsProvider, SettingsRevisionDomain, SettingsRevisionGuard, SettingsStorageFailure,
    SettingsUnavailable, SettingsValidationFailure,
};
use pandect::ApplicationSettings;
use tabard::theme::choice::ThemeChoice;
use workbench::SettingsRef;

use super::{APPLICATION_REFERENCE, ApplicationSettingsProvider};

const APPEARANCE: &str = "appearance-choice";
const SHELL_CHROME: &str = "shell-chrome";
const BEHAVIORS: &str = "behaviors";
const THEME_CATALOG: &str = "theme-catalog";
const DOMAINS: [&str; 4] = [APPEARANCE, SHELL_CHROME, BEHAVIORS, THEME_CATALOG];

/// A shared in-process authority for Turnstone's application settings.
///
/// Clones serialize writes through one `RefCell` and compare revisions before
/// persistence. This guarantee covers callers that participate through these
/// shared-owner clones. The backend file replacement is not cross-process
/// compare-and-swap; another process or a direct writer can race between a
/// durable read and replacement, and the next owner read only observes the
/// resulting state. `provider_snapshot` is a detached projection for existing
/// readers and does not join that write-coordination boundary.
#[derive(Clone)]
pub struct SharedApplicationSettingsOwner {
    inner: Rc<RefCell<OwnerState>>,
}

struct OwnerState {
    provider: ApplicationSettingsProvider,
    incarnation: SettingsOwnerIncarnation,
    revisions: BTreeMap<String, u64>,
}

impl SharedApplicationSettingsOwner {
    /// Create a shared owner around one loaded persistence provider.
    pub fn new(provider: ApplicationSettingsProvider) -> Self {
        let revisions = DOMAINS
            .into_iter()
            .map(|domain| (domain.to_owned(), 0))
            .collect();
        Self {
            inner: Rc::new(RefCell::new(OwnerState {
                provider,
                incarnation: SettingsOwnerIncarnation(uuid::Uuid::new_v4().to_string()),
                revisions,
            })),
        }
    }

    /// Return the persistence root selected by the application.
    pub fn data_root(&self) -> PathBuf {
        self.inner.borrow().provider.data_root().to_path_buf()
    }

    /// Clone a provider projection for existing read-only consumers.
    pub fn provider_snapshot(&self) -> ApplicationSettingsProvider {
        self.inner.borrow().provider.clone()
    }

    /// Reload authored themes and advance only the catalog revision when its
    /// semantic contents or read status changed.
    pub fn reload_themes(&self) -> Result<(), SettingsError> {
        let mut state = self.inner.borrow_mut();
        observe_durable_settings(&mut state)?;
        let previous = state.provider.catalog().clone();
        state.provider.reload_themes();
        if *state.provider.catalog() != previous {
            let next = match next_revision(&state, THEME_CATALOG) {
                Ok(next) => next,
                Err(error) => {
                    state.provider.theme_catalog = previous;
                    return Err(error);
                },
            };
            state.revisions.insert(THEME_CATALOG.into(), next);
        }
        Ok(())
    }

    /// Serialized compatibility entry point for the existing theme workshop.
    /// It uses the same in-process owner lock and advances the choice revision
    /// only after the backend persists the choice successfully.
    pub fn apply_theme_choice(&self, choice: ThemeChoice) -> Result<(), SettingsError> {
        let mut state = self.inner.borrow_mut();
        observe_durable_settings(&mut state)?;
        let next = next_revision(&state, APPEARANCE)?;
        state.provider.apply_theme_choice(choice)?;
        state.revisions.insert(APPEARANCE.into(), next);
        Ok(())
    }
}

impl SettingsProvider for SharedApplicationSettingsOwner {
    fn describe(
        &self,
        reference: &SettingsRef,
    ) -> Result<Vec<mere_surface_api::settings::SettingSpec>, SettingsError> {
        let mut state = self.inner.borrow_mut();
        observe_durable_settings(&mut state)?;
        state.provider.describe(reference)
    }

    fn apply(
        &mut self,
        reference: &SettingsRef,
        setting_id: &str,
        value: SettingValue,
    ) -> Result<(), SettingsError> {
        let mut state = self.inner.borrow_mut();
        observe_durable_settings(&mut state)?;
        let domain = domain_for_setting(setting_id);
        let next = domain
            .map(|domain| next_revision(&state, domain))
            .transpose()?;
        state.provider.apply(reference, setting_id, value)?;
        if let (Some(domain), Some(next)) = (domain, next) {
            state.revisions.insert(domain.into(), next);
        }
        Ok(())
    }
}

impl RevisionedSettingsProvider for SharedApplicationSettingsOwner {
    fn snapshot(
        &self,
        reference: &SettingsRef,
    ) -> Result<RevisionedSettingsSnapshot, SettingsUnavailable> {
        let mut state = self.inner.borrow_mut();
        observe_durable_settings(&mut state).map_err(|error| SettingsUnavailable {
            message: format!("{error:?}"),
        })?;
        if reference.0 != APPLICATION_REFERENCE {
            return Err(SettingsUnavailable {
                message: format!("unsupported settings reference: {}", reference.0),
            });
        }
        let specs = state
            .provider
            .describe(reference)
            .map_err(|error| SettingsUnavailable {
                message: format!("{error:?}"),
            })?
            .into_iter()
            .filter_map(|spec| {
                let domain = domain_for_setting(&spec.id)?;
                Some(RevisionedSettingSpec {
                    spec,
                    guard: guard_for(&state, domain),
                })
            })
            .collect();
        Ok(snapshot_from_state(&state, reference.clone(), specs))
    }

    fn apply_revisioned(
        &mut self,
        request: RevisionedSettingsWriteRequest,
    ) -> RevisionedWriteOutcome {
        let request_id = request.request_id.clone();
        let mut state = self.inner.borrow_mut();
        if let Err(error) = observe_durable_settings(&mut state) {
            return RevisionedWriteOutcome::Storage {
                request_id,
                failure: SettingsStorageFailure {
                    message: format!("{error:?}"),
                },
            };
        }
        if request.reference.0 != APPLICATION_REFERENCE {
            return RevisionedWriteOutcome::Unavailable {
                request_id,
                reason: SettingsUnavailable {
                    message: format!("unsupported settings reference: {}", request.reference.0),
                },
            };
        }
        let Some(domain) = domain_for_setting(&request.setting_id) else {
            return RevisionedWriteOutcome::Validation {
                request_id,
                failure: SettingsValidationFailure {
                    setting_id: request.setting_id,
                    message: "unknown setting".into(),
                },
            };
        };
        let current = guard_for(&state, domain);
        if request.expected != current {
            return RevisionedWriteOutcome::Conflict {
                request_id,
                current,
            };
        }
        let next = match next_revision(&state, domain) {
            Ok(next) => next,
            Err(error) => {
                return RevisionedWriteOutcome::Storage {
                    request_id,
                    failure: SettingsStorageFailure {
                        message: format!("{error:?}"),
                    },
                };
            },
        };
        if let Err(error) = state.provider.apply(
            &request.reference,
            &request.setting_id,
            request.value.clone(),
        ) {
            return map_write_error(request_id, error);
        }
        state.revisions.insert(domain.into(), next);
        let committed_value = state
            .provider
            .describe(&request.reference)
            .ok()
            .and_then(|specs| {
                specs
                    .into_iter()
                    .find(|spec| spec.id == request.setting_id)
                    .map(|spec| spec.value)
            })
            .unwrap_or(request.value);
        RevisionedWriteOutcome::Committed(CommittedSettingsWrite {
            request_id,
            reference: request.reference,
            setting_id: request.setting_id,
            value: committed_value,
            guard: guard_for(&state, domain),
        })
    }
}

fn snapshot_from_state(
    state: &OwnerState,
    reference: SettingsRef,
    specs: Vec<RevisionedSettingSpec>,
) -> RevisionedSettingsSnapshot {
    RevisionedSettingsSnapshot {
        reference,
        owner: state.incarnation.clone(),
        revisions: DOMAINS
            .into_iter()
            .map(|domain| SettingsDomainRevision {
                domain: SettingsRevisionDomain(domain.into()),
                revision: state.revisions[domain],
            })
            .collect(),
        specs,
    }
}

fn guard_for(state: &OwnerState, domain: &str) -> SettingsRevisionGuard {
    SettingsRevisionGuard {
        owner: state.incarnation.clone(),
        domain: SettingsRevisionDomain(domain.into()),
        revision: state.revisions[domain],
    }
}

fn domain_for_setting(setting_id: &str) -> Option<&'static str> {
    if setting_id.starts_with("theme.") {
        Some(APPEARANCE)
    } else if setting_id == "ui.zoom" || setting_id.starts_with("chrome.") {
        Some(SHELL_CHROME)
    } else if setting_id.starts_with("behaviors.") {
        Some(BEHAVIORS)
    } else {
        None
    }
}

/// Read the durable base and fold externally observed axis changes into
/// this in-process owner's cache and revisions before any snapshot or write.
fn observe_durable_settings(state: &mut OwnerState) -> Result<ApplicationSettings, SettingsError> {
    let durable = state.provider.current_write_base()?;
    let cached = state.provider.settings();
    let changed = [
        (APPEARANCE, cached.theme != durable.theme),
        (
            SHELL_CHROME,
            cached.ui_zoom != durable.ui_zoom
                || cached.shellbar_edge != durable.shellbar_edge
                || cached.shellbar_hidden != durable.shellbar_hidden,
        ),
        (BEHAVIORS, cached.cascade_budget != durable.cascade_budget),
    ];
    let mut updates = Vec::new();
    for (domain, differs) in changed {
        if differs {
            updates.push((domain, next_revision(state, domain)?));
        }
    }
    state.provider.settings = durable.clone();
    for (domain, revision) in updates {
        state.revisions.insert(domain.into(), revision);
    }
    Ok(durable)
}

fn next_revision(state: &OwnerState, domain: &str) -> Result<u64, SettingsError> {
    state.revisions[domain]
        .checked_add(1)
        .ok_or_else(|| SettingsError::Storage(format!("settings revision overflow for {domain}")))
}

fn map_write_error(request_id: String, error: SettingsError) -> RevisionedWriteOutcome {
    match error {
        SettingsError::UnsupportedReference(reference) => RevisionedWriteOutcome::Unavailable {
            request_id,
            reason: SettingsUnavailable {
                message: format!("unsupported settings reference: {}", reference.0),
            },
        },
        SettingsError::UnknownSetting(setting_id) => RevisionedWriteOutcome::Validation {
            request_id,
            failure: SettingsValidationFailure {
                setting_id,
                message: "unknown setting".into(),
            },
        },
        SettingsError::InvalidValue {
            setting_id,
            message,
        } => RevisionedWriteOutcome::Validation {
            request_id,
            failure: SettingsValidationFailure {
                setting_id,
                message,
            },
        },
        SettingsError::Storage(message) => RevisionedWriteOutcome::Storage {
            request_id,
            failure: SettingsStorageFailure { message },
        },
    }
}

#[path = "owner_tests.rs"]
#[cfg(test)]
mod tests;
