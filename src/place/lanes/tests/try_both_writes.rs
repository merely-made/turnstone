// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Share one node and send one message; both answers are returned.
pub(super) fn try_both_writes(
    worker: &armillary::ActorHandle<PlaceWorkerCommand>,
    updates: &std::sync::mpsc::Receiver<Update>,
    session: SessionId,
    channel: &str,
    first_request: u64,
) -> [Result<crate::place::OfflinePlaceSnapshot, String>; 2] {
    let commands = [
        PlaceCommand::ShareNode {
            address: "https://refused.example/page".into(),
        },
        PlaceCommand::SendMessage {
            channel: channel.into(),
            body: "refused".into(),
        },
    ];
    let mut request = first_request;
    commands.map(|command| {
        worker.command(PlaceWorkerCommand::Author {
            session,
            generation: 1,
            request,
            command,
        });
        let answer = authored(updates, request);
        request += 1;
        answer
    })
}
