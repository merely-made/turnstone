// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A deterministic per-(participant, capability) nonce, so re-issuing the same
/// grant yields the same certificate id rather than an ever-growing pile.
pub(super) fn cert_nonce(subject_hex: &str, cap: &Cap, generation: u64) -> [u8; 32] {
    *blake3::hash(format!("{subject_hex}/{generation}/{}", cap.to_wire()).as_bytes()).as_bytes()
}
