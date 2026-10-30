// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn keyboard_completion(result: Result<(), String>, json: &str) -> Result<(), String> {
    result.map_err(|error| format!("Scry keyboard CDP completion failed: {error}"))?;
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|error| format!("Scry keyboard CDP completion was invalid JSON: {error}"))?;
    if let Some(error) = value.get("error") {
        return Err(format!("Scry keyboard CDP rejected input: {error}"));
    }
    Ok(())
}
