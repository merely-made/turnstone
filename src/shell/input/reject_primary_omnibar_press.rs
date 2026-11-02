// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(in super::super) fn reject_primary_omnibar_press(
    host: &mut impl PrimaryPressAdmissionHost,
) -> bool {
    if !host.pointer_omnibar_open() {
        return false;
    }
    dispatch_primary_action_press(host, Action::OmnibarClose);
    true
}
