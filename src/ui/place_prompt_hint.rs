// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PlacePrompt {
    /// The one-line hint shown while this prompt is open.
    pub fn hint(self) -> &'static str {
        match self {
            Self::FoundPlace => "Enter a place name and press Enter to found it",
            Self::ExportCard => "Enter a path and press Enter to write the place card",
            Self::RefreshContact => {
                "Enter a current card path to refresh this place contact and reconnect"
            },
            Self::OfferPrekey => "Enter a card path and press Enter to offer a pre-key",
            Self::Invite => "Enter a pre-key path, optionally \" for 10m\" (s|m|h|d), then Enter",
            Self::InviteReader => "Enter a pre-key path and press Enter to invite a reader",
            Self::Join => "Enter an invitation path and press Enter to join",
            Self::SendMessage => "Enter a message and press Enter to send it",
        }
    }
}
