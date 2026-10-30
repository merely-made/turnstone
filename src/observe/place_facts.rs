// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The place facts, when this session holds an opened place.
pub fn place_facts(app: &App) -> Option<PlaceFacts> {
    let crate::place::PlaceState::Offline {
        binding, snapshot, ..
    } = &app.place
    else {
        return None;
    };
    Some(PlaceFacts {
        personae_root: crate::place::hex32(&snapshot.personae_root),
        moot: crate::place::hex32(&binding.moot.0),
        root: crate::place::hex32(&binding.root.0),
        chat: crate::place::hex32(&binding.chat.0),
        default_channel: binding.default_channel.clone(),
        local_rendezvous: snapshot
            .sync
            .as_ref()
            .map(|sync| sync.local_rendezvous.clone())
            .unwrap_or_default(),
        members: snapshot.moot.members,
        graph_nodes: snapshot.graph.nodes,
        chat_messages: snapshot.chat.messages,
        group_frames_pending: snapshot.group.pending_frames,
        group_frames_refused: snapshot.group.refused_frames,
        parked_records: snapshot.group.parked_records,
        readmitted_records: snapshot.group.readmitted_records,
        evicted_records: snapshot.group.evicted_records,
        graph_digest: snapshot.graph_digest.clone(),
        chat_digest: snapshot.chat_digest.clone(),
        lanes: snapshot
            .sync
            .as_ref()
            .map(|sync| {
                sync.lanes
                    .iter()
                    .map(|lane| PlaceLaneFacts {
                        name: lane.name.to_string(),
                        syncing: lane.syncing,
                        sync_rounds: lane.sync_rounds,
                        ops_received: lane.ops_received,
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}
