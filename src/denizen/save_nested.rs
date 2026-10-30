// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Persist a resident's nested log (whole-log JSON; the log IS the graph).
/// Best-effort like every sidecar: a failed save warns, never panics.
pub fn save_nested(session_dir: &Path, log_id: &str, nested: &GraphLog<Container, Relation>) {
    let target = nested_log_path(session_dir, log_id);
    let result = (|| -> std::io::Result<()> {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(nested.log()).map_err(std::io::Error::other)?;
        std::fs::write(&target, json)
    })();
    if let Err(err) = result {
        tracing::warn!(%err, path = ?target, "failed to persist a participant's nested log");
    }
}
