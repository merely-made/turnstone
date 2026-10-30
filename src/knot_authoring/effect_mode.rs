// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn effect_mode(name: &str) -> Result<String, String> {
    let value = std::env::var(name).unwrap_or_else(|_| "never".into());
    match value.as_str() {
        "auto" | "ask" | "never" => Ok(value),
        _ => Err(format!("{name} must be auto, ask, or never")),
    }
}
