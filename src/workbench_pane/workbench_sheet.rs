// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn workbench_sheet() -> &'static str {
    static SHEET: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        format!(
            "{}\n{}\n.wb-single-reading .read-header {{ cursor:grab; }}",
            crate::ui::CAMBIUM_SHEET,
            cambium::SHELL_CSS
        )
    });
    &SHEET
}
