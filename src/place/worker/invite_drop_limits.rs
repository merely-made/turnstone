// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Bounds for a peer-supplied Gemot drop. An invitation arrives from someone
/// who is not yet trusted, so the reader is bounded before it allocates.
pub(super) fn invite_drop_limits() -> DropLimits {
    DropLimits::default()
}
