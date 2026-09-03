// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Retained, app-owned state for web permission and authentication decisions.
//!
//! The shell owns the pending backend callbacks and profile policy service.
//! This module holds only the visible request identity, public origin or
//! protection-space description, and a short-lived credential draft. Secret
//! text uses [`crate::action::SensitiveString`], whose debug representation is
//! always redacted, and is never projected through observation.

use std::collections::VecDeque;

use inker::{PermissionDescriptor, UserAgentRequestId};
use uuid::Uuid;

use crate::action::SensitiveString;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserAgentRequestKey {
    pub node: Uuid,
    pub id: UserAgentRequestId,
}

impl UserAgentRequestKey {
    pub const fn new(node: Uuid, id: UserAgentRequestId) -> Self {
        Self { node, id }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionChoice {
    AllowOnce,
    AlwaysAllow,
    Deny,
    /// Close the prompt without retaining either grant or denial. Used by the
    /// Escape key and timeout/navigation withdrawal paths, never as the named
    /// deny choice.
    Dismiss,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AuthenticationField {
    #[default]
    Username,
    Password,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingUserAgentDecision {
    Permission {
        request: UserAgentRequestKey,
        origin: String,
        descriptors: Vec<PermissionDescriptor>,
    },
    Authentication {
        request: UserAgentRequestKey,
        host: String,
        port: u16,
        realm: Option<String>,
        scheme: String,
        is_proxy: bool,
    },
}

impl PendingUserAgentDecision {
    pub const fn request(&self) -> UserAgentRequestKey {
        match self {
            Self::Permission { request, .. } | Self::Authentication { request, .. } => *request,
        }
    }

    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Permission { .. } => "permission",
            Self::Authentication { .. } => "authentication",
        }
    }

    pub fn prompt(&self) -> String {
        match self {
            Self::Permission {
                origin,
                descriptors,
                ..
            } => format!(
                "{origin} wants {}",
                descriptors
                    .iter()
                    .map(permission_label)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Authentication {
                host,
                port,
                realm,
                scheme,
                is_proxy,
                ..
            } => {
                let authority = if *port == 80 || *port == 443 {
                    host.clone()
                } else {
                    format!("{host}:{port}")
                };
                let realm = realm
                    .as_deref()
                    .filter(|realm| !realm.trim().is_empty())
                    .map(|realm| format!(" for {realm}"))
                    .unwrap_or_default();
                let proxy = if *is_proxy { "proxy " } else { "" };
                format!("Sign in to {proxy}{authority}{realm} with {scheme}")
            }
        }
    }
}

fn permission_label(descriptor: &PermissionDescriptor) -> String {
    match descriptor {
        PermissionDescriptor::Camera => "camera".into(),
        PermissionDescriptor::Microphone => "microphone".into(),
        PermissionDescriptor::Geolocation => "location".into(),
        PermissionDescriptor::Notifications => "notifications".into(),
        PermissionDescriptor::ClipboardRead => "clipboard reading".into(),
        PermissionDescriptor::Midi { sysex: false } => "MIDI".into(),
        PermissionDescriptor::Midi { sysex: true } => "MIDI system-exclusive access".into(),
        PermissionDescriptor::PointerLock => "pointer lock".into(),
        PermissionDescriptor::KeyboardLock => "keyboard lock".into(),
        PermissionDescriptor::IdleDetection => "idle detection".into(),
        PermissionDescriptor::LocalFonts => "local fonts".into(),
        PermissionDescriptor::StorageAccess => "storage access".into(),
        PermissionDescriptor::ProtectedMediaIdentifier => "protected-media identity".into(),
        PermissionDescriptor::DisplayCapture {
            audio: true,
            video: true,
        } => "screen and audio capture".into(),
        PermissionDescriptor::DisplayCapture {
            audio: true,
            video: false,
        } => "audio capture".into(),
        PermissionDescriptor::DisplayCapture {
            audio: false,
            video: true,
        } => "screen capture".into(),
        PermissionDescriptor::DisplayCapture {
            audio: false,
            video: false,
        } => "display capture".into(),
        PermissionDescriptor::Other(name) => name.clone(),
        _ => "an engine-defined permission".into(),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthenticationDraft {
    pub field: AuthenticationField,
    pub username: SensitiveString,
    pub password: SensitiveString,
    pub remember_for_process: bool,
}

impl AuthenticationDraft {
    pub fn password_mask(&self) -> String {
        "\u{2022}".repeat(self.password.as_str().chars().count())
    }

    fn active_mut(&mut self) -> &mut SensitiveString {
        match self.field {
            AuthenticationField::Username => &mut self.username,
            AuthenticationField::Password => &mut self.password,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserAgentDecisionState {
    pending: VecDeque<PendingUserAgentDecision>,
    pub authentication: AuthenticationDraft,
    pub submitting: Option<UserAgentRequestKey>,
    pub error: Option<String>,
}

impl UserAgentDecisionState {
    pub fn active(&self) -> Option<&PendingUserAgentDecision> {
        self.pending.front()
    }

    pub fn active_request(&self) -> Option<UserAgentRequestKey> {
        self.active().map(PendingUserAgentDecision::request)
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_open(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn accepts_text(&self) -> bool {
        matches!(
            self.active(),
            Some(PendingUserAgentDecision::Authentication { .. })
        ) && self.submitting.is_none()
    }

    pub fn receive(&mut self, decision: PendingUserAgentDecision) -> bool {
        let request = decision.request();
        if self
            .pending
            .iter()
            .any(|pending| pending.request() == request)
        {
            return false;
        }
        let was_empty = self.pending.is_empty();
        self.pending.push_back(decision);
        if was_empty {
            self.authentication = AuthenticationDraft::default();
            self.error = None;
        }
        true
    }

    pub fn begin(&mut self, request: UserAgentRequestKey) -> bool {
        if self.active_request() != Some(request) || self.submitting.is_some() {
            return false;
        }
        self.submitting = Some(request);
        self.error = None;
        true
    }

    pub fn finish(
        &mut self,
        request: UserAgentRequestKey,
        terminal: bool,
        error: Option<String>,
    ) -> bool {
        if self.submitting == Some(request) {
            self.submitting = None;
        }
        if terminal {
            let changed = self.remove(request);
            // The callback is gone even when durable policy persistence
            // failed. Do not pin that terminal error onto the next queued
            // request; the sanitized outcome event carries the failure.
            self.error = None;
            changed
        } else if self.active_request() == Some(request) {
            self.error = error;
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, request: UserAgentRequestKey) -> bool {
        let Some(index) = self
            .pending
            .iter()
            .position(|pending| pending.request() == request)
        else {
            return false;
        };
        let active_changed = index == 0;
        self.pending.remove(index);
        if active_changed {
            self.authentication = AuthenticationDraft::default();
            self.error = None;
        }
        if self.submitting == Some(request) {
            self.submitting = None;
        }
        true
    }

    pub fn clear_node(&mut self, node: Uuid) -> Vec<UserAgentRequestKey> {
        let active = self.active_request();
        let removed = self
            .pending
            .iter()
            .filter_map(|pending| {
                let request = pending.request();
                (request.node == node).then_some(request)
            })
            .collect::<Vec<_>>();
        self.pending
            .retain(|pending| pending.request().node != node);
        if active.is_some_and(|request| request.node == node) {
            self.authentication = AuthenticationDraft::default();
            self.error = None;
        }
        if self.submitting.is_some_and(|request| request.node == node) {
            self.submitting = None;
        }
        removed
    }

    pub fn focus_authentication(&mut self, field: AuthenticationField) -> bool {
        if !self.accepts_text() {
            return false;
        }
        let changed = self.authentication.field != field;
        self.authentication.field = field;
        changed
    }

    pub fn toggle_authentication_field(&mut self) -> bool {
        let field = match self.authentication.field {
            AuthenticationField::Username => AuthenticationField::Password,
            AuthenticationField::Password => AuthenticationField::Username,
        };
        self.focus_authentication(field)
    }

    pub fn insert_authentication(&mut self, text: &str) -> bool {
        if !self.accepts_text() || text.is_empty() {
            return false;
        }
        self.authentication.active_mut().push_str(text);
        self.error = None;
        true
    }

    pub fn backspace_authentication(&mut self) -> bool {
        if !self.accepts_text() {
            return false;
        }
        let changed = self.authentication.active_mut().pop().is_some();
        if changed {
            self.error = None;
        }
        changed
    }

    pub fn toggle_remember_for_process(&mut self) -> bool {
        if !self.accepts_text() {
            return false;
        }
        self.authentication.remember_for_process = !self.authentication.remember_for_process;
        true
    }
}
