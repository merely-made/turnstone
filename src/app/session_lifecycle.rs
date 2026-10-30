// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Session lifecycle: boot, mint, adopt, fork, trash.
//!
//! Adopting is the load half of boot AND the whole of a switch, so both go
//! through one path: swap the graph in place, re-dress it from the facet
//! store, rebuild the participant runtime, and reopen the session's own windows.
//! A session owns its arrangement, so its lens spaces travel with it.

use std::path::PathBuf;

use mere::canvas::Canvas;
use uuid::Uuid;

use crate::action::Effect;
use crate::content::ContentStates;
use crate::observe::AppEvent;
use crate::panes::{FrisketLayout, GraphId, PaneContent, SessionId};
use crate::session;
use crate::surface::FocusTarget;
use crate::ui::OmnibarState;

use super::App;

#[path = "session_lifecycle/app_adopt_session.rs"]
mod app_adopt_session;
#[path = "session_lifecycle/app_boot.rs"]
mod app_boot;
#[path = "session_lifecycle/app_fork_session_from.rs"]
mod app_fork_session_from;
#[path = "session_lifecycle/app_offer_place_prekey.rs"]
mod app_offer_place_prekey;
#[path = "session_lifecycle/app_refuse_session_adoption.rs"]
mod app_refuse_session_adoption;
