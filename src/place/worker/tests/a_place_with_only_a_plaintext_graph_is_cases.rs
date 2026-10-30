// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// E2: a place whose shared graph is a plaintext store from before
/// encryption is refused by name, not opened with an empty graph, and its
/// plaintext records are never read as encrypted.
#[test]
fn a_place_with_only_a_plaintext_graph_is_refused_by_name() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("profile");
    let identity = RootIdentity::Local(InMemoryProvider::from_seed([0x84; 32]));
    let binding = binding(0x35);
    seed_profile(&directory, &identity, &binding, 1);
    // Positive control: the same profile opens while its store is encrypted.
    let (open, snapshot) = open_cached_place(&directory, &binding, &identity, &settings()).unwrap();
    assert_eq!(snapshot.graph.nodes, 1);
    drop(open);

    let stores = place_store_dir(&directory);
    std::fs::remove_file(stores.join(GRAPH_STORE)).unwrap();
    let backend = RedbBackend::open(stores.join(LEGACY_GRAPH_STORE)).unwrap();
    let mut plaintext = commons::Replica::for_identity(backend, binding.root.0, &identity).unwrap();
    pollster::block_on(plaintext.edit(|log| {
        log.insert_node(&Author::new("turnstone"), Container::new("plaintext"));
    }))
    .unwrap();
    drop(plaintext);

    let Err(error) = open_cached_place(&directory, &binding, &identity, &settings()) else {
        panic!("a plaintext-only place opened");
    };
    assert_eq!(error, LEGACY_GRAPH_REFUSAL);
    assert!(
        !stores.join(GRAPH_STORE).exists(),
        "the refusal created nothing"
    );
    let lines = crate::place::PlaceState::Degraded {
        binding,
        generation: 1,
        error,
    }
    .status_lines();
    assert!(
        lines
            .iter()
            .any(|row| row == "Place unavailable: this place predates encrypted shared graphs"),
        "{lines:?}"
    );
}

