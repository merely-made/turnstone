// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Withdraw the seeded delegation on the retained Gemot lane.
pub(super) fn revoke_place_delegation(
    directory: &Path,
    binding: &PlaceBindingV1,
    subject: [u8; 32],
) {
    let founder = founder_for(binding);
    let founder_id = founder.master_public_key().to_bytes();
    let rules = place_rules(founder_id, binding.moot.0);
    let certificate = place_delegation(&founder, binding.moot.0, subject);
    let moot = pollster::block_on(MootFile::open(
        place_store_dir(directory).join("gemot"),
        MootId(binding.moot.0),
        founder_id,
        settings().retention,
    ))
    .unwrap();
    let revocation = SignedDelegationRevocation::issue(
        &founder,
        DelegationRevocation::new(
            certificate.certificate.id(),
            founder_id,
            certificate.certificate.scope.clone(),
            60,
            [2; 32],
        ),
    )
    .unwrap();
    pollster::block_on(moot.delegation_store().author_revoke(
        &founder_signing_key(&founder, binding.moot.0),
        &rules,
        revocation,
    ))
    .unwrap();
    drop(moot);
}
