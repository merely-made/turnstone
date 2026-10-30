// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Found a Moot with both roots in its membership fold, as the authoring
/// side would already have.
pub(crate) fn found_place_for_authoring(
    directory: &Path,
    binding: &PlaceBindingV1,
    founder: &InMemoryProvider,
    joiner_root: [u8; 32],
) {
    let founder_id = founder.master_public_key().to_bytes();
    let stores = place_store_dir(directory);
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
            member: joiner_root,
            access: MootAccessLevel::Write,
        },
    ))
    .unwrap();
}
