// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Remove `member`'s recipient from the group and rotate, so what is authored
/// next is sealed to an epoch it never receives.
///
/// The recipient comes from the group session's own pre-key bindings, not
/// from a record beside them: this profile resolves whoever it registered,
/// which is every root it welcomed. A root it cannot resolve is REFUSED
/// rather than skipped, because revoking in Gemot without rotating would
/// leave the member reading. One already removed has been rotated away from,
/// so it authors nothing.
pub(super) fn rekey_without(
    open: &mut OpenPlace,
    identity: &dyn IdentityProvider,
    member: [u8; 32],
) -> Result<Vec<stickleback::Operation<GroupKeyExt>>, String> {
    let Some(recipient) = open.group.session.recipient_for_root(member) else {
        return Err(NO_GROUP_RECIPIENT.to_string());
    };
    let present = open
        .group
        .session
        .members()
        .map_err(|error| format!("read group membership: {error}"))?
        .contains(&recipient);
    if !present {
        return Ok(Vec::new());
    }
    let mut next = open.group.copy()?;
    let removal = next
        .remove(recipient)
        .map_err(|error| format!("remove the member from the group: {error}"))?;
    let rotation = next
        .update()
        .map_err(|error| format!("rotate the group: {error}"))?;
    let authored = open
        .group
        .commit(&open.directory, identity, next, &[removal, rotation])?;
    // A key-handle refresh: what the rotation makes readable is re-admitted
    // before the caller's snapshot folds.
    open.readmit_parked()?;
    Ok(authored)
}
