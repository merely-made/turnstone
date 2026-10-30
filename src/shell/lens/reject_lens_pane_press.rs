// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(in super::super) fn reject_lens_pane_press(host: &mut impl LensPanePointerHost) -> bool {
    let Some(admitted) = host.lens_pane_admission() else {
        return false;
    };
    // One physical mouse gesture cannot admit a competing page or pane Down,
    // including a new press arriving in another native window.
    if !host.lens_pane_admission_current(admitted) {
        // Existing stale-withdrawal policy is distinct from valid custody.
        host.lens_cancel_stale_admission();
    }
    true
}
