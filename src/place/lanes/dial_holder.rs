// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Dial a place member who holds a document, over this place's own transport.
///
/// The visiting half's one entry point. The returned carrier is what
/// `RetainedEndpointSession::over` mounts; the peer id is returned with it so
/// a caller can name the holder it reached without re-importing the ticket.
///
/// Two things happen here that are not obvious from the call. The ticket is
/// imported first, because a ticket is an address and the address book is
/// what `connect` reads. And the stored grant is not the leaf: the handshake
/// binds the claimed subject to the peer the carrier authenticated, and this
/// place's transport key is derived per place, so the grant's one remaining
/// delegation hop is spent here on a leaf from the member's Personae root to
/// its own place-transport key. That key is then what the hello claims.
///
/// The carrier is not `Send` by mere's deliberate choice, so it belongs to
/// the thread that opened it.
pub(crate) fn dial_holder(
    lanes: &LiveLanes,
    holder_ticket: &str,
    grant: &[u8],
    identity: &dyn IdentityProvider,
    nonce: [u8; 32],
) -> Result<(PlaceProjectionCarrier, PeerID), String> {
    holder_dial(lanes, holder_ticket, grant, identity, nonce)?.open()
}
