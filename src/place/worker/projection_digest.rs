// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// blake3 over a domain tag and one canonical projection, hex.
pub(super) fn projection_digest<T: serde::Serialize>(domain: &[u8], value: &T) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(&serde_json::to_vec(value).unwrap_or_default());
    hasher.finalize().to_hex().to_string()
}
