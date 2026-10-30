// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// `open_cached_place`, retried while redb still reports the lock held.
///
/// Only the Reconnect path needs this: it is the one place that closes a live
/// bind and reopens the same stores inside one process.
pub(super) fn reopen_cached_place(
    directory: &Path,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    settings: &PlaceWorkerSettings,
) -> Result<(OpenPlace, OfflinePlaceSnapshot), String> {
    let deadline = std::time::Instant::now() + RECONNECT_REOPEN_BUDGET;
    loop {
        match open_cached_place(directory, binding, identity, settings) {
            Err(error)
                if error.contains("already open") && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(25));
            },
            outcome => return outcome,
        }
    }
}
