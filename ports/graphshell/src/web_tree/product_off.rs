// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Local saved-graph editing and intake with the `product` feature off (the viewer cone,
//! mer3ly Rulings 110 and 119): `product.rs`'s API, inert. `open` never finds a
//! saved graph, so no `SavedProduct` exists and its methods cannot run.
use super::*;
use cambium::{SelectState, TextInput};
use std::convert::Infallible;
use uuid::Uuid;

/// Never built: the uninhabited field keeps it unconstructible.
pub(super) struct SavedProduct {
    pub(super) selected: Option<Uuid>,
    pub(super) title: TextInput,
    pub(super) tags: TextInput,
    pub(super) address: String,
    pub(super) detail_open: bool,
    pub(super) role: SelectState,
    pub(super) saving: bool,
    pub(super) status: String,
    pub(super) storage: String,
    pub(super) save_state: &'static str,
    pub(super) session: String,
    pub(super) reopened: bool,
    never: Infallible,
}

pub(super) async fn open(_root: &Element) -> Result<Option<SavedProduct>, String> {
    Ok(None)
}

pub(super) fn focused_text(
    _runner: &cambium_rootstock::Runner<TreePage, Logic, Child>,
) -> Option<cambium_rootstock::FocusedTextSlot<TreePage>> {
    None
}

impl SavedProduct {
    pub(super) fn authority_nodes(&self) -> Option<usize> {
        match self.never {}
    }
    pub(super) fn load_projection_input(&mut self, _: &str, _: Option<&str>) {
        match self.never {}
    }
    pub(super) fn history_fields(&self, _snapshot: ProbeSnapshot) -> ProbeSnapshot {
        match self.never {}
    }
    pub(super) fn projection_fields(&self, snapshot: ProbeSnapshot) -> ProbeSnapshot {
        match self.never {}
    }
    pub(super) fn filter_count(&self) -> usize {
        match self.never {}
    }
    pub(super) fn saved_scene_facet(&self) -> &'static str {
        match self.never {}
    }
    pub(super) fn export_bytes(&self) -> usize {
        match self.never {}
    }
    pub(super) fn imported_nodes(&self) -> usize {
        match self.never {}
    }
    pub(super) fn action_count(&self) -> u64 {
        match self.never {}
    }
    pub(super) fn detail_facet(&self) -> String {
        match self.never {}
    }
    pub(super) fn ready(&self) -> bool {
        match self.never {}
    }

    pub(super) fn selection_locked(&self) -> bool {
        false
    }

    pub(super) fn initial_fixture_positions(
        &self,
    ) -> Option<
        Vec<(
            mere::kernel::graph::NodeKey,
            mere::kernel::geometry::PortablePoint,
        )>,
    > {
        match self.never {}
    }
    pub(super) fn link_visibility_state(&self) -> (&'static str, &str) {
        match self.never {}
    }
    pub(super) fn assertion_visibility_state(&self) -> (&'static str, &str) {
        match self.never {}
    }
    pub(super) fn restore_canvas_link_hides(&mut self, _canvas: &mut Canvas) {
        match self.never {}
    }

    pub(super) fn graph(&self) -> Graph {
        match self.never {}
    }

    pub(super) fn inspect_link_history(
        &self,
        _target: graphshell::link_inspection::LinkTarget,
    ) -> Result<graphshell::link_inspection::LinkHistoryInspection, String> {
        match self.never {}
    }

    pub(super) fn link_source(&self) -> Result<scenograph::link::LinkSource, String> {
        match self.never {}
    }

    pub(super) fn saved_file(&self) -> Option<serde_json::Value> {
        match self.never {}
    }

    pub(super) fn select(&mut self, _selected: Option<Uuid>) {
        match self.never {}
    }

    pub(super) fn open_detail(&mut self, _canvas: &Canvas) {
        match self.never {}
    }

    pub(super) fn save(&mut self) {
        match self.never {}
    }

    pub(super) fn poll(&mut self, _canvas: &mut Canvas) -> bool {
        match self.never {}
    }
}

pub(super) fn controls(_page: &TreePage) -> Child {
    Box::new(cambium::el("div", ()))
}

pub(super) fn find_section(page: &TreePage) -> Child {
    controls(page)
}
pub(super) fn scenes_section(page: &TreePage) -> Child {
    controls(page)
}

pub(super) fn projection_section(page: &TreePage) -> Child {
    controls(page)
}
pub(super) fn projection_editor(_page: &TreePage) -> Option<Child> {
    None
}

pub(super) fn history_section(page: &TreePage) -> Child {
    controls(page)
}

pub(super) fn menu(_: &TreePage) -> Option<Child> {
    None
}
impl SavedProduct {
    pub(super) fn close_menu(&mut self) {}
    pub(super) fn menu_open(&self) -> bool {
        false
    }
    pub(super) fn open_menu(&mut self, _: mere::canvas::ContextRequest, _: &mut Canvas) {}
}

impl SavedProduct {
    pub(super) fn run_command(
        &mut self,
        _: &str,
        _: &mut Canvas,
        _: &str,
        _: (u32, u32),
    ) -> Result<(), String> {
        match self.never {}
    }
}
