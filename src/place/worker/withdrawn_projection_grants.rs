// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Revocations of every projection grant this profile issued and has since
/// withdrawn, for a door that binds after the withdrawal. Rebuilt from the
/// Gemot fold rather than kept on the side, so the door follows Gemot.
pub(crate) fn withdrawn_projection_grants(
    open: &OpenPlace,
    identity: &dyn IdentityProvider,
) -> Result<Vec<SignedDelegationRevocation>, String> {
    let moot_id = open.binding.moot.0;
    let local = identity.master_public_key().to_bytes();
    let snapshot = pollster::block_on(open.moot.snapshot())
        .map_err(|error| format!("materialize Gemot: {error}"))?;
    let delegations = pollster::block_on(open.moot.delegations())
        .map_err(|error| format!("materialize Gemot delegations: {error}"))?;
    delegations
        .projections(moot_id, &snapshot.governance.rules, 0)
        .into_iter()
        .filter(|grant| grant.issuer == local && grant.directly_revoked)
        // The statement's time is not retained in the fold; the ledger
        // records only which certificate is withdrawn and by whom.
        .map(|grant| {
            crate::place::projection_host::projection_grant_revocation(
                identity,
                moot_id,
                grant.subject,
                grant.not_before_ms,
                grant.expires_at_ms,
                grant.not_before_ms,
            )
        })
        .collect()
}
