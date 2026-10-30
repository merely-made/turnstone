// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// A window's layout scale as Turnstone composes at it: finite and within the
/// range the native hosts report.
pub(crate) fn clean_scale(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.clamp(0.5, 4.0)
    } else {
        1.0
    }
}
