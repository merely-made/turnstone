// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The actual primary UA admission gate, after chrome has offered the press.
/// Retire primary content custody before clearing its shell mirror.
pub(in super::super) fn reject_primary_modal_press(host: &mut impl PrimaryPointerHost) -> bool {
    if !host.pointer_user_agent_decision_open() {
        return false;
    }
    let foreign_capture = host.pointer_has_foreign_capture();
    host.pointer_cancel_primary_content();
    if !foreign_capture {
        *host.pointer_state().2 = None;
    }
    true
}
