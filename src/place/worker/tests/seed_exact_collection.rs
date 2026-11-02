// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn seed_exact_collection(
    directory: &Path,
    identity: &RootIdentity,
    binding: &PlaceBindingV1,
) -> (PlaceWorkerSettings, PlaceCollectionVersion, [u8; 32]) {
    seed_profile(directory, identity, binding, 0);
    let founder = founder_for(binding);
    assert_eq!(
        identity.master_public_key().to_bytes(),
        founder.master_public_key().to_bytes(),
        "the focused collection fixture uses the Moot founder"
    );
    let (settings, manifest, other_manifest) = seed_exact_collection_captures(directory);

    let collection_id = CollectionId([0xc7; 32]);
    let collection = CollectionRef {
        moot_id: binding.moot.0,
        collection_id,
    };
    let moot = pollster::block_on(MootFile::open_existing(
        place_store_dir(directory).join("gemot"),
        MootId(binding.moot.0),
        settings.retention.clone(),
    ))
    .unwrap();
    let mut rules = place_rules(founder.master_public_key().to_bytes(), binding.moot.0);
    rules.admission = Policy::MembersOnly {
        rate_limit: 20,
        rate_window_ms: 60_000,
    };
    rules.grant(CapabilityGrant {
        id: COLLECTION_GRANT,
        subject: founder.master_public_key().to_bytes(),
        path_prefix: cap_path(&Cap::scope("moot").unwrap()),
        not_before_ms: 1,
        expires_at_ms: Some(1_000),
        delegation_depth: 0,
    });
    pollster::block_on(moot.amend(founder.master_keypair().to_seed(), rules, 2)).unwrap();
    pollster::block_on(moot.membership_store().author_for_identity(
        &founder,
        MootMembershipAction::Create {
            initial_members: vec![MootMember {
                member: founder.master_public_key().to_bytes(),
                access: MootAccessLevel::Manage,
            }],
        },
    ))
    .unwrap();
    let share = pollster::block_on(moot.share_for_identity(
        &founder,
        manifest,
        FLEECE_ANNOTATION_SCHEMA_ID.into(),
        "Field note".into(),
        20,
    ))
    .unwrap();
    pollster::block_on(moot.share_for_identity(
        &founder,
        other_manifest,
        FLEECE_ANNOTATION_SCHEMA_ID.into(),
        "Unselected note".into(),
        21,
    ))
    .unwrap();
    pollster::block_on(moot.declare_collection_for_identity(
        &founder,
        collection_id,
        "Field notes".into(),
        None,
        22,
    ))
    .unwrap();
    let declared = pollster::block_on(moot.authorized_collection(collection_id, AUTHORITY_AT_MS))
        .unwrap()
        .unwrap();
    pollster::block_on(moot.set_collection_membership_for_identity(
        &founder,
        collection,
        declared.heads,
        CollectionChange::SetMembership {
            contribution: ContributionRef {
                moot_id: binding.moot.0,
                share: share.operation,
            },
            included: true,
        },
        23,
    ))
    .unwrap();
    let version = pollster::block_on(moot.authorized_collection(collection_id, AUTHORITY_AT_MS))
        .unwrap()
        .unwrap()
        .version;
    drop(moot);
    (
        settings,
        PlaceCollectionVersion {
            moot: PlaceId(version.collection.moot_id),
            collection: PlaceCollectionId(version.collection.collection_id.0),
            frontier: version.frontier,
            membership_commitment: version.membership_commitment,
        },
        share.operation,
    )
}
