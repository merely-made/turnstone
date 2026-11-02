// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A worker's stores, reopened by hand once its worker has let go.
pub(super) fn reopen_by_hand(
    directory: &Path,
    binding: &PlaceBindingV1,
    identity: &InMemoryProvider,
) -> crate::place::worker::OpenPlace {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match open_cached_place(directory, binding, identity, &settings()) {
            Ok((open, _)) => return open,
            Err(error) if error.contains("already open") && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            },
            Err(error) => panic!("reopen {}: {error}", directory.display()),
        }
    }
}
