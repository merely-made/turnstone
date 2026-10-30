// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Load a participant's certificates; empty when absent or unreadable.
pub fn load_certs(session_dir: &Path, subject_hex: &str) -> Vec<SignedDelegationCertificate> {
    let path = certs_path(session_dir, subject_hex);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    match serde_json::from_str(&text) {
        Ok(certs) => certs,
        Err(err) => {
            tracing::warn!(%err, path = ?path, "failed to parse a participant's certificates");
            Vec::new()
        },
    }
}
