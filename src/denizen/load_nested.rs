// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Load a resident's nested log; `None` when absent or unreadable (the
/// participant then starts on an empty world — its binding still stands).
pub fn load_nested(session_dir: &Path, log_id: &str) -> Option<GraphLog<Container, Relation>> {
    let path = nested_log_path(session_dir, log_id);
    let text = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<Journal<chartulary::Batch<Container, Relation>>>(&text) {
        Ok(log) => Some(GraphLog::replay(log)),
        Err(err) => {
            tracing::warn!(%err, path = ?path, "failed to parse a participant's nested log");
            None
        },
    }
}
