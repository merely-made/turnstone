// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

// Owner publication and helper acknowledgement are separate durability domains.
// Keep their actual ordering in one seam so an uncertain journal reply cannot
// hide a destination graph that has already been recovered from storage.
pub(super) fn install_destination_then_acknowledge<T, E>(
    install: impl FnOnce(),
    acknowledge: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    install();
    acknowledge()
}
