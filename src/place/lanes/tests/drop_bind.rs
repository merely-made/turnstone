// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn drop_bind(open: crate::place::worker::OpenPlace, mut lanes: super::super::LiveLanes) {
    lanes.leave_and_wait();
    drop(lanes);
    drop(open);
}
