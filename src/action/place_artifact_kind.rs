// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Which place artifact a read is expected to hold. The shell parses the
/// bytes into exactly this shape and refuses anything else; a mislabelled
/// file is a refusal, never a guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceArtifactKind {
    /// A place card, read to offer a pre-key against its Moot.
    Card,
    /// A local contact refresh, bound to the session that asked for it.
    Contact {
        session: crate::panes::SessionId,
        generation: u64,
    },
    /// A published pre-key offer, read to author an invitation for it. The
    /// access it will admit at rides along: the prompt that asked for the
    /// path is what decided it, not the file.
    Prekey {
        access: crate::place::PlaceInviteAccess,
        /// A writer's grant lifetime, or `None` for grants without expiry.
        lifetime_ms: Option<u64>,
    },
    /// An invitation envelope, read to join through it.
    Invite,
}
