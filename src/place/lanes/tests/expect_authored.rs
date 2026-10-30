// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Wait for one authored command's answer, stepping past the lane nudges
/// that arrive on the same channel. A bare `recv` races the watcher.
pub(super) fn expect_authored(updates: &std::sync::mpsc::Receiver<Update>, request: u64) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        match updates.recv_timeout(Duration::from_secs(10)) {
            Ok(Update::PlaceCommandDone {
                request: got,
                result: Ok(_),
                ..
            }) if got == request => return,
            Ok(Update::PlaceCommandDone {
                result: Err(error), ..
            }) => panic!("request {request} refused: {error}"),
            _ => continue,
        }
    }
    panic!("request {request} never answered");
}
