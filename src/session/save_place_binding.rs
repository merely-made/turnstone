// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Persist the public binding through an adjacent temporary file. Group
/// secrets, welcome material, and live rendezvous state have no representation
/// in this sidecar.
pub fn save_place_binding(
    session_dir: &Path,
    binding: &PlaceBindingV1,
) -> Result<(), PlaceSidecarError> {
    binding.validate()?;
    std::fs::create_dir_all(session_dir)?;
    let target = place_binding_path(session_dir);
    let temporary = target.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(binding)?;
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
