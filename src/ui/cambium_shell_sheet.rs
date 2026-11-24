// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Shared graph hit layers, with the native pane's palette layered over them.
pub(crate) fn cambium_pane_sheet() -> &'static str {
    static SHEET: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        format!(
            "{}\n{}\n{CAMBIUM_SHEET}",
            cambium::GRAPH_CANVAS_SWATCH_CSS,
            cambium::projection_table::PROJECTION_TABLE_CSS,
        )
    });
    &SHEET
}

/// Baseline pane sheet for retained standalone probes as well as live shared controls.
pub(crate) fn cambium_shell_sheet() -> &'static str {
    static SHEET: std::sync::LazyLock<String> =
        std::sync::LazyLock::new(|| format!("{}\n{}", cambium_pane_sheet(), cambium::SHELL_CSS));
    &SHEET
}
