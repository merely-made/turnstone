// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The IME candidate location for an omnibar already positioned by the chrome
/// projection.  Keeping this separate from the default wrapper means a docked
/// omnibar does not leave its composition popup at the old overlay location.
pub fn ime_cursor_area_at(state: &OmnibarState, left: f32, top: f32) -> ((f32, f32), (f32, f32)) {
    let chars_before = state.text[..state.cursor].chars().count();
    (
        (left + 16.0 + chars_before as f32 * 8.0, top + 10.0),
        (2.0, 28.0),
    )
}
