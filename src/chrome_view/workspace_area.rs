// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn workspace_area(
    chrome: &crate::shell_services::ShellChromeConfig,
    primary: bool,
    width: u32,
    height: u32,
) -> crate::surface::Rect {
    workspace_area_for_window(chrome, if primary { 0 } else { 1 }, width, height)
}
