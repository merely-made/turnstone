// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn parse_lifecycle(s: &str) -> Result<Lifecycle, String> {
    match s {
        "Active" => Ok(Lifecycle::Active),
        "Paused" => Ok(Lifecycle::Paused),
        "Revoked" => Ok(Lifecycle::Revoked),
        _ => Err("invalid stored lifecycle".into()),
    }
}
