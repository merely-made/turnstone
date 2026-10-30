// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Node and view arms: what a node IS and how it is shown.
//!
//! Deleting stages into the recycle bin and recovery restores the ORIGINAL
//! id, which is the whole identity contract the bin exists for.

use uuid::Uuid;

use crate::action::{Action, Effect};
use crate::observe::AppEvent;
use crate::panes::PaneContent;
use crate::shell_services::{EntryPrivacy, ShellInput, ShellIntent, ShellOutcome};
use crate::surface::FocusTarget;
use crate::ui::{OmnibarState, Suggestion, normalize_address};

use super::App;

/// The Chromium user-agent zoom ladder: the notches its Zoom In / Zoom Out
/// commands step through. One copy, so the palette rows, the keyboard chords
/// and the ctrl+wheel gesture all land on the same rungs.
const PAGE_ZOOM_LADDER: [f32; 17] = [
    0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0,
];

/// A stored request need not sit exactly on a rung — an older ladder, a
/// hand-edited sidecar — so a step compares against the current value with
/// slack rather than searching for an exact match.
const PAGE_ZOOM_RUNG_EPSILON: f32 = 1e-4;

#[cfg(test)]
#[path = "node_arms/tests.rs"]
mod tests;

#[path = "node_arms/micron_field_editor_value.rs"]
mod micron_field_editor_value;
use micron_field_editor_value::micron_field_editor_value;
#[path = "node_arms/smolweb_query_url.rs"]
mod smolweb_query_url;
pub(crate) use smolweb_query_url::smolweb_query_url;
#[path = "node_arms/app_browser_command_target.rs"]
mod app_browser_command_target;
#[path = "node_arms/app_commit_gemini_identity.rs"]
mod app_commit_gemini_identity;
#[path = "node_arms/app_commit_omnibar.rs"]
mod app_commit_omnibar;
#[path = "node_arms/app_compose_focused_micron_form.rs"]
mod app_compose_focused_micron_form;
#[path = "node_arms/app_refuse_unzoomable.rs"]
mod app_refuse_unzoomable;
#[path = "node_arms/app_set_layout_strategy.rs"]
mod app_set_layout_strategy;
