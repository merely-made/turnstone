// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Create this place's crypto group with this profile as its first member.
///
/// The founding counterpart to [`prepare_group_identity`]. A Moot's governance
/// fold and its DCGKA group are founded separately and neither implies the
/// other: Gemot decides who belongs, Stickleback decides who can read. Until
/// this runs, the founder holds a group identity but is not an active member
/// of any group, so it cannot welcome anyone.
///
/// Idempotent. A group that already has members is left exactly as it is
/// rather than re-created, since re-creating would strand every epoch already
/// handed out.
pub fn found_place_group(
    directory: &Path,
    identity: &dyn IdentityProvider,
    moot: [u8; 32],
) -> Result<(), String> {
    prepare_group_identity(directory, identity, moot)?;
    let mut group = load_group_session(directory, identity, moot)?;
    let members = group
        .members()
        .map_err(|error| format!("read group membership: {error}"))?;
    if !members.is_empty() {
        return Ok(());
    }
    group
        .create(&[])
        .map_err(|error| format!("create place group: {error}"))?;
    save_group_session(directory, identity, &group)
}
