// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Rewrite a locally held Knot address into the place-held form, or `None`
/// when this address names no document inside `vault_root`.
///
/// Called where the sharer's Personae root and its vault root are both known,
/// which is the place worker. An address already in the place-held form is
/// left alone: re-sharing someone else's document must not re-attribute it.
pub fn place_held_rewrite(
    address: &str,
    vault_root: &std::path::Path,
    holder_root: &[u8; 32],
) -> Option<String> {
    if parse_place_held_address(address).is_some() {
        return None;
    }
    if let Some(name) = address
        .get(..13)
        .filter(|prefix| prefix.eq_ignore_ascii_case("knot://vault/"))
        .and(address.get(13..))
        .map(|rest| rest.trim_start_matches('/'))
        .filter(|rest| !rest.is_empty())
    {
        return Some(place_held_address(holder_root, name));
    }
    let path = url::Url::parse(address).ok()?.to_file_path().ok()?;
    let relative = vault_relative(&path, vault_root)?;
    Some(place_held_address(holder_root, &relative))
}
