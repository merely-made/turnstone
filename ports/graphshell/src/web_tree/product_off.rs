// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The saved-graph workflow with the `product` feature off (the viewer cone,
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

pub(super) async fn open() -> Result<Option<SavedProduct>, String> {
    Ok(None)
}

pub(super) fn focused_text(
    _runner: &cambium_rootstock::Runner<TreePage, Logic, Child>,
) -> Option<cambium_rootstock::FocusedTextSlot<TreePage>> {
    None
}

impl SavedProduct {
    pub(super) fn ready(&self) -> bool {
        match self.never {}
    }

    pub(super) fn graph(&self) -> Graph {
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
