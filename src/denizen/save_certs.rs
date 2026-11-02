// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Persist a participant's certificates. Best-effort like every sidecar; a failed
/// save warns, and the participant loses authority on the next adopt rather than
/// silently keeping it.
pub fn save_certs(session_dir: &Path, subject_hex: &str, certs: &[SignedDelegationCertificate]) {
    let target = certs_path(session_dir, subject_hex);
    let result = (|| -> std::io::Result<()> {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(certs).map_err(std::io::Error::other)?;
        std::fs::write(&target, json)
    })();
    if let Err(err) = result {
        tracing::warn!(%err, path = ?target, "failed to persist a participant's certificates");
    }
}
