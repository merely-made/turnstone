// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn dispatch_primary_action_press(
    host: &mut impl PrimaryPressAdmissionHost,
    action: Action,
) {
    // Retire this window's admitted content before an App action can alter
    // focus or replace/drop its receiver. Preserve foreign-window custody.
    let foreign_capture = host.pointer_has_foreign_capture();
    host.pointer_retire_primary_pointers();
    if !foreign_capture {
        *host.pointer_state().2 = None;
    }
    host.pointer_press_action(action);
}
