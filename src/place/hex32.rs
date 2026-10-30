// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Hex for a 32-byte public identifier, the one spelling the card uses.
pub fn hex32(bytes: &[u8; 32]) -> String {
    encode_hex(bytes)
}
