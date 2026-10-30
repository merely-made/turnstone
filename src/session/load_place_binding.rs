// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Load and validate this session's public shared-place binding. Absence means
/// a personal session; malformed or unsupported content is an explicit error.
pub fn load_place_binding(session_dir: &Path) -> Result<Option<PlaceBindingV1>, PlaceSidecarError> {
    let path = place_binding_path(session_dir);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let binding: PlaceBindingV1 = serde_json::from_slice(&bytes)?;
    binding.validate()?;
    Ok(Some(binding))
}
