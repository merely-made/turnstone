// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PlaceState {
    /// Shared by the status surface and observation. Facts are from the last
    /// worker snapshot; opening this view requests a fresh local snapshot.
    pub fn status_lines(&self) -> Vec<String> {
        let mut rows = match self {
            Self::Personal => vec!["Personal session: no shared place joined".into()],
            Self::Joining { .. } => vec!["Place: checking invitation".into()],
            Self::Opening { .. } => vec!["Place: opening retained state or reconnecting".into()],
            Self::Degraded { error, .. } => vec![format!("Place unavailable: {error}")],
            Self::Failed { error } => vec![format!("Place not joined: {error}")],
            Self::Left { retained, .. } => vec![format!(
                "Place left: history retained; {} shared nodes, {} messages",
                retained.graph_nodes, retained.chat_messages
            )],
            Self::Offline { snapshot, .. } => {
                let mut rows = vec![match &snapshot.sync {
                    None => "Last refresh: retained data only; no local sync lanes opened".into(),
                    Some(_) => {
                        "Last refresh: local sync lanes opened; peer reachability unknown".into()
                    },
                }];
                rows.push(format!(
                    "Retained history: {} shared nodes, {} messages",
                    snapshot.graph.nodes, snapshot.chat.messages
                ));
                rows.push(format!(
                    "Group key frames: {} pending; {} refused",
                    snapshot.group.pending_frames, snapshot.group.refused_frames
                ));
                // Parked records are held, not lost: they are offered again
                // after every key refresh. Eviction is the one loss, so it
                // says so on the same row rather than only in a log.
                rows.push(format!(
                    "Awaiting keys: {} parked; {} re-admitted; {} evicted",
                    snapshot.group.parked_records,
                    snapshot.group.readmitted_records,
                    snapshot.group.evicted_records
                ));
                match &snapshot.permissions {
                    Some(permissions) => {
                        let permission = |effective| {
                            if effective {
                                "effective"
                            } else {
                                "not effective"
                            }
                        };
                        rows.push(format!(
                            "Message permission: {} locally at last refresh",
                            permission(permissions.message_write)
                        ));
                        rows.push(format!(
                            "Shared graph permission: {} locally at last refresh",
                            permission(permissions.graph_write)
                        ));
                    },
                    None => rows.push("Writing permissions: not evaluated".into()),
                }
                rows.extend(snapshot.standing.status_line());
                rows.push("Writing: permissions are checked again for each action".into());
                rows.push(
                    "Delivery: these sync observations do not confirm message delivery".into(),
                );
                if let Some(sync) = &snapshot.sync {
                    // Listen-only is a real state and must never read as
                    // connected: a founder alone in a place has lanes open and
                    // nobody on them.
                    if sync.dialed_rendezvous == 0 {
                        rows.push("Listening for peers: none dialed".into());
                    }
                    rows.extend(
                        sync.local_rendezvous
                            .iter()
                            // The full ticket stays in the snapshot, the
                            // observation, and the exported card; only this
                            // one-line row is elided.
                            .map(|ticket| format!("Local rendezvous: {}", elide_middle(ticket))),
                    );
                    // "at last refresh" is dropped here: the header row above
                    // already states every fact on this card is from the last
                    // refresh, so repeating it per lane only pushed the row
                    // past the card's text budget. (Mark, 2026-09-13)
                    rows.push(match &sync.projection {
                        Some(projection) => format!(
                            "Projection: serving {} to {} live session{}; {} refused",
                            projection.route,
                            projection.live_sessions,
                            if projection.live_sessions == 1 {
                                ""
                            } else {
                                "s"
                            },
                            projection.refused
                        ),
                        None => "Projection: not serving (no Knot vault)".into(),
                    });
                    rows.extend(sync.lanes.iter().map(|lane| {
                        format!(
                            "{}: {}; {} rounds; {} accepted ops",
                            lane.label(),
                            if lane.syncing { "syncing" } else { "idle" },
                            lane.sync_rounds,
                            lane.ops_received
                        )
                    }));
                }
                rows
            },
        };
        if self.binding().is_some() {
            rows.push("Reconnect: use Refresh place contact if saved hints are stale".into());
            rows.push("Leave: detach this session and retain its history".into());
        }
        if matches!(self, Self::Left { .. }) {
            rows.push("Rejoin: reconnect with the retained history".into());
        }
        rows
    }

    /// This bind's own dialable tickets at the last refresh, whole. Empty
    /// unless a place is open and its lanes reported one.
    pub fn local_rendezvous(&self) -> &[String] {
        match self {
            Self::Offline { snapshot, .. } => snapshot
                .sync
                .as_ref()
                .map(|sync| sync.local_rendezvous.as_slice())
                .unwrap_or_default(),
            _ => &[],
        }
    }

    /// `None` for every state including `Left`: every "not joined" guard
    /// (found, join, offer a pre-key) reads this the same way for a left
    /// session as for a personal one. `Rejoin` alone needs the binding a
    /// left session still remembers, so it reads [`Self::left_binding`]
    /// instead.
    pub fn binding(&self) -> Option<&PlaceBindingV1> {
        match self {
            Self::Opening { binding, .. }
            | Self::Offline { binding, .. }
            | Self::Degraded { binding, .. } => Some(binding),
            Self::Personal | Self::Joining { .. } | Self::Failed { .. } | Self::Left { .. } => None,
        }
    }

    /// The binding a locally left session still remembers, for `Rejoin`
    /// alone. Every other guard uses [`Self::binding`], which reads `Left`
    /// as not joined.
    pub fn left_binding(&self) -> Option<&PlaceBindingV1> {
        match self {
            Self::Left { binding, .. } => Some(binding),
            _ => None,
        }
    }

    pub fn generation(&self) -> Option<u64> {
        match self {
            Self::Joining { generation }
            | Self::Opening { generation, .. }
            | Self::Offline { generation, .. }
            | Self::Degraded { generation, .. } => Some(*generation),
            Self::Personal | Self::Failed { .. } | Self::Left { .. } => None,
        }
    }
}
