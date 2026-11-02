// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Read one place artifact and lower what it holds back into an action.
///
/// Bounded and parsed to exactly the shape the caller asked for. A file that
/// is missing, oversize, or of another kind is a refusal with the reason, not
/// a guess and never a panic.
pub(super) fn read_place_artifact(
    path: &str,
    kind: crate::action::PlaceArtifactKind,
) -> Result<Action, String> {
    use crate::action::PlaceArtifactKind;

    /// The invitation bound already covers the largest artifact here.
    const MAX_BYTES: u64 = 4 * 1024 * 1024;

    let metadata = std::fs::metadata(path).map_err(|error| format!("open {path}: {error}"))?;
    if metadata.len() > MAX_BYTES {
        return Err(format!("{path} is larger than {MAX_BYTES} bytes"));
    }
    let bytes = std::fs::read(path).map_err(|error| format!("read {path}: {error}"))?;
    match kind {
        PlaceArtifactKind::Card => {
            let card: crate::place::PlaceCardV1 = serde_json::from_slice(&bytes)
                .map_err(|error| format!("{path} is not a place card: {error}"))?;
            card.validate()?;
            Ok(Action::OfferPlacePrekeyForCard {
                card: Box::new(card),
                out: format!("{path}.prekey.json"),
            })
        },
        PlaceArtifactKind::Contact {
            session,
            generation,
        } => {
            let card: crate::place::PlaceCardV1 = serde_json::from_slice(&bytes)
                .map_err(|error| format!("{path} is not a place card: {error}"))?;
            card.validate()?;
            Ok(Action::ReconnectPlaceWithCard {
                session,
                generation,
                card: Box::new(card),
            })
        },
        PlaceArtifactKind::Prekey {
            access,
            lifetime_ms,
        } => {
            let offer: crate::place::PlacePrekeyOfferV1 = serde_json::from_slice(&bytes)
                .map_err(|error| format!("{path} is not a pre-key offer: {error}"))?;
            Ok(Action::InviteToPlaceWithPrekey {
                prekey: offer.prekey_bytes()?,
                out: format!("{path}.invite.json"),
                access,
                lifetime_ms,
            })
        },
        PlaceArtifactKind::Invite => {
            let invite: crate::place::invite::PlaceInviteV1 = serde_json::from_slice(&bytes)
                .map_err(|error| format!("{path} is not an invitation: {error}"))?;
            invite
                .validate()
                .map_err(|error| format!("{path}: {error}"))?;
            Ok(Action::JoinPlace(Box::new(invite)))
        },
    }
}
