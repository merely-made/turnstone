// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Split a place-held address into its holder and the path it names.
///
/// `None` for every other Knot address, including `knot://vault/...`: a host
/// that is not 64 hex characters is not a Personae root and must not be
/// guessed at.
pub fn parse_place_held_address(address: &str) -> Option<([u8; 32], String)> {
    let rest = address
        .get(..7)
        .filter(|scheme| scheme.eq_ignore_ascii_case("knot://"))
        .and_then(|_| address.get(7..))?;
    let (host, path) = rest.split_once('/')?;
    if host.len() != 64 || !host.bytes().all(|byte| byte.is_ascii_hexdigit()) || path.is_empty() {
        return None;
    }
    let mut root = [0u8; 32];
    for (slot, pair) in root.iter_mut().zip(host.as_bytes().chunks(2)) {
        *slot = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some((root, path.to_string()))
}
