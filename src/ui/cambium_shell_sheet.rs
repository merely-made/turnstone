// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Baseline pane sheet for retained standalone probes as well as live shared controls.
pub(crate) fn cambium_shell_sheet() -> &'static str {
    static SHEET: std::sync::LazyLock<String> =
        std::sync::LazyLock::new(|| format!("{CAMBIUM_SHEET}\n{}", cambium::SHELL_CSS));
    &SHEET
}
