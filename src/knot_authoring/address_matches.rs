// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Whether a presentation the endpoint disclosed answers the requested
/// address.
///
/// A place-held address is the one case where the two strings differ on
/// purpose: the holder discloses its own local address, which a visitor has
/// no way to know and no business knowing. The path inside the vault is the
/// part both ends agree on, so that is what is compared. Which hub the
/// request reached has already decided whose vault this is.
pub(super) fn address_matches(requested: &str, disclosed: &str) -> bool {
    if requested == disclosed {
        return true;
    }
    let Some((_, path)) = parse_place_held_address(requested) else {
        return false;
    };
    let tail = format!("/{path}");
    disclosed.len() > tail.len() && disclosed.ends_with(&tail)
}
