// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Split a place-held address into its holder and the path it names.
///
/// The grammar is the shared release address (`gemot-address`); a place-held
/// address is its log-key form, where the key is a Personae root. An edition
/// pin is dropped: the holder serves its current path. `None` for every other
/// Knot address, including `knot://vault/...` and Moot-anchored hosts: a host
/// that is not a 64-hex key is not a holder root and must not be guessed at.
pub fn parse_place_held_address(address: &str) -> Option<([u8; 32], String)> {
    let parsed = gemot_address::KnotAddress::parse(address).ok()?;
    Some((*parsed.log_key()?, parsed.path))
}
