// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn validate_action_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 160 {
        Err("resident run action name is invalid or exceeds its bound".into())
    } else {
        Ok(())
    }
}
