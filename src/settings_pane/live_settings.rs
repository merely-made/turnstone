// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Value-facing settings publication shared with the shell.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use pandect::ApplicationSettings;

use crate::settings_provider::{ApplicationSettingsProvider, SharedApplicationSettingsOwner};

use super::ChromeSettings;

/// A cloneable live projection owned by the shell. The provider publishes only
/// after it has persisted a successful write, so consumers cannot observe a
/// value that failed to reach the application owner.
#[derive(Clone)]
pub struct LiveSettingsHandle(Rc<RefCell<LiveSettingsState>>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OwnerAttachError {
    DifferentDataRoot {
        existing: PathBuf,
        requested: PathBuf,
    },
}

struct LiveSettingsState {
    snapshot: ChromeSettings,
    catalog_applied: bool,
    actions: std::collections::VecDeque<crate::action::Action>,
    notice: Option<String>,
    owner: Option<SharedApplicationSettingsOwner>,
}

impl LiveSettingsHandle {
    pub fn new(settings: &ApplicationSettings) -> Self {
        Self(Rc::new(RefCell::new(LiveSettingsState {
            snapshot: ChromeSettings::from(settings),
            catalog_applied: false,
            actions: Default::default(),
            notice: None,
            owner: None,
        })))
    }

    /// Build a shell-facing handle around the actual provider owner.
    pub(crate) fn from_provider(provider: ApplicationSettingsProvider) -> Self {
        let handle = Self::new(provider.settings());
        let owner = SharedApplicationSettingsOwner::new(provider);
        handle.0.borrow_mut().owner = Some(owner.clone());
        let snapshot = owner.provider_snapshot();
        handle.publish_provider(&snapshot);
        handle
    }

    /// Attach once to the provider-backed owner. Reuse it for another pane
    /// only when both panes address the same data root.
    pub(crate) fn owner_for(
        &self,
        provider: &ApplicationSettingsProvider,
    ) -> Result<SharedApplicationSettingsOwner, OwnerAttachError> {
        let requested = provider.data_root().to_path_buf();
        let mut state = self.0.borrow_mut();
        match &state.owner {
            Some(owner) if owner.data_root() == requested => Ok(owner.clone()),
            Some(owner) => Err(OwnerAttachError::DifferentDataRoot {
                existing: owner.data_root(),
                requested,
            }),
            None => {
                let owner = SharedApplicationSettingsOwner::new(provider.clone());
                state.owner = Some(owner.clone());
                Ok(owner)
            },
        }
    }

    pub(crate) fn owner(&self) -> Option<SharedApplicationSettingsOwner> {
        self.0.borrow().owner.clone()
    }

    #[cfg(test)]
    pub(crate) fn replace_owner_for_test(
        &self,
        provider: ApplicationSettingsProvider,
    ) -> SharedApplicationSettingsOwner {
        let owner = SharedApplicationSettingsOwner::new(provider);
        self.0.borrow_mut().owner = Some(owner.clone());
        let snapshot = owner.provider_snapshot();
        self.publish_provider(&snapshot);
        owner
    }

    pub fn snapshot(&self) -> ChromeSettings {
        self.0.borrow().snapshot.clone()
    }

    pub(crate) fn report_status(&self, status: Option<String>) {
        self.0.borrow_mut().notice = status;
    }

    pub(super) fn notice(&self) -> Option<String> {
        self.0.borrow().notice.clone()
    }

    pub(crate) fn request_action(&self, action: crate::action::Action) {
        self.0.borrow_mut().actions.push_back(action);
    }

    pub(crate) fn take_actions(&self) -> Vec<crate::action::Action> {
        self.0.borrow_mut().actions.drain(..).collect()
    }

    pub(crate) fn publish_provider(&self, provider: &ApplicationSettingsProvider) {
        let mut state = self.0.borrow_mut();
        let mut next = ChromeSettings::from_provider(provider);
        // Library refresh and unrelated setting writes must keep the applied
        // definition, even when an editor saved new bytes under the same id.
        if state.catalog_applied {
            next.theme_id = state.snapshot.theme_id.clone();
            next.theme_mode = state.snapshot.theme_mode.clone();
            next.presentation = state.snapshot.presentation.clone();
            next.light_presentation = state.snapshot.light_presentation.clone();
            next.typography = state.snapshot.typography.clone();
            next.interface_font_properties = state.snapshot.interface_font_properties.clone();
        }
        state.snapshot = next;
        state.catalog_applied = true;
    }

    pub(crate) fn apply_provider(&self, provider: &ApplicationSettingsProvider) {
        let mut state = self.0.borrow_mut();
        state.snapshot = ChromeSettings::from_provider(provider);
        state.catalog_applied = true;
    }
}
