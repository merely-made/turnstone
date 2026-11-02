// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn focus_host_input(app: &mut App, target: accesskit::NodeId) -> bool {
    if host_input_focus(app) != Some(target) {
        return false;
    }
    app.focus = crate::surface::FocusTarget::Chrome;
    true
}
