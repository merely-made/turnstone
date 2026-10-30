// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Build a real invitation: a founded Moot whose membership contains both
/// roots, exported as a plain drop, plus a genuine Stickleback welcome
/// addressed to the joiner's registered pre-key.
pub(super) fn real_invitation(
    founder_dir: &Path,
    binding: &PlaceBindingV1,
    founder: &InMemoryProvider,
    joiner_published_prekey: Vec<u8>,
    joiner_id: [u8; 32],
) -> PlaceInviteV1 {
    let founder_id = founder.master_public_key().to_bytes();
    let stores = place_store_dir(founder_dir);
    std::fs::create_dir_all(&stores).unwrap();
    let moot = pollster::block_on(MootFile::open(
        stores.join("gemot"),
        MootId(binding.moot.0),
        founder_id,
        settings().retention,
    ))
    .unwrap();
    pollster::block_on(moot.found(
        founder.master_keypair().to_seed(),
        None,
        None,
        place_rules(founder_id, binding.moot.0),
        1,
    ))
    .unwrap();
    pollster::block_on(moot.membership_store().author_for_identity(
        founder,
        MootMembershipAction::Create {
            initial_members: vec![MootMember {
                member: founder_id,
                access: MootAccessLevel::Manage,
            }],
        },
    ))
    .unwrap();
    pollster::block_on(moot.membership_store().author_for_identity(
        founder,
        MootMembershipAction::Add {
            member: joiner_id,
            access: MootAccessLevel::Write,
        },
    ))
    .unwrap();

    let mut drop_bytes = Vec::new();
    pollster::block_on(moot.export_plain_drop(
        &mut drop_bytes,
        DropExportProfile::default(),
        DropLimits::default(),
    ))
    .unwrap();
    let membership_heads = pollster::block_on(moot.snapshot())
        .unwrap()
        .membership
        .auth_heads;
    drop(moot);

    // The joiner's group identity already exists and its pre-key is
    // published: that is the precondition for being invitable at all, since
    // the recipient id comes from the RNG rather than the Personae root.
    let joiner_prekey = GroupPrekeyBundle::from_bytes(&joiner_published_prekey).unwrap();
    let joiner_recipient = joiner_prekey.recipient;
    let (mut founder_group, founder_prekey) =
        GroupSession::new(GroupSessionId(binding.moot.0), founder).unwrap();
    founder_group.register_prekey(&joiner_prekey).unwrap();
    founder_group.create(&[]).unwrap();
    let dispatch = founder_group.add(joiner_recipient).unwrap();
    let direct = dispatch.direct_for(joiner_recipient).unwrap();

    PlaceInviteV1 {
        version: PLACE_INVITE_VERSION,
        binding: binding.clone(),
        founder: founder_id,
        inviter: founder_id,
        inviter_prekey: inline_artifact(&founder_prekey.to_bytes().unwrap()),
        governance: inline_artifact(&drop_bytes),
        key_welcome: inline_artifact(&dispatch.control.to_bytes().unwrap()),
        key_direct: inline_artifact(&direct.to_bytes().unwrap()),
        expected_epoch: founder_group
            .current_epoch()
            .expect("adding a member installs an epoch"),
        membership_heads,
        // Comfortably after the pinned AUTHORITY_AT_MS, so only the test
        // that moves the clock forward sees an expiry.
        not_after_ms: AUTHORITY_AT_MS + 1_000,
        rendezvous: Vec::new(),
        projection_grant: None,
    }
}
