// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn share_node(
    founder: &(
        armillary::ActorHandle<PlaceWorkerCommand>,
        std::sync::mpsc::Receiver<Update>,
    ),
    session: SessionId,
    request: u64,
    address: &str,
) {
    founder.0.command(PlaceWorkerCommand::Author {
        session,
        generation: 1,
        request,
        command: PlaceCommand::ShareNode {
            address: address.into(),
        },
    });
    authored(&founder.1, request).unwrap_or_else(|error| panic!("share {address}: {error}"));
}
