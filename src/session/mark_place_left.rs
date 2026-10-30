// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Mark this session as having locally left its place: write a small marker
/// beside `place.json` recording the retained counts at the moment of
/// leaving. The binding, the rendezvous hints, and every retained store are
/// left exactly as they were -- `Rejoin` needs all three, and the marker's
/// presence alone is what `Left` reads on, not the binding's absence.
pub fn mark_place_left(
    session_dir: &Path,
    retained: &crate::place::PlaceLeftSummary,
) -> Result<(), PlaceSidecarError> {
    std::fs::create_dir_all(session_dir)?;
    let target = session_dir.join(PLACE_LEFT_FILE);
    let temporary = target.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(&PlaceLeftMarkerV1 {
        version: PLACE_LEFT_MARKER_VERSION,
        graph_nodes: retained.graph_nodes,
        chat_messages: retained.chat_messages,
        members: retained.members,
    })?;
    if let Err(error) = (|| -> std::io::Result<()> {
        std::fs::write(&temporary, bytes)?;
        if target.exists() {
            std::fs::remove_file(&target)?;
        }
        std::fs::rename(&temporary, &target)
    })() {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}
