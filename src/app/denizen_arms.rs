// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Gate-ring arms: install, uninstall, run a participant.
//!
//! Host-only by ring, so no grant can ever cover them: a component confirming
//! its own review would be self-escalation.

use std::time::Instant;
use uuid::Uuid;

use crate::action::{Action, Effect};
use crate::observe::AppEvent;
use crate::surface::FocusTarget;
use crate::ui::OmnibarState;

use super::App;

#[path = "denizen_arms/app_lower_behavior_actions.rs"]
mod app_lower_behavior_actions;
#[path = "denizen_arms/app_run_denizen_for_cascade.rs"]
mod app_run_denizen_for_cascade;
