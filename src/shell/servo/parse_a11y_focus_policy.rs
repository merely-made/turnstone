// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn parse_a11y_focus_policy(
    value: Option<&str>,
) -> Result<A11yFocusPolicy, SurfaceError> {
    match value {
        None | Some("document-root") => Ok(A11yFocusPolicy::DocumentRoot),
        Some("upstream") => Ok(A11yFocusPolicy::Upstream),
        Some(_) => Err(spawn_error(
            "TURNSTONE_SERVO_A11Y_FOCUS must be upstream or document-root",
        )),
    }
}
