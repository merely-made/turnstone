// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Print one snapshot's ten lane counters, plus the two folds this
/// diagnostic is about.
pub(super) fn report_lanes(what: &str, snapshot: &crate::place::OfflinePlaceSnapshot) {
    eprintln!(
        "{what}: members {} nodes {} channels {}",
        snapshot.moot.members, snapshot.graph.nodes, snapshot.chat.channels
    );
    let Some(sync) = &snapshot.sync else {
        eprintln!("{what}: no lanes");
        return;
    };
    for lane in &sync.lanes {
        eprintln!(
            "{what}: {} rounds {} ops {} syncing {}",
            lane.name, lane.sync_rounds, lane.ops_received, lane.syncing
        );
    }
}
