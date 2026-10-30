// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Whether the chrome layer has anything to show (the shell skips the
/// second rasterize pass entirely when it does not).
pub fn chrome_has_content(state: &OmnibarState, caption: Option<&str>) -> bool {
    state.open || caption.is_some()
}
