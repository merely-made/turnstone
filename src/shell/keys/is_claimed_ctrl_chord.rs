// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Whether a key is a chord this module claims before the focused surface sees
/// it: address/command summons, reload, find, and page zoom.
pub(super) fn is_claimed_ctrl_chord(key: &WinitKey) -> bool {
    matches!(key, WinitKey::Character(value)
        if ["f", "l", "k", "r"].iter().any(|claimed| value.eq_ignore_ascii_case(claimed))
            || PageZoomChord::for_character(value).is_some())
}
