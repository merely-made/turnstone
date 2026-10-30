// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Shared reserved rows for the retained workspace and the chrome overlay.
/// The control line and its padding grow with the UI zoom.
pub(crate) fn navigation_height(zoom: f32) -> f32 {
    cambium::ShellMetrics::toolbar_height(zoom)
}