#[test]
fn two_profiles_reopen_their_own_retained_place_state() {
    let root = std::env::temp_dir().join(format!("turnstone-place-worker-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let first_dir = root.join("first");
    let second_dir = root.join("second");
    let first_identity = RootIdentity::Local(InMemoryProvider::from_seed([0x81; 32]));
    let second_identity = RootIdentity::Local(InMemoryProvider::from_seed([0x82; 32]));
    let first_binding = binding(0x21);
    let second_binding = binding(0x31);
    seed_profile(&first_dir, &first_identity, &first_binding, 1);
    seed_profile(&second_dir, &second_identity, &second_binding, 2);

    let (_, first) =
        open_cached_place(&first_dir, &first_binding, &first_identity, &settings()).unwrap();
    let (_, second) =
        open_cached_place(&second_dir, &second_binding, &second_identity, &settings()).unwrap();
    assert_eq!((first.graph.nodes, first.chat.messages), (1, 1));
    assert_eq!((second.graph.nodes, second.chat.messages), (2, 2));
    assert!(first.group.has_current_epoch);
    assert!(second.group.has_current_epoch);
    assert_eq!(first.moot.delegated_certificates, 1);
    assert_eq!(second.moot.delegated_certificates, 1);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_valid_invitation_admits_and_a_refused_one_leaves_nothing_behind() {
    let root = std::env::temp_dir().join(format!("turnstone-place-admit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let founder = InMemoryProvider::from_seed([0xb1; 32]);
    let joiner = RootIdentity::Local(InMemoryProvider::from_seed([0xb2; 32]));
    let stranger = RootIdentity::Local(InMemoryProvider::from_seed([0xb3; 32]));
    let binding = binding(0x61);
    let joined = root.join("joiner");
    let joiner_id = joiner.master_public_key().to_bytes();

    // Being invitable is a precondition, not a consequence: the identity
    // exists and its pre-key is published before any envelope is authored.
    let published = prepare_group_identity(&joined, &joiner, binding.moot.0).unwrap();
    assert_eq!(
        prepare_group_identity(&joined, &joiner, binding.moot.0).unwrap(),
        published,
        "preparing twice must not rotate the identity a welcome is addressed to"
    );
    let invite = real_invitation(
        &root.join("founder"),
        &binding,
        &founder,
        published.clone(),
        joiner_id,
    );

    // The happy path: every domain answers, and only then is a secret sealed.
    let admitted =
        admit_invitation(&joined, &invite, &joiner, &settings()).expect("valid invitation");
    assert_eq!(admitted.binding, binding);
    assert_eq!(admitted.moot.members, 2, "Gemot's governance fold");
    assert_eq!(admitted.group_members, 2, "Stickleback's crypto fold");
    assert_eq!(residue(&joined), (true, true, true));

    // A tampered artifact never reaches a domain, and leaves no store.
    let tampered_dir = root.join("tampered");
    let mut tampered = invite.clone();
    let ArtifactRefV1::Inline { bytes, .. } = &mut tampered.governance else {
        unreachable!("fixture is inline")
    };
    bytes.push(0);
    let error = admit_invitation(&tampered_dir, &tampered, &joiner, &settings()).unwrap_err();
    assert!(error.contains("declared digest"), "{error}");
    assert_eq!(residue(&tampered_dir), (false, false, false));

    // A stranger holding the same envelope is refused by Gemot membership,
    // not by anything the envelope says about itself.
    let stranger_dir = root.join("stranger");
    let error = admit_invitation(&stranger_dir, &invite, &stranger, &settings()).unwrap_err();
    assert!(error.contains("membership does not contain"), "{error}");
    assert_eq!(residue(&stranger_dir), (false, false, false));

    // An envelope naming a non-member as its author is refused even though
    // the welcome frames themselves are genuine.
    let forged_dir = root.join("forged-author");
    let mut forged = invite.clone();
    forged.inviter = stranger.master_public_key().to_bytes();
    let error = admit_invitation(&forged_dir, &forged, &joiner, &settings()).unwrap_err();
    assert!(error.contains("outside Gemot membership"), "{error}");
    assert_eq!(residue(&forged_dir), (false, false, false));

    // Check 4's two refusals need a welcome genuinely addressed to the
    // directory under test, since every prepared identity draws a fresh
    // random recipient. Each gets its own matched invitation.
    //
    // An epoch other than the one the invitation names is refused even
    // though the welcome itself is genuine and processes cleanly.
    let (wrong_epoch_dir, mut wrong_epoch) =
        matched_case(&root, "wrong-epoch", &binding, &founder, &joiner);
    wrong_epoch.expected_epoch = [0xee; 32];
    let error = admit_invitation(&wrong_epoch_dir, &wrong_epoch, &joiner, &settings()).unwrap_err();
    assert!(error.contains("does not name"), "{error}");

    // Membership heads the inviter pinned but Gemot did not converge to.
    // This is what stops a welcome minted before a removal from handing the
    // joiner a key a departed member still holds.
    let (stale_heads_dir, mut stale_heads) =
        matched_case(&root, "stale-heads", &binding, &founder, &joiner);
    stale_heads.membership_heads = vec![[0xaa; 32]];
    let error = admit_invitation(&stale_heads_dir, &stale_heads, &joiner, &settings()).unwrap_err();
    assert!(error.contains("did not converge"), "{error}");

    // The inviter's own time bound, on an otherwise valid envelope, with a
    // clock moved past it. Nothing is created, so expiry is cheap and
    // cannot be reached by an envelope that would have failed anyway.
    let expired_dir = root.join("expired");
    let expired_clock = PlaceWorkerSettings {
        authority_clock: AuthorityClock::Fixed(invite.not_after_ms + 1),
        ..PlaceWorkerSettings::default()
    };
    let error = admit_invitation(&expired_dir, &invite, &joiner, &expired_clock).unwrap_err();
    assert_eq!(error, "invitation expired at 1970-01-01T00:00:01Z");
    assert_eq!(residue(&expired_dir), (false, false, false));

    // Aliased Commons scopes are refused before any store is created.
    let aliased_dir = root.join("aliased");
    let mut aliased = invite.clone();
    aliased.binding.chat = ChatSpaceId(aliased.binding.root.0);
    let error = admit_invitation(&aliased_dir, &aliased, &joiner, &settings()).unwrap_err();
    assert!(error.contains("same scope"), "{error}");
    assert_eq!(residue(&aliased_dir), (false, false, false));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_authored_invitation_admits_on_the_other_side() {
    let root = std::env::temp_dir().join(format!("turnstone-place-author-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let founder = InMemoryProvider::from_seed([0xc1; 32]);
    let joiner = RootIdentity::Local(InMemoryProvider::from_seed([0xc2; 32]));
    let outsider = RootIdentity::Local(InMemoryProvider::from_seed([0xc3; 32]));
    let binding = binding(0x71);
    let host = root.join("host");
    let guest = root.join("guest");

    // Both sides prepare a durable group identity before any envelope
    // exists. This is the precondition, not a step in the flow.
    let joiner_prekey = prepare_group_identity(&guest, &joiner, binding.moot.0).unwrap();
    found_place_for_authoring(
        &host,
        &binding,
        &founder,
        joiner.master_public_key().to_bytes(),
    );
    // The host founds its crypto group; the guest only prepares an
    // identity. Different preconditions for different roles.
    found_place_group(&host, &founder, binding.moot.0).unwrap();
    assert!(
        found_place_group(&host, &founder, binding.moot.0).is_ok(),
        "founding twice must not strand the epochs already handed out"
    );

    // Someone the Moot never admitted cannot be invited, and the refusal
    // names the real reason rather than minting an envelope that could
    // only ever be refused on arrival.
    let outsider_prekey =
        prepare_group_identity(&root.join("outsider"), &outsider, binding.moot.0).unwrap();
    let error = author_invitation(
        &host,
        &binding,
        &founder,
        &outsider_prekey,
        AUTHORITY_AT_MS + 1_000,
        Vec::new(),
        None,
        &settings(),
    )
    .unwrap_err();
    assert!(
        error.contains("does not contain the invited root"),
        "{error}"
    );

    let invite = author_invitation(
        &host,
        &binding,
        &founder,
        &joiner_prekey,
        AUTHORITY_AT_MS + 1_000,
        Vec::new(),
        None,
        &settings(),
    )
    .unwrap();

    // The product path produced it and the product path accepts it: no
    // test fixture stands between the two sides.
    let admitted = admit_invitation(&guest, &invite, &joiner, &settings())
        .expect("an authored invitation admits");
    assert_eq!(admitted.binding, binding);
    assert_eq!(admitted.moot.members, 2);
    assert_eq!(admitted.group_members, 2);
    assert_eq!(residue(&guest), (true, true, true));
    let _ = std::fs::remove_dir_all(&root);
}

/// Found, offer, invite, admit — through the product functions only, with
/// no fixture standing between the two profiles.
///
/// The refusal in the middle is the point of the ordering: a pre-key whose
/// root the Moot has never admitted cannot be invited, so an envelope is
/// only ever minted for someone governance already knows.
#[test]
fn a_founded_place_invites_an_offered_prekey_and_admits_it() {
    let root = std::env::temp_dir().join(format!("turnstone-place-found-{}", uuid::Uuid::new_v4()));
    let host = root.join("host");
    let guest = root.join("guest");
    std::fs::create_dir_all(&host).unwrap();
    std::fs::create_dir_all(&guest).unwrap();
    let host_identity = RootIdentity::Local(InMemoryProvider::from_seed([0xd1; 32]));
    let guest_identity = RootIdentity::Local(InMemoryProvider::from_seed([0xd2; 32]));
    // The product clock, not a fixture's: the windows under test are the
    // ones a real founding opens.
    let settings = PlaceWorkerSettings::default();

    let binding = found_place(&host, &host_identity, "Hearth", &settings).unwrap();
    // Founding samples the parent's not-before bound. A later admission
    // must start after that bound, rather than reuse a pre-founding time.
    let now_ms = settings.authority_clock.now_ms();
    assert_eq!(binding.default_channel, DEFAULT_CHANNEL);
    assert_ne!(binding.moot.0, binding.root.0);
    assert_ne!(binding.moot.0, binding.chat.0);
    assert_ne!(binding.root.0, binding.chat.0);

    // The founder's own writes read Effective against its own Moot; the
    // self-delegation is what makes that true.
    let (opened, snapshot) = open_cached_place(&host, &binding, &host_identity, &settings).unwrap();
    let permissions = snapshot.permissions.clone().unwrap();
    assert!(permissions.message_write, "the founder may speak here");
    assert!(permissions.graph_write, "the founder may share here");
    assert_eq!(snapshot.moot.members, 1);
    assert_eq!(snapshot.chat.channels, 1);
    assert_eq!(
        snapshot.personae_root,
        host_identity.master_public_key().to_bytes()
    );
    assert!(!snapshot.graph_digest.is_empty() && !snapshot.chat_digest.is_empty());
    drop(opened);

    // The guest publishes its group identity for this Moot. An offer, not
    // an admission: nothing on the host has changed yet.
    let prekey = prepare_group_identity(&guest, &guest_identity, binding.moot.0).unwrap();
    let refused = author_invitation(
        &host,
        &binding,
        &host_identity,
        &prekey,
        now_ms + 60_000,
        Vec::new(),
        None,
        &settings,
    )
    .unwrap_err();
    assert!(
        refused.contains("membership does not contain the invited root"),
        "{refused}"
    );

    let moot = pollster::block_on(MootFile::open_existing(
        place_store_dir(&host).join("gemot"),
        MootId(binding.moot.0),
        settings.retention.clone(),
    ))
    .unwrap();
    let guest_root = guest_identity.master_public_key().to_bytes();
    admit_member(
        &moot,
        &binding,
        &host_identity,
        guest_root,
        Writer,
        now_ms,
        None,
        true,
    )
    .unwrap();
    // Idempotent: inviting the same root twice must not double the fold.
    let projection_grant = admit_member(
        &moot,
        &binding,
        &host_identity,
        guest_root,
        Writer,
        now_ms,
        None,
        true,
    )
    .unwrap()
    .projection;
    assert!(
        projection_grant.is_some(),
        "a writer is admitted at the projection door as well as the Moot"
    );
    let mut group = PlaceGroup::open(&host, &host_identity, binding.moot.0).unwrap();
    let (invite, _) = author_invitation_with(
        &moot,
        &mut group,
        &host,
        &binding,
        &host_identity,
        &prekey,
        now_ms + 7 * 24 * 60 * 60 * 1000,
        Vec::new(),
        projection_grant,
    )
    .unwrap();
    drop((moot, group));

    let admitted = admit_invitation(&guest, &invite, &guest_identity, &settings).unwrap();
    assert_eq!(admitted.binding, binding);
    assert_eq!(admitted.moot.members, 2);
    assert_eq!(admitted.group_members, 2);

    // The admitted guest reopens through the ordinary path and holds the
    // capability the delegation granted.
    let (guest_open, guest_snapshot) =
        open_cached_place(&guest, &binding, &guest_identity, &settings).unwrap();
    assert!(guest_snapshot.permissions.clone().unwrap().message_write);
    assert_eq!(guest_snapshot.personae_root, guest_root);
    // Two peers that have converged on nothing yet still agree on the
    // digest of nothing; the point is that the digest is comparable.
    assert_eq!(guest_snapshot.graph_digest, snapshot.graph_digest);
    drop(guest_open);
    let _ = std::fs::remove_dir_all(&root);
}
