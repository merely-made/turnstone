// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Remove one member: rotate the group away from it, revoke every delegation
/// this profile issued to it, then its membership, then refuse its projection
/// grants at this door.
///
/// The rekey and the revocations come before the removal, so a failure part
/// way leaves the member removable on a retry. Stored before published, as
/// every other authored fact.
pub(super) fn revoke_member(
    open: &mut OpenPlace,
    identity: &dyn IdentityProvider,
    member: [u8; 32],
    at_ms: u64,
) -> Result<(), String> {
    let moot_id = open.binding.moot.0;
    let local = identity.master_public_key().to_bytes();
    let snapshot = pollster::block_on(open.moot.snapshot())
        .map_err(|error| format!("materialize Gemot: {error}"))?;
    let members = &snapshot.membership.members;
    if !members
        .iter()
        .any(|entry| entry.member == local && entry.access == MootAccessLevel::Manage)
    {
        return Err("only a profile that manages this place can revoke a member".to_string());
    }
    if member == local {
        return Err("a manager cannot revoke itself".to_string());
    }
    if !members.iter().any(|entry| entry.member == member) {
        return Err("that root is not a member of this place".to_string());
    }
    let rules = &snapshot.governance.rules;
    let delegations = pollster::block_on(open.moot.delegations())
        .map_err(|error| format!("materialize Gemot delegations: {error}"))?;
    let key = founder_signing_key(identity, moot_id)?;
    let mut revoked = Vec::new();
    let grants: Vec<_> = delegations
        .projections(moot_id, rules, at_ms)
        .into_iter()
        .filter(|grant| grant.issuer == local && grant.subject == member && !grant.directly_revoked)
        .collect();
    let withdrawn = if open.offers_projection() {
        grants
            .iter()
            .map(|grant| {
                crate::place::projection_host::projection_grant_revocation(
                    identity,
                    moot_id,
                    member,
                    grant.not_before_ms,
                    grant.expires_at_ms,
                    at_ms,
                )
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    let rekeyed = rekey_without(open, identity, member)?;
    for grant in grants {
        let scope = CapabilityScope {
            domain: MOOT_DELEGATION_DOMAIN.into(),
            resource: moot_id.to_vec(),
            path_prefix: grant.path_prefix.clone(),
            actions: grant.actions.clone(),
        };
        let statement = SignedDelegationRevocation::issue(
            &ProviderRef(identity),
            DelegationRevocation::new(
                grant.certificate,
                local,
                scope,
                at_ms,
                place_tag(b"turnstone.place.revocation.v1/", grant.certificate.0),
            ),
        )
        .map_err(|error| format!("sign the revocation: {error}"))?;
        revoked.push(
            pollster::block_on(
                open.moot
                    .delegation_store()
                    .author_revoke(&key, rules, statement),
            )
            .map_err(|error| format!("revoke the Commons delegation: {error}"))?,
        );
    }
    let removal = pollster::block_on(
        open.moot
            .membership_store()
            .author_for_identity(identity, MootMembershipAction::Remove { member }),
    )
    .map_err(|error| format!("remove the member: {error}"))?;
    if let Some(lanes) = &open.lanes {
        for operation in rekeyed {
            lanes.publish_group_key(operation)?;
        }
        for operation in revoked {
            lanes.publish_delegation(operation)?;
        }
        lanes.publish_membership(removal)?;
        lanes.withdraw_projection_grants(&withdrawn)?;
    }
    Ok(())
}
