// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn keyring_state(session: &GroupSession) -> Result<Vec<u8>, String> {
    session
        .data_keyring_state()
        .map_err(|error| format!("read group data epochs: {error}"))
}
