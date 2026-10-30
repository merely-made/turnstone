// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(in super::super) fn dispatch_primary_context_press(
    host: &mut impl PrimaryPressAdmissionHost,
    button: MouseButton,
    hit: Option<crate::surface::HitResult>,
) -> bool {
    if button != MouseButton::Right
        || hit.is_some_and(|hit| matches!(hit.kind, crate::surface::SurfaceKind::Graph(_)))
    {
        return false;
    }
    dispatch_primary_action_press(host, Action::OmnibarOpen { command: true });
    true
}
