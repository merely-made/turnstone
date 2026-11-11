// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Opening a node whose address names a correspondent.
//!
//! A misfin address (`misfin://mailbox@host`) names a correspondent, not a
//! document, so a host that opens such a node shows the Comms conversation with
//! them instead of a tile (ruled 2026-10-26, GUI catalog record §9).

pub use comms::CommsPane;
use comms::misfin_correspondent;

/// What opening a node's address did to the Comms pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommsOpen {
    /// The address names no correspondent; the node opens as a tile.
    Tile,
    /// The pane is open on the conversation with the correspondent.
    Conversation,
    /// The pane is open, but no conversation with this correspondent is loaded.
    NoConversation(String),
}

/// Open Comms to the conversation `address` names, when it names a correspondent.
/// Opens the pane if it is closed; selects the loaded conversation with them.
pub fn open_node(pane: &mut CommsPane, address: &str) -> CommsOpen {
    let Some(correspondent) = misfin_correspondent(address) else {
        return CommsOpen::Tile;
    };
    pane.open();
    match pane.misfin_conversation(correspondent) {
        Some(id) => {
            pane.select(id);
            CommsOpen::Conversation
        },
        None => CommsOpen::NoConversation(correspondent.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use comms::{Conversation, ConversationId, Inbox, ProtocolKind};

    fn conversation(id: ConversationId) -> Conversation {
        Conversation {
            title: id.key.clone(),
            id,
            participants: Vec::new(),
            last_activity_ms: Some(1),
            unread: 0,
        }
    }

    fn pane_with(ids: &[ConversationId]) -> CommsPane {
        let mut pane = CommsPane::new();
        pane.set_inbox(Inbox {
            conversations: ids.iter().cloned().map(conversation).collect(),
            failures: Vec::new(),
        });
        pane
    }

    #[test]
    fn a_misfin_node_opens_the_closed_pane_on_its_conversation() {
        let ana = ConversationId::for_account(ProtocolKind::Misfin, "primary", "ana@example.test");
        let bob = ConversationId::for_account(ProtocolKind::Misfin, "primary", "bob@example.test");
        let mut pane = pane_with(&[bob.clone(), ana.clone()]);
        assert!(!pane.is_open());

        assert_eq!(
            open_node(&mut pane, "misfin://ana@example.test"),
            CommsOpen::Conversation
        );
        assert!(pane.is_open() && pane.dock.focused);
        assert_eq!(pane.selected(), Some(&ana));

        // An open pane follows the next misfin node.
        assert_eq!(
            open_node(&mut pane, "misfin://bob@example.test"),
            CommsOpen::Conversation
        );
        assert_eq!(pane.selected(), Some(&bob));
    }

    #[test]
    fn a_correspondent_with_no_loaded_conversation_opens_the_pane_alone() {
        let ana = ConversationId::new(ProtocolKind::Misfin, "ana@example.test");
        let mut pane = pane_with(&[ana.clone()]);
        pane.select(ana.clone());

        assert_eq!(
            open_node(&mut pane, "misfin://zed@example.test"),
            CommsOpen::NoConversation("zed@example.test".into())
        );
        assert!(pane.is_open());
        assert_eq!(pane.selected(), Some(&ana), "the prior selection stays");
    }

    #[test]
    fn any_other_address_leaves_the_pane_untouched_and_opens_a_tile() {
        let ana = ConversationId::new(ProtocolKind::Misfin, "ana@example.test");
        let mut pane = pane_with(&[ana]);
        for address in [
            "https://example.test/ana@example.test",
            "gemini://ana@example.test",
            "misfin://example.test",
        ] {
            assert_eq!(open_node(&mut pane, address), CommsOpen::Tile, "{address}");
        }
        assert!(!pane.is_open());
        assert!(pane.selected().is_none());
    }
}
