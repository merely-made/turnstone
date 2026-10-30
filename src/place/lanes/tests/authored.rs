// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One authored command's answer, stepping past lane nudges.
pub(super) fn authored(
    updates: &std::sync::mpsc::Receiver<Update>,
    request: u64,
) -> Result<crate::place::OfflinePlaceSnapshot, String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Ok(Update::PlaceCommandDone {
            request: got,
            result,
            ..
        }) = updates.recv_timeout(Duration::from_secs(10))
            && got == request
        {
            return result;
        }
    }
    panic!("request {request} never answered");
}
