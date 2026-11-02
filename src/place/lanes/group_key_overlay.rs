// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The group-key lane's sync overlay, from stickleback's exported topic. A
/// dialer must tag it or the lane never forms.
pub(super) fn group_key_overlay(moot: [u8; 32]) -> [u8; 32] {
    sync_overlay_topic(stickleback::group_key_sync_topic(
        stickleback::GroupSessionId(moot),
    ))
}
