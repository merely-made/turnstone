// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Take the next invitation off a worker's update channel, stepping past
/// the lane nudges that share it.
pub(super) fn expect_invited(
    updates: &std::sync::mpsc::Receiver<Update>,
    what: &str,
) -> Box<crate::place::invite::PlaceInviteV1> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        assert!(Instant::now() < deadline, "{what}: never answered");
        match updates.recv_timeout(Duration::from_secs(5)) {
            Ok(Update::PlaceInvited {
                result: Ok(invite), ..
            }) => return invite,
            Ok(Update::PlaceInvited {
                result: Err(error), ..
            }) => panic!("{what}: refused: {error}"),
            _ => continue,
        }
    }
}
