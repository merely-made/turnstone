// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Drain the founder's update stream until nothing has arrived for
/// `quiet`, so a later nudge can only be the one this test provoked.
pub(super) fn settle(founder: &ServedFounder, quiet: Duration) -> usize {
    let mut seen = 0usize;
    while founder.updates.recv_timeout(quiet).is_ok() {
        seen += 1;
    }
    seen
}
