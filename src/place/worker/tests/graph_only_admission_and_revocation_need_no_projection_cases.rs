// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn graph_only_admission_and_revocation_need_no_projection_signer() {
    struct NoProjectionKeys(InMemoryProvider);
    impl IdentityProvider for NoProjectionKeys {
        fn master_public_key(&self) -> identity::Ed25519PublicKey {
            self.0.master_public_key()
        }
        fn derive_keypair(
            &self,
            salt: &[u8],
        ) -> Result<identity::Ed25519Keypair, identity::IdentityError> {
            let prefix = insigne::delegation::delegation_signing_prefix(
                graphshell_host::admission::GRAPHSHELL_DOMAIN,
            );
            if salt.starts_with(&prefix) {
                return Err(identity::IdentityError::Backend(
                    "projection signing is not offered".into(),
                ));
            }
            self.0.derive_keypair(salt)
        }
        fn attest_derived_key(
            &self,
            salt: &[u8],
        ) -> Result<insigne::DerivedKeyAttestation, identity::IdentityError> {
            self.derive_keypair(salt)?;
            self.0.attest_derived_key(salt)
        }
    }
    let root = tempfile::tempdir().unwrap();
    let host = root.path().join("host");
    let guest = root.path().join("guest");
    std::fs::create_dir_all(&host).unwrap();
    std::fs::create_dir_all(&guest).unwrap();
    let host_identity = NoProjectionKeys(InMemoryProvider::from_seed([0xe1; 32]));
    let guest_identity = RootIdentity::from_seed([0xe2; 32]);
    let settings = PlaceWorkerSettings::default();
    let binding = found_place(&host, &host_identity, "Graph only", &settings).unwrap();
    let now_ms = settings.authority_clock.now_ms();
    let guest_root = guest_identity.master_public_key().to_bytes();
    let prekey = prepare_group_identity(&guest, &guest_identity, binding.moot.0).unwrap();
    let (mut open, _) = open_cached_place(&host, &binding, &host_identity, &settings).unwrap();

    let refused = admit_member(
        &open.moot,
        &binding,
        &host_identity,
        guest_root,
        Writer,
        now_ms,
        None,
        true,
    )
    .err()
    .expect("an offered projection still needs its signing authority");
    assert!(refused.contains("issue projection grant"), "{refused}");
    assert_eq!(
        pollster::block_on(open.moot.snapshot())
            .unwrap()
            .membership
            .members
            .len(),
        1,
        "projection refusal must precede membership mutation"
    );

    let authored = admit_member(
        &open.moot,
        &binding,
        &host_identity,
        guest_root,
        Writer,
        now_ms,
        None,
        false,
    )
    .unwrap();
    assert!(authored.projection.is_none());
    let (invite, _) = author_invitation_with(
        &open.moot,
        &mut open.group,
        &host,
        &binding,
        &host_identity,
        &prekey,
        now_ms + 60_000,
        Vec::new(),
        authored.projection,
    )
    .unwrap();
    assert!(invite.projection_grant.is_none());
    let admitted = admit_invitation(&guest, &invite, &guest_identity, &settings).unwrap();
    assert_eq!(admitted.moot.members, 2);
    let (guest_open, snapshot) =
        open_cached_place(&guest, &binding, &guest_identity, &settings).unwrap();
    assert!(snapshot.permissions.as_ref().unwrap().graph_write);
    assert!(snapshot.permissions.as_ref().unwrap().message_write);
    drop(guest_open);

    revoke_member(&mut open, &host_identity, guest_root, now_ms + 1).unwrap();
    let snapshot = pollster::block_on(open.moot.snapshot()).unwrap();
    assert_eq!(snapshot.membership.members.len(), 1);
}

