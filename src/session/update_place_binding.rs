// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Update a binding that admission already established.
///
/// Returns `Ok(false)` when none is present. Routine session saves use this
/// rather than [`save_place_binding`] so that saving can never *mint* a
/// `place.json` for a session that was never admitted. Creating one is
/// admission's job alone, and keeping that structural means the ordering
/// survives someone adding a new save path later.
pub fn update_place_binding(
    session_dir: &Path,
    binding: &PlaceBindingV1,
) -> Result<bool, PlaceSidecarError> {
    if !place_binding_path(session_dir).exists() {
        return Ok(false);
    }
    save_place_binding(session_dir, binding)?;
    Ok(true)
}
