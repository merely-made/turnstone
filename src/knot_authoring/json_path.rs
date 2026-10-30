// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn json_path(value: &serde_json::Value) -> Option<Vec<u32>> {
    value
        .as_array()?
        .iter()
        .map(|component| {
            component
                .as_u64()
                .and_then(|component| u32::try_from(component).ok())
        })
        .collect()
}
