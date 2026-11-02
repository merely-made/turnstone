// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn requested_a11y_focus_policy() -> Result<A11yFocusPolicy, SurfaceError> {
    match std::env::var("TURNSTONE_SERVO_A11Y_FOCUS") {
        Ok(value) => parse_a11y_focus_policy(Some(&value)),
        Err(std::env::VarError::NotPresent) => parse_a11y_focus_policy(None),
        Err(_) => Err(spawn_error(
            "TURNSTONE_SERVO_A11Y_FOCUS is not valid Unicode",
        )),
    }
}
