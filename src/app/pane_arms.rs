// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Pane-ring arms: arrangement edits.
//!
//! Every one lands on a leaf in whichever space holds it, primary or lens, so
//! the same op works in any window and persists with that space.

use uuid::Uuid;

use crate::action::{Action, Effect, SpaceRef, WbAxis};
use crate::observe::AppEvent;
use crate::panes::{
    GraphId, InsertSide, PaneContent, PaneId, PaneKindId, PaneMultiplicity, PaneNode, kind,
};

use super::App;

#[cfg(test)]
#[path = "pane_arms/tests.rs"]
mod tests;

#[path = "pane_arms/app_reveal_pane.rs"]
mod app_reveal_pane;
#[path = "pane_arms/app_tear_out_tile.rs"]
mod app_tear_out_tile;
