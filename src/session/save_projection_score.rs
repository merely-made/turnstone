// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Persist a score atomically. It is view state: a missing or malformed score
/// must never prevent the graph session itself from opening.
pub fn save_projection_score(session_dir: &Path, score: &Score) {
    let target = projection_score_path(session_dir);
    let tmp = target.with_extension("json.tmp");
    let result = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(session_dir)?;
        let bytes = serde_json::to_vec_pretty(score).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, bytes)?;
        if target.exists() {
            std::fs::remove_file(&target)?;
        }
        std::fs::rename(&tmp, &target)
    })();
    if let Err(err) = result {
        tracing::warn!(%err, path = ?target, "failed to persist projection score");
        let _ = std::fs::remove_file(tmp);
    }
}