/// A reader's invitation: membership without delegation.
///
/// The two halves are separate questions and this proves they stay
/// separate. Reading is answered by the welcome and the Moot's fold, so
/// the founder's shared node reaches the reader's projection; authoring
/// is answered by a capability nobody issued, so the reader's own worker
/// refuses it before a store or a lane sees anything.
#[test]
fn a_reader_invitation_admits_a_member_that_cannot_author() {
    let root =
        std::env::temp_dir().join(format!("turnstone-place-reader-{}", uuid::Uuid::new_v4()));
    let host = root.join("host");
    let guest = root.join("guest");
    std::fs::create_dir_all(&host).unwrap();
    std::fs::create_dir_all(&guest).unwrap();
    let host_identity = RootIdentity::Local(InMemoryProvider::from_seed([0xd3; 32]));
    let reader_identity = RootIdentity::Local(InMemoryProvider::from_seed([0xd4; 32]));
    let settings = PlaceWorkerSettings::default();
    let now_ms = settings.authority_clock.now_ms();

    let binding = found_place(&host, &host_identity, "Hearth", &settings).unwrap();

    // Authored BEFORE the invitation, so what the reader reads is content
    // that existed before it was anybody here.
    const SHARED: &str = "https://shared.example/page";
    let (mut host_open, _) = open_cached_place(&host, &binding, &host_identity, &settings).unwrap();
    author_into_place(
        &mut host_open,
        &binding,
        &host_identity,
        &PlaceCommand::ShareNode {
            address: SHARED.into(),
        },
        &settings,
    )
    .unwrap();
    let host_snapshot = place_snapshot(&host_open, binding.moot.0, &settings).unwrap();
    assert_eq!(host_snapshot.graph.nodes, 1);
    drop(host_open);

    let prekey = prepare_group_identity(&guest, &reader_identity, binding.moot.0).unwrap();
    let reader_root = reader_identity.master_public_key().to_bytes();
    let moot = pollster::block_on(MootFile::open_existing(
        place_store_dir(&host).join("gemot"),
        MootId(binding.moot.0),
        settings.retention.clone(),
    ))
    .unwrap();
    admit_member(
        &moot,
        &binding,
        &host_identity,
        reader_root,
        Reader,
        now_ms,
        None,
        true,
    )
    .unwrap();
    // Idempotent for a reader too: no second membership fact, no
    // delegation sneaking in on the way through.
    let projection_grant = admit_member(
        &moot,
        &binding,
        &host_identity,
        reader_root,
        Reader,
        now_ms,
        None,
        true,
    )
    .unwrap()
    .projection;
    assert!(
        projection_grant.is_none(),
        "a reader is admitted to the Moot and to no projection door"
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

    let admitted = admit_invitation(&guest, &invite, &reader_identity, &settings).unwrap();
    assert_eq!(admitted.moot.members, 2);
    assert_eq!(admitted.group_members, 2);

    // The graph lane's delivery, offline: the reader ends up holding the
    // operations the founder authored, exactly as a sync round would
    // leave it. Everything proved below is a verdict on those operations.
    std::fs::copy(
        place_store_dir(&host).join(GRAPH_STORE),
        place_store_dir(&guest).join(GRAPH_STORE),
    )
    .unwrap();

    let (mut reader_open, reader_snapshot) =
        open_cached_place(&guest, &binding, &reader_identity, &settings).unwrap();
    assert_eq!(
        reader_snapshot.permissions,
        Some(crate::place::PlacePermissionSnapshot {
            message_write: false,
            graph_write: false,
        }),
        "a reader holds neither write capability"
    );
    assert_eq!(reader_snapshot.personae_root, reader_root);
    // Reading needs the welcome, not a delegation: the founder's node is
    // here, and both peers digest it the same way.
    assert_eq!(reader_snapshot.graph.nodes, 1);
    assert_eq!(
        reader_snapshot.shared.addresses().collect::<Vec<_>>(),
        vec![SHARED]
    );
    assert_eq!(reader_snapshot.graph_digest, host_snapshot.graph_digest);
    assert_eq!(reader_snapshot.graph.pending_authority, 0);

    // Its own worker refuses both authoring paths, with the reason the
    // product surfaces as a place refusal.
    for command in [
        PlaceCommand::ShareNode {
            address: "https://reader.example/page".into(),
        },
        PlaceCommand::SendMessage {
            channel: binding.default_channel.clone(),
            body: "refused".into(),
        },
    ] {
        let refused = author_into_place(
            &mut reader_open,
            &binding,
            &reader_identity,
            &command,
            &settings,
        )
        .unwrap_err();
        assert!(refused.contains("no effective capability"), "{refused}");
    }
    // The refusal authored nothing: the shared graph is still the
    // founder's one node.
    let after = place_snapshot(&reader_open, binding.moot.0, &settings).unwrap();
    assert_eq!(after.graph.nodes, 1);
    assert_eq!(after.chat.messages, 0);
    drop(reader_open);
    let _ = std::fs::remove_dir_all(&root);
}

/// T1 without lanes: revocation removes the member, revokes the
/// delegation admission issued, and the door ledger rebuilt from the Gemot
/// fold withdraws exactly the projection grant that admission issued.
#[test]
fn revoking_a_writer_withdraws_every_grant_its_admission_issued() {
    let root = tempfile::tempdir().unwrap();
    let host = root.path().join("host");
    std::fs::create_dir_all(&host).unwrap();
    let host_identity = RootIdentity::Local(InMemoryProvider::from_seed([0xd7; 32]));
    let writer = InMemoryProvider::from_seed([0xd8; 32]);
    let host_root = host_identity.master_public_key().to_bytes();
    let writer_root = writer.master_public_key().to_bytes();
    let reader_root = InMemoryProvider::from_seed([0xd9; 32])
        .master_public_key()
        .to_bytes();
    let settings = settings();
    let binding = found_place(&host, &host_identity, "Hearth", &settings).unwrap();
    let (mut open, _) = open_cached_place(&host, &binding, &host_identity, &settings).unwrap();

    let expires = Some(AUTHORITY_AT_MS + 60_000);
    let grant = admit_member(
        &open.moot,
        &binding,
        &host_identity,
        writer_root,
        Writer,
        AUTHORITY_AT_MS,
        expires,
        true,
    )
    .unwrap()
    .projection
    .expect("a writer is issued a projection grant");
    assert_eq!(
        grant.certificate.expires_at_ms, expires,
        "the lifetime bounds the grant"
    );
    admit_member(
        &open.moot,
        &binding,
        &host_identity,
        reader_root,
        Reader,
        AUTHORITY_AT_MS,
        None,
        true,
    )
    .unwrap();
    // The group half of each admission. Gemot membership alone leaves the
    // group session unable to resolve the root, and a revoke it cannot
    // resolve is now refused rather than half-applied.
    let mut welcome = |open: &mut OpenPlace, seed: u8| {
        let directory = root.path().join(format!("member-{seed:02x}"));
        std::fs::create_dir_all(&directory).unwrap();
        let prekey = prepare_group_identity(
            &directory,
            &InMemoryProvider::from_seed([seed; 32]),
            binding.moot.0,
        )
        .unwrap();
        author_invitation_with(
            &open.moot,
            &mut open.group,
            &host,
            &binding,
            &host_identity,
            &prekey,
            AUTHORITY_AT_MS + 60_000,
            Vec::new(),
            None,
        )
        .unwrap();
    };
    welcome(&mut open, 0xd8);
    welcome(&mut open, 0xd9);
    let before = place_snapshot(&open, binding.moot.0, &settings).unwrap();
    assert_eq!(before.members.len(), 3);
    let rules = pollster::block_on(open.moot.snapshot())
        .unwrap()
        .governance
        .rules;
    let issued = |open: &OpenPlace| {
        pollster::block_on(open.moot.delegations())
            .unwrap()
            .projections(binding.moot.0, &rules, AUTHORITY_AT_MS)
            .into_iter()
            .filter(|grant| grant.subject == writer_root)
            .collect::<Vec<_>>()
    };
    assert_eq!(issued(&open).len(), 1);
    assert_eq!(
        issued(&open)[0].expires_at_ms,
        expires,
        "and the delegation"
    );

    // Refusals name their reason and author nothing.
    let refused = revoke_member(&mut open, &writer, reader_root, AUTHORITY_AT_MS).unwrap_err();
    assert_eq!(
        refused,
        "only a profile that manages this place can revoke a member"
    );
    for (member, reason) in [
        (host_root, "a manager cannot revoke itself"),
        ([0x77; 32], "that root is not a member of this place"),
    ] {
        let command = PlaceCommand::RevokeMember { member };
        let refused = author_into_place(&mut open, &binding, &host_identity, &command, &settings)
            .unwrap_err();
        assert_eq!(refused, reason);
    }
    assert!(
        withdrawn_projection_grants(&open, &host_identity)
            .unwrap()
            .is_empty()
    );

    let command = PlaceCommand::RevokeMember {
        member: writer_root,
    };
    author_into_place(&mut open, &binding, &host_identity, &command, &settings).unwrap();
    let after = place_snapshot(&open, binding.moot.0, &settings).unwrap();
    assert!(
        !after
            .members
            .iter()
            .any(|member| member.root == writer_root)
    );
    assert!(
        issued(&open)
            .iter()
            .all(|grant| grant.directly_revoked && !grant.active)
    );
    let withdrawn = withdrawn_projection_grants(&open, &host_identity).unwrap();
    assert_eq!(withdrawn.len(), 1);
    assert_eq!(withdrawn[0].revocation.certificate, grant.certificate.id());
    let mut ledger = notochord::RevocationLedger::new();
    ledger.fold(withdrawn[0].check().expect("the issued revocation checks"));
    assert!(
        ledger.revokes(&grant.certificate),
        "the door refuses the issued grant"
    );

    // A reader was issued no grant: its revocation is membership alone.
    let command = PlaceCommand::RevokeMember {
        member: reader_root,
    };
    author_into_place(&mut open, &binding, &host_identity, &command, &settings).unwrap();
    let last = place_snapshot(&open, binding.moot.0, &settings).unwrap();
    assert_eq!(last.members.len(), 1);
    assert_eq!(
        withdrawn_projection_grants(&open, &host_identity)
            .unwrap()
            .len(),
        1
    );
    // The founder's own standing is untouched.
    assert_eq!(last.standing, crate::place::PlaceStanding::Member);
    assert_eq!(
        last.permissions,
        Some(crate::place::PlacePermissionSnapshot {
            message_write: true,
            graph_write: true,
        })
    );
}
