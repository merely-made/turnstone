// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn joined_snapshot(
    updates: &std::sync::mpsc::Receiver<Update>,
) -> crate::place::OfflinePlaceSnapshot {
    match updates.recv_timeout(Duration::from_secs(60)) {
        Ok(Update::PlaceJoined {
            result: Ok((_, snapshot)),
            ..
        }) => snapshot,
        Ok(Update::PlaceJoined {
            result: Err(error), ..
        }) => panic!("join refused: {error}"),
        _ => panic!("join answered with an unrelated update"),
    }
}
