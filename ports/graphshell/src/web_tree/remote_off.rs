// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The remote session with the `remote` feature off (the viewer cone, mer3ly
//! Rulings 110 and 119): `remote.rs`'s API, inert. There is no WebRTC link, so
//! no session ever goes live, the board is never shown, and `connect` refuses.
use super::*;
use cambium::{DisclosureState, SelectState};
use graphshell::client::MountedScene;
use graphshell::client::remote::ActionForm;
use graphshell::remote_board::RemoteBoard;
use mere::canvas::{BoardFit, BoardText};
use std::convert::Infallible;

pub(super) const BOARD_FIT: BoardFit = BoardFit {
    left: 24.0,
    right: 24.0,
    top: 24.0,
    bottom: 24.0,
    frame_edges: true,
};

pub(super) struct Sections {
    pub(super) physics: DisclosureState,
    pub(super) remote: DisclosureState,
}

impl Default for Sections {
    fn default() -> Self {
        Self {
            physics: DisclosureState::new("tools-physics", "Arrangement and physics")
                .expanded(true),
            remote: DisclosureState::new("tools-remote", "Remote session").expanded(false),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Session {
    Local,
    Remote,
}

/// A live link, which this build cannot have.
pub(super) struct LiveRemote {
    pub(super) session: LiveSession,
}

pub(super) struct LiveSession(Infallible);

impl LiveSession {
    pub(super) fn last_resume(&self) -> &str {
        match self.0 {}
    }

    pub(super) fn rejoins(&self) -> u64 {
        match self.0 {}
    }
}

pub(super) struct TreeRemote {
    pub(super) live: Option<LiveRemote>,
    form: ActionForm,
    pub(super) events: Vec<String>,
    pub(super) board: RemoteBoard,
    pub(super) text: BoardText,
    pub(super) generation: u64,
}

impl TreeRemote {
    pub(super) fn new() -> Self {
        Self {
            live: None,
            form: ActionForm::new("Ready"),
            events: Vec::new(),
            board: RemoteBoard::new(),
            text: BoardText::new(),
            generation: 0,
        }
    }

    pub(super) fn pump(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    pub(super) fn form(&self) -> &ActionForm {
        &self.form
    }

    pub(super) fn form_mut(&mut self) -> &mut ActionForm {
        &mut self.form
    }

    pub(super) fn status(&self) -> &str {
        "no remote in this build"
    }

    pub(super) fn in_flight(&self) -> bool {
        false
    }

    pub(super) fn mounted(&self) -> Option<&MountedScene> {
        None
    }

    pub(super) fn revision(&self) -> Option<u64> {
        None
    }

    pub(super) fn label(&self) -> String {
        "Remote projection · no remote in this build".to_string()
    }

    pub(super) fn actions(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    pub(super) fn sync_board(
        &mut self,
        choice: mere::canvas::PhysicsChoice,
        speed: mere::canvas::Speed,
    ) {
        self.board.sync(None, None, choice, speed);
    }
}

pub(crate) fn connect(_signal_url: String, _invite: Option<String>) -> Result<(), String> {
    Err("this build has no remote session: the `remote` feature is off".to_string())
}

#[derive(Default)]
pub(super) struct DraftControls {
    pub(super) fields: Vec<SelectState>,
}

pub(super) fn board_semantics(
    _remote: &TreeRemote,
    _width: u32,
    _height: u32,
) -> cambium_rootstock::ProducerSemantics {
    use cambium_rootstock::{ProducerRole, ProducerSemantics};
    ProducerSemantics {
        role: Some(ProducerRole::List),
        name: Some("Remote board · 0 cards".to_string()),
        children: Vec::new(),
    }
}

pub(super) fn active_line(page: &TreePage) -> String {
    format!(
        "Local Mere · {} objects",
        page.shared.canvas.borrow().graph().node_count()
    )
}

pub(super) fn section(_page: &TreePage) -> Child {
    Box::new(cambium::el("div", ()))
}
