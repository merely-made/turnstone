// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The Settings pane's live Cambium sheet, in layout pixels. Other pane kinds
/// retain their established host styling until they take the same typed
/// appearance value as an explicit consumer; this pane is the first proof that
/// changing the settings changes the currently-running UI.
pub(crate) fn cambium_sheet(appearance: &crate::shell_services::AppearanceConfig) -> String {
    let zoom = appearance.zoom();
    let (z12, z13) = (12.0 * zoom, 13.0 * zoom);
    let sheet = format!(
        "{CAMBIUM_SHEET} \
        .pane, .grid {{ background-color: var(--tabard-color-bg); color: var(--tabard-color-text); font-size: {z13:.2}px; }} \
        .list-section-title, .setting-label {{ color: var(--tabard-color-text-dim); font-size: {z12:.2}px; }} \
        .list-row, .setting-row {{ color: var(--tabard-color-text); font-size: {z13:.2}px; }} \
        .list-row.muted, .setting-unsupported {{ color: var(--tabard-color-text-dim); }} \
        .setting-apply {{ color: var(--tabard-color-text-header); background-color: var(--tabard-color-surface); border: 1px solid var(--tabard-color-text-dim); font-size: {z13:.2}px; }} \
        .radio {{ color: var(--tabard-color-text); font-size: {z13:.2}px; }} \
        .toggle {{ color: var(--tabard-color-text); font-size: {z13:.2}px; }} \
        .radio.selected {{ color: var(--tabard-color-primary); }} \
        .slider-track {{ background-color: var(--tabard-color-text-dim); }} \
        .slider-thumb {{ background-color: var(--tabard-color-primary); border-color: var(--tabard-color-text-dim); }}"
    );
    themed_sheet(appearance, format!("{sheet}\n{}", cambium::SHELL_CSS))
}
