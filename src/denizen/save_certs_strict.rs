// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn save_certs_strict(
    session_dir: &Path,
    subject_hex: &str,
    certs: &[SignedDelegationCertificate],
) -> Result<(), String> {
    let target = certs_path(session_dir, subject_hex);
    let parent = target
        .parent()
        .ok_or_else(|| "certificate path has no parent".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|err| format!("cannot create certificate directory: {err}"))?;
    let json = serde_json::to_vec_pretty(certs)
        .map_err(|err| format!("cannot encode certificates: {err}"))?;
    std::fs::write(&target, json).map_err(|err| format!("cannot persist certificates: {err}"))
}
