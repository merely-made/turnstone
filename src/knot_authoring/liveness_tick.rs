// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// How often the liveness watch checks the budget. It is a separate thread
/// because the one it watches is blocked inside the carrier read.
pub(super) const LIVENESS_TICK: Duration = Duration::from_millis(250);
