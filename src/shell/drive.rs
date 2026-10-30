// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The automation surface: what a scenario drives turnstone through.
//!
//! `Automatable` grants the shared `resolve`, `click`, and `select-text` verbs,
//! so the app implements only what it alone knows: its DOMs (via a visitor,
//! since they sit behind `RefCell`), its retained text targets, its snapshot,
//! its labelled actions, and whether it is still busy. `Driveable` adds the two
//! the generic loop cannot do: a screenshot and turnstone's own verbs.

use super::{NodeSessions, NodeSurfaces};
use winit::event::MouseButton;
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};

use crate::action::Action;
use crate::panes::PaneContent;

use super::Shell;

/// A scenario wait polled once per normal rendered frame. Neither a remote
/// status change nor an exchange file may hold up presentation/event delivery.
pub(super) struct PendingScenarioWait {
    condition: WaitCondition,
    remaining: u32,
}
enum WaitCondition {
    File(std::path::PathBuf),
    Status(String),
}

#[cfg(test)]
#[path = "drive/pending_wait_tests.rs"]
mod pending_wait_tests;

#[cfg(test)]
#[path = "drive/foreign_a11y_receipt_tests.rs"]
mod foreign_a11y_receipt_tests;

struct IdleDiagnosis {
    pending_fetches: bool,
    requested_content: bool,
    graph_settling: bool,
    unsettled_sessions: Vec<uuid::Uuid>,
}

#[cfg(test)]
#[path = "drive/saved_reader_probe_tests.rs"]
mod saved_reader_probe_tests;

#[path = "drive/parse_public_fixture_a11y_receipt.rs"]
mod parse_public_fixture_a11y_receipt;
#[path = "drive/pending_scenario_wait_file.rs"]
mod pending_scenario_wait_file;
use parse_public_fixture_a11y_receipt::parse_public_fixture_a11y_receipt;
#[path = "drive/public_fixture_a11y_receipt.rs"]
mod public_fixture_a11y_receipt;
use public_fixture_a11y_receipt::public_fixture_a11y_receipt;
#[path = "drive/focused_saved_reader_node_for.rs"]
mod focused_saved_reader_node_for;
#[path = "drive/idle_diagnosis_is_busy.rs"]
mod idle_diagnosis_is_busy;
#[path = "drive/shell_idle_diagnosis.rs"]
mod shell_idle_diagnosis;
#[path = "drive/shell_wait_frames.rs"]
mod shell_wait_frames;
use focused_saved_reader_node_for::focused_saved_reader_node_for;
#[path = "drive/cmp_usize.rs"]
mod cmp_usize;
#[path = "drive/impl_taproot_automatable_for_shell.rs"]
mod impl_taproot_automatable_for_shell;
#[path = "drive/impl_taproot_driveable_for_shell.rs"]
mod impl_taproot_driveable_for_shell;
#[path = "drive/shell_adapter_busy.rs"]
mod shell_adapter_busy;
#[path = "drive/shell_adapter_snapshot.rs"]
mod shell_adapter_snapshot;
#[path = "drive/shell_adapter_with_surfaces.rs"]
mod shell_adapter_with_surfaces;
#[path = "drive/shell_focused_saved_reader_node.rs"]
mod shell_focused_saved_reader_node;
#[path = "drive/shell_run_scenario_step.rs"]
mod shell_run_scenario_step;
#[path = "drive/shell_scenario_assert_focused.rs"]
mod shell_scenario_assert_focused;
#[path = "drive/shell_scenario_assert_no_surface.rs"]
mod shell_scenario_assert_no_surface;
use cmp_usize::cmp_usize;
#[path = "drive/cmp_f32.rs"]
mod cmp_f32;
use cmp_f32::cmp_f32;
