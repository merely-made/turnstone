// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The action catalog: what the app offers right now, and in what order.
//!
//! Contextual rows lead the static registry, because a pending grant review
//! must be the first thing an opened palette shows. One composition, read by
//! the `>` lane, the snapshot, and the automation runner alike — composing it
//! twice is how they come to disagree about what a label means.

use crate::action::{Action, Effect};
use crate::observe::AppEvent;
use crate::panes::PaneContent;

use super::{App, pane_label};

/// The commands Turnstone keeps by default (Mark, 2026-10-08): the browsing
/// eight, by id. A shared verb's id is the catalogue's; the rest keep their
/// label as a host-owned id. A person can drop any of them and keep any other
/// command.
pub(crate) const DEFAULT_KEPT_COMMANDS: [&str; 8] = [
    cambium::catalogue::ids::NAV_BACK,
    cambium::catalogue::ids::NAV_FORWARD,
    cambium::catalogue::ids::NAV_RELOAD,
    cambium::catalogue::ids::NAV_STOP,
    "Toggle live content",
    "Open node in Workbench",
    cambium::catalogue::ids::VIEW_FIT,
    cambium::catalogue::ids::SESSION_SAVE,
];

/// The command category of the rows composed ahead of the static registry.
pub(crate) const CONTEXT_COMMANDS: &str = "context";
/// The command category of the static registry's host-owned rows. A shared
/// verb takes its catalogue namespace instead.
const STATIC_COMMANDS: &str = "turnstone";
/// The bare `>` lane's last row, which expands it to the whole catalog.
pub(crate) const ALL_COMMANDS_ROW: &str = "All commands\u{2026}";

/// The catalog registered as a command set: each row once, under its shared
/// id or its label, with the action it runs.
pub(crate) struct CommandCatalog {
    pub(crate) set: cambium::CommandSet,
    actions: std::collections::HashMap<String, Action>,
    /// A shown label to the id it registered under; the first row wins a
    /// shared label, as it does for the automation runner.
    ids: std::collections::HashMap<String, String>,
}

/// Which labels are commands, and which of those the person keeps.
pub(crate) struct CommandKeepStates {
    registered: std::collections::HashSet<String>,
    kept: std::collections::HashSet<String>,
}

#[cfg(test)]
#[path = "palette/tests.rs"]
mod tests;

/// The `>` lane read through Cambium's command set (Scenograph editor plan
/// SE28 to SE32).
#[cfg(test)]
#[path = "palette/command_menu_tests.rs"]
mod command_menu_tests;

#[path = "palette/app_lower_denizen_actions.rs"]
mod app_lower_denizen_actions;
#[path = "palette/app_place_collection_actions.rs"]
mod app_place_collection_actions;
#[path = "palette/collection_id_suffix.rs"]
mod collection_id_suffix;
#[path = "palette/command_catalog_id_of.rs"]
mod command_catalog_id_of;
#[path = "palette/command_keep_states_of.rs"]
mod command_keep_states_of;
use collection_id_suffix::collection_id_suffix;
#[path = "palette/collection_id_hex.rs"]
mod collection_id_hex;
use collection_id_hex::collection_id_hex;
