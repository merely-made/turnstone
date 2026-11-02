// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// 32 unpredictable bytes for one place identifier.
pub(super) fn random_id() -> Result<[u8; 32], String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|error| format!("draw place identifier: {error}"))?;
    Ok(bytes)
}
