// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Where the omnibar caret roughly sits on screen, as `(position, size)` in
/// physical px — for the host to aim the IME candidate window
/// (`Window::set_ime_cursor_area`). Average-advance approximation; the IME
/// popup only needs the neighborhood, not the glyph-exact x.
pub fn ime_cursor_area(state: &OmnibarState, w: u32) -> ((f32, f32), (f32, f32)) {
    let left = ((w as f32 - CARD_W) / 2.0).max(8.0);
    ime_cursor_area_at(state, left, CARD_TOP)
}
