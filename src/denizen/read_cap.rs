// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The capability covering the app's READ face (the observe tier). Not an
/// emission ring, since nothing is dispatched by reading, so every resident
/// gets it and the rings are what the review actually asks for.
pub fn read_cap() -> Cap {
    Cap::Power("read".to_string())
}
