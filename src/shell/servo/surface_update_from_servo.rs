// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Servo pins AccessKit 0.24 while Mere's surface contract is 0.25. 0.25 only
/// adds node properties and renames `Tree` to `TreeInfo`, so the serde form of
/// a 0.24 update is a valid 0.25 update. Convert there, at this one boundary.
pub(super) fn surface_update_from_servo(
    update: &servo::accesskit::TreeUpdate,
) -> Result<SurfaceAccessibilityUpdate, serde_json::Error> {
    serde_json::from_value(serde_json::to_value(update)?)
}
