// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Every place test builds its worker settings here, so this is where
/// the suite claims room for its sockets and stores: run in parallel, it
/// holds more than the 1024 open files Linux gives a process by default.
pub(crate) fn settings() -> PlaceWorkerSettings {
    open_file_room();
    PlaceWorkerSettings {
        authority_clock: AuthorityClock::Fixed(AUTHORITY_AT_MS),
        ..PlaceWorkerSettings::default()
    }
}
