// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! The Comms pane a node naming a correspondent opens instead of a tile.
use super::*;
use graphshell::comms_open::{CommsOpen, CommsPane, open_node};
use uuid::Uuid;

#[derive(Default)]
pub(super) struct Pane {
    view: CommsPane,
    /// Why the pane shows no conversation, when the last open found none.
    notice: Option<String>,
}

/// Open Comms rather than a tile when `member`'s address names a correspondent.
/// Returns whether it did, so the caller skips the tile.
pub(super) fn open_correspondent(page: &mut TreePage, member: Uuid) -> bool {
    let Some(address) = page
        .shared
        .canvas
        .borrow()
        .graph()
        .get_node_by_id(member)
        .map(|(_, node)| node.url().to_owned())
    else {
        return false;
    };
    let notice = match open_node(&mut page.comms.view, &address) {
        CommsOpen::Tile => return false,
        CommsOpen::Conversation => None,
        CommsOpen::NoConversation(who) => Some(format!("No conversation with {who} is loaded")),
    };
    page.comms.notice = notice;
    page.shared.dirty.set(true);
    true
}

pub(super) fn section(page: &TreePage) -> Child {
    let pane = &page.comms;
    if !pane.view.is_open() {
        return Box::new(el("div", ()));
    }
    let line = match (pane.view.selected_conversation(), &pane.notice) {
        (Some(conversation), _) => format!("Conversation with {}", conversation.title),
        (None, Some(notice)) => notice.clone(),
        (None, None) => "No conversation selected".to_owned(),
    };
    Box::new(
        el("section", (el("h2", "Comms"), el("p", line)))
            .attr("role", "region")
            .attr("aria-label", "Comms"),
    )
}
