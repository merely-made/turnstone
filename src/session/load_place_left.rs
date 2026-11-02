// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The retained counts a local leave recorded, or `None` when this session
/// was not left (the ordinary case, and what a fresh reconnect leaves it as
/// once `Rejoin` clears the mark).
pub fn load_place_left(
    session_dir: &Path,
) -> Result<Option<crate::place::PlaceLeftSummary>, PlaceSidecarError> {
    let bytes = match std::fs::read(session_dir.join(PLACE_LEFT_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let marker: PlaceLeftMarkerV1 = serde_json::from_slice(&bytes)?;
    if marker.version != PLACE_LEFT_MARKER_VERSION {
        return Err(PlaceSidecarError::UnsupportedVersion(marker.version));
    }
    Ok(Some(crate::place::PlaceLeftSummary {
        graph_nodes: marker.graph_nodes,
        chat_messages: marker.chat_messages,
        members: marker.members,
    }))
}
