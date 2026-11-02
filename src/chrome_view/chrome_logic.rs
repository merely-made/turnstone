// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The per-projection logic: one closure definition (so every projection
/// shares one `Logic` type), instantiated with its window slot.
pub(super) fn chrome_logic(slot: usize) -> impl FnMut(&ChromeState) -> ChromeView {
    move |state| window_chrome_view(state, slot)
}
