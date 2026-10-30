// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn take_window_capture(
    window: u64,
    lens_target: Option<u64>,
    primary: &mut Option<std::path::PathBuf>,
    lens: &mut Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    if window == 0 {
        primary.take()
    } else if lens_target == Some(window) {
        lens.take()
    } else {
        None
    }
}
