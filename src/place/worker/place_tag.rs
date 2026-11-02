// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Domain-separated 32 bytes for one place-scoped derivation.
pub(crate) fn place_tag(domain: &[u8], moot: [u8; 32]) -> [u8; 32] {
    let mut input = Vec::with_capacity(domain.len() + 32);
    input.extend_from_slice(domain);
    input.extend_from_slice(&moot);
    *blake3::hash(&input).as_bytes()
}
