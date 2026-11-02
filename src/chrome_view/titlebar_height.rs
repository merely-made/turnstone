// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(crate) fn titlebar_height(zoom: f32, insets: cambium_rootstock::TitlebarInsets) -> f32 {
    cambium::ShellMetrics::toolbar_height(zoom).max(insets.height)
}
