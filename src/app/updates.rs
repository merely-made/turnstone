// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The port answers: every `Update` a port hands back, folded into app truth.
//!
//! The other half of the spine from `update`. Ports never touch state; they
//! answer with one of these and the fold happens here.

use crate::action::{Effect, Update};
use crate::browse;
use crate::observe::AppEvent;

use super::App;

#[path = "updates/app_apply_update.rs"]
mod app_apply_update;
#[path = "updates/app_update_gemini_certificate_changed.rs"]
mod app_update_gemini_certificate_changed;
#[path = "updates/app_update_place_invited.rs"]
mod app_update_place_invited;
#[path = "updates/app_update_saved_sources_loaded.rs"]
mod app_update_saved_sources_loaded;
