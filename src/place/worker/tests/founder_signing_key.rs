// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn founder_signing_key(
    founder: &InMemoryProvider,
    moot: [u8; 32],
) -> identity::Ed25519Keypair {
    super::super::founder_signing_key(founder, moot).unwrap()
}
