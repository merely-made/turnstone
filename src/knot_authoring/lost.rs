// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// What a lost endpoint is called, which depends on whose it was.
pub(super) fn lost(visiting: bool, reason: String) -> HubEvent {
    if visiting {
        HubEvent::Unavailable(reason)
    } else {
        HubEvent::Revoked(reason)
    }
}
