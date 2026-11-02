// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(in super::super) fn dispatch_lens_pane_release(
    host: &mut impl LensPanePointerHost,
    window: u64,
    position: (f32, f32),
    button: MouseButton,
) -> bool {
    // Existing correct terminal rule: every other button is swallowed, only
    // the host's same-window admitted Left can release this receiver.
    if host.lens_pane_admission().is_none() {
        return false;
    }
    if button == MouseButton::Left {
        host.lens_release_admission(window, position.0, position.1);
    }
    true
}
