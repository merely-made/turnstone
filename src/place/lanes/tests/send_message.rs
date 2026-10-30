// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn send_message(
    worker: &armillary::ActorHandle<PlaceWorkerCommand>,
    updates: &std::sync::mpsc::Receiver<Update>,
    session: SessionId,
    request: u64,
    channel: &str,
    body: &str,
) -> crate::place::OfflinePlaceSnapshot {
    worker.command(PlaceWorkerCommand::Author {
        session,
        generation: 1,
        request,
        command: PlaceCommand::SendMessage {
            channel: channel.into(),
            body: body.into(),
        },
    });
    authored(updates, request).unwrap_or_else(|error| panic!("{body}: {error}"))
}
