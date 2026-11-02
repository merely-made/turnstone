// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Which place question the omnibar is asking. One variant per palette row
/// in the founding vocabulary, so a commit cannot land in the wrong action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacePrompt {
    FoundPlace,
    ExportCard,
    RefreshContact,
    OfferPrekey,
    Invite,
    /// The same question as `Invite`, answered with a reader's admission.
    InviteReader,
    Join,
    SendMessage,
}
