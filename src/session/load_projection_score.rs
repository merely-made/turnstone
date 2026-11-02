// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Restore the last valid score. A corrupt sidecar is diagnosed and ignored;
/// the canvas will recompute a fresh score from current graph truth.
pub fn load_projection_score(session_dir: &Path) -> Option<Score> {
    let path = projection_score_path(session_dir);
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(score) => Some(score),
            Err(err) => {
                tracing::warn!(%err, path = ?path, "failed to parse projection score");
                None
            },
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            tracing::warn!(%err, path = ?path, "failed to read projection score");
            None
        },
    }
}
