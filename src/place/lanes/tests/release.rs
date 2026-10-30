// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn release(worker: &armillary::ActorHandle<PlaceWorkerCommand>) {
    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
    worker.command(PlaceWorkerCommand::Release(ack_tx));
    let _ = ack_rx.recv_timeout(Duration::from_secs(30));
}
