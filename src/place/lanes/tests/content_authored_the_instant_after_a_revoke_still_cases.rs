// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// R2b's race: the founder revokes B and authors immediately, before A can
/// have drained the rotation. What A receives ahead of the epoch it is
/// sealed to parks rather than being refused, and A's next drain re-admits
/// it, so A reads both the message and the shared node without
/// reconnecting -- and F's LATER message, whose predecessor was the parked
/// one, arrives too. B, which read the pre-revocation pair, reads neither.
#[test]
fn content_authored_the_instant_after_a_revoke_still_reaches_the_remaining_member() {
    use crate::place::PlaceStanding;
    let root =
        std::env::temp_dir().join(format!("turnstone-place-r2b-race-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let [host, a_dir, b_dir] = ["host", "a", "b"].map(|name| root.join(name));
    for directory in [&host, &a_dir, &b_dir] {
        std::fs::create_dir_all(directory).unwrap();
    }
    const F: u8 = 0xe8;
    const A: u8 = 0xe9;
    const B: u8 = 0xea;
    const BEFORE: &str = "https://before.example/race";
    const AFTER: &str = "https://after.example/race";
    let root_of = |seed: u8| {
        InMemoryProvider::from_seed([seed; 32])
            .master_public_key()
            .to_bytes()
    };

    let (founder, f_session, binding) = found_live(&host, F);
    let moot = binding.moot.0;
    let channel = binding.default_channel.clone();
    let invite = invite_writer(&founder, f_session, &host, &a_dir, A, moot);
    let (a_worker, a_updates, a_session) = join_member(&a_dir, A, invite);
    let invite = invite_writer(&founder, f_session, &host, &b_dir, B, moot);
    let (b_worker, b_updates, b_session) = join_member(&b_dir, B, invite);

    // Positive control: both members read a message and a node authored
    // before the revocation.
    send_message(
        &founder.0,
        &founder.1,
        f_session,
        1,
        &channel,
        "before revocation",
    );
    share_node(&founder, f_session, 2, BEFORE);
    let pair = |s: &crate::place::OfflinePlaceSnapshot| s.chat.messages == 1 && s.graph.nodes == 1;
    converge_until(
        &a_worker,
        &a_updates,
        a_session,
        "A reads the first pair",
        pair,
    );
    let b_before = converge_until(
        &b_worker,
        &b_updates,
        b_session,
        "B reads the first pair",
        pair,
    );
    assert_eq!(addresses(&b_before), [BEFORE]);

    // The race. Nothing waits for A's drain between these three commands.
    founder.0.command(PlaceWorkerCommand::Author {
        session: f_session,
        generation: 1,
        request: 3,
        command: PlaceCommand::RevokeMember { member: root_of(B) },
    });
    let revoked = authored(&founder.1, 3).expect("the founder revokes B");
    let revoked_at = Instant::now();
    send_message(
        &founder.0,
        &founder.1,
        f_session,
        4,
        &channel,
        "after revocation",
    );
    share_node(&founder, f_session, 5, AFTER);
    assert_eq!(revoked.group.members, 2);

    let message_at = std::cell::Cell::new(None);
    let node_at = std::cell::Cell::new(None);
    let (a_read, nudges) = until_nudged(
        &a_worker,
        &a_updates,
        a_session,
        Duration::from_secs(60),
        |s| {
            if message_at.get().is_none() && s.chat.messages == 2 {
                message_at.set(Some(revoked_at.elapsed()));
            }
            if node_at.get().is_none() && s.graph.nodes == 2 {
                node_at.set(Some(revoked_at.elapsed()));
            }
            message_at.get().is_some() && node_at.get().is_some()
        },
    )
    .unwrap_or_else(|| panic!("A never read the post-revoke pair within 60 s"));
    eprintln!(
        "R2b race: A read the message {} ms and the node {} ms after the revoke answered, over {nudges} nudge(s); re-admitted {}, parked {}, evicted {}, frames pending {} refused {}",
        message_at.get().unwrap().as_millis(),
        node_at.get().unwrap().as_millis(),
        a_read.group.readmitted_records,
        a_read.group.parked_records,
        a_read.group.evicted_records,
        a_read.group.pending_frames,
        a_read.group.refused_frames,
    );
    assert_eq!(addresses(&a_read), [AFTER, BEFORE]);
    assert_eq!(a_read.group.parked_records, 0, "A left a record parked");
    assert_eq!(a_read.group.refused_frames, 0);

    // The successor case: F's next message backlinks the one that may have
    // parked. Before parking landed, that predecessor was dropped and this
    // message could never be admitted.
    send_message(&founder.0, &founder.1, f_session, 6, &channel, "successor");
    let successor_at = Instant::now();
    let (a_successor, _) = until_nudged(
        &a_worker,
        &a_updates,
        a_session,
        Duration::from_secs(60),
        |s| s.chat.messages == 3,
    )
    .unwrap_or_else(|| panic!("A never read F's successor message within 60 s"));
    eprintln!(
        "R2b race: A read the successor {} ms after it was authored",
        successor_at.elapsed().as_millis()
    );
    assert_eq!(a_successor.group.parked_records, 0);

    // B stayed connected across the whole window -- it saw its own
    // revocation, a plaintext Gemot fact -- and read none of the three
    // encrypted records authored after it.
    let b_after = converge_until(
        &b_worker,
        &b_updates,
        b_session,
        "B sees its revocation",
        |s| s.standing == PlaceStanding::MembershipRevoked,
    );
    eprintln!(
        "R2b race: B after the revoke: messages {}, nodes {}, parked {}, epochs {} (before {})",
        b_after.chat.messages,
        b_after.graph.nodes,
        b_after.group.parked_records,
        b_after.group.epochs,
        b_before.group.epochs,
    );
    assert_eq!(b_after.chat.messages, 1, "B read post-revocation chat");
    assert_eq!(
        addresses(&b_after),
        [BEFORE],
        "B read a post-revocation node"
    );
    assert_eq!(b_after.group.epochs, b_before.group.epochs);

    for worker in [&founder.0, &a_worker, &b_worker] {
        release(worker);
    }
    let a_open = reopen_by_hand(&a_dir, &binding, &InMemoryProvider::from_seed([A; 32]));
    let b_open = reopen_by_hand(&b_dir, &binding, &InMemoryProvider::from_seed([B; 32]));
    let a_bodies = chat_bodies(&a_open);
    let b_bodies = chat_bodies(&b_open);
    eprintln!("R2b race retained bodies: A {a_bodies:?}; B {b_bodies:?}");
    assert!(
        b_bodies.iter().any(|body| body == "before revocation"),
        "positive control"
    );
    for body in ["after revocation", "successor"] {
        assert!(a_bodies.iter().any(|held| held == body), "A lacks {body}");
        assert!(!b_bodies.iter().any(|held| held == body), "B read {body}");
    }
    drop((a_open, b_open));
    let _ = std::fs::remove_dir_all(&root);
}

/// K4 of the co-op lifecycle parity plan: Turnstone's duplicate-replay
/// behaviour is true by construction (LogSync's own accept path
/// deduplicates) but was never receipted directly. A founder and one
/// live writer converge on one chat message, one shared node, and (via
/// the product `Invite` of a third root that never joins) one
/// membership add and one group-key add frame. The writer's converged
/// counts and lane counters are recorded, then each fact is handed back
/// to the writer's own store a second time -- offline, the way R2/E2
/// hand a replica an operation directly -- and once more through a
/// fresh reconnect. Nothing should move.
#[test]
fn a_replayed_operation_adds_no_place_state() {
    let root = std::env::temp_dir().join(format!("turnstone-place-replay-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let [host, w_dir, third_dir] = ["host", "writer", "third"].map(|name| root.join(name));
    for directory in [&host, &w_dir, &third_dir] {
        std::fs::create_dir_all(directory).unwrap();
    }
    const F: u8 = 0xf1;
    const W: u8 = 0xf2;
    const T: u8 = 0xf3;
    const BODY: &str = "replay probe";
    const ADDRESS: &str = "https://replay.example/node";

    let (founder, f_session, binding) = found_live(&host, F);
    let moot = binding.moot.0;
    let channel = binding.default_channel.clone();
    let invite = invite_writer(&founder, f_session, &host, &w_dir, W, moot);
    let (w_worker, w_updates, w_session) = join_member(&w_dir, W, invite);
    converge_until(
        &w_worker,
        &w_updates,
        w_session,
        "the writer's initial catch-up",
        |s| s.moot.members == 2,
    );

    // One chat message, one shared node, and -- through the product
    // `Invite` of a third root, which need not join -- one membership
    // add and one group-key add frame (every invite authors one,
    // whatever the access level).
    send_message(&founder.0, &founder.1, f_session, 1, &channel, BODY);
    share_node(&founder, f_session, 2, ADDRESS);
    let third_prekey =
        prepare_group_identity(&third_dir, &InMemoryProvider::from_seed([T; 32]), moot).unwrap();
    founder.0.command(PlaceWorkerCommand::Invite {
        session: f_session,
        generation: 1,
        directory: host.clone(),
        prekey: third_prekey,
        access: crate::place::PlaceInviteAccess::Reader,
        lifetime_ms: None,
    });
    expect_invited(&founder.1, "the third root's invitation");

    let converged = converge_until(
        &w_worker,
        &w_updates,
        w_session,
        "the writer converges on every fact",
        |s| s.chat.messages == 1 && s.graph.nodes == 1 && s.moot.members == 3,
    );
    report_lanes("writer converged, before any replay", &converged);
    let before_counters = lane_ops(&converged);
    let group_epochs_before = converged.group.epochs;
    eprintln!(
        "K4 before replay: messages {} nodes {} members {} group epochs {}; lanes {before_counters:?}",
        converged.chat.messages, converged.graph.nodes, converged.moot.members, group_epochs_before,
    );

    // Release the writer's own worker and reopen its stores by hand --
    // the same bypass R2/E2 use to hand a replica an operation
    // directly -- then feed each fact straight back into its own
    // accept.
    release(&w_worker);
    let identity = InMemoryProvider::from_seed([W; 32]);
    let open = reopen_by_hand(&w_dir, &binding, &identity);

    let chat_hit = pollster::block_on(open.chat.projection())
        .unwrap()
        .messages
        .into_iter()
        .find(|message| message.message.body == BODY)
        .expect("the writer holds its own converged message");
    let chat_op = pollster::block_on(
        open.chat
            .sync_store()
            .get_operation(&chat_hit.operation.into()),
    )
    .unwrap()
    .expect("the writer retains the message operation");
    let chat_accepted = pollster::block_on(open.chat.accept(&chat_op)).unwrap();
    let chat_parked = pollster::block_on(open.chat.parking_status()).unwrap();
    eprintln!(
        "K4 chat replay: accept() -> inserted {chat_accepted}, parked afterwards {}",
        chat_parked.parked
    );
    assert!(
        !chat_accepted,
        "a replayed chat message was accepted as new"
    );
    assert_eq!(
        chat_parked.parked, 0,
        "the replayed message was parked, not recognized as stored"
    );

    // Every retained graph operation, via the same authority override
    // R2/E2 use to name operations without already knowing their ids.
    let graph_ops = pollster::block_on(open.graph.projection_with_authority(&EveryOperation))
        .unwrap()
        .revoked;
    assert_eq!(
        graph_ops.len(),
        1,
        "expected exactly the one shared node authored above"
    );
    let graph_op = pollster::block_on(
        open.graph
            .sync_store()
            .get_operation(&graph_ops[0].operation.into()),
    )
    .unwrap()
    .expect("the writer retains the node operation");
    let graph_accepted = pollster::block_on(open.graph.accept(&graph_op)).unwrap();
    let graph_parked = pollster::block_on(open.graph.parking_status()).unwrap();
    eprintln!(
        "K4 graph replay: accept() -> inserted {graph_accepted}, parked afterwards {}",
        graph_parked.parked
    );
    assert!(
        !graph_accepted,
        "a replayed shared node was accepted as new"
    );
    assert_eq!(
        graph_parked.parked, 0,
        "the replayed node was parked, not recognized as stored"
    );

    // Membership: nothing here raced (one founder, serialized), so the
    // auth DAG is linear and its single head is exactly the third
    // root's admission operation.
    let membership = pollster::block_on(open.moot.snapshot()).unwrap().membership;
    assert_eq!(
        membership.auth_heads.len(),
        1,
        "membership auth DAG forked; the head-is-the-op shortcut does not hold here"
    );
    let membership_op = pollster::block_on(
        open.moot
            .membership_store()
            .get(&membership.auth_heads[0].into()),
    )
    .unwrap()
    .expect("the writer retains the third root's admission operation");
    let membership_accepted =
        pollster::block_on(open.moot.membership_store().accept(&membership_op)).unwrap();
    eprintln!("K4 membership replay: accept() -> inserted {membership_accepted}");
    assert!(
        !membership_accepted,
        "a replayed membership admission was accepted as new"
    );

    // Group-key: NOT exercised here. Reaching the applied add frame's
    // own Operation<GroupKeyExt> needs p2panda-core's Topic/VerifyingKey
    // and p2panda-store's TopicStore to enumerate the lane by author --
    // neither crate is a direct dependency of turnstone, and no product
    // accessor surfaces an applied frame's operation id. Its
    // duplicate-safety is instead covered below, through a fresh
    // reconnect that re-runs the same LogSync accept path this lane
    // shares with the other three.
    eprintln!(
        "K4 group-key replay: direct accept() not reachable without p2panda-core/-store as \
             turnstone dependencies; covered by the reconnect leg below instead"
    );

    drop(open);

    // Once more via a fresh sync round: a whole new LiveLanes, dialing
    // in and reconciling from scratch rather than being nudged.
    let (w_worker2, w_updates2) = spawn_place_worker(
        Arc::new(|| {}),
        Arc::new(RootIdentity::from_seed([W; 32])),
        settings(),
    );
    w_worker2.command(PlaceWorkerCommand::Reconnect {
        contact: None,
        session: w_session,
        generation: 1,
        directory: w_dir.clone(),
        binding: binding.clone(),
    });
    let caught_up = converge_until(
        &w_worker2,
        &w_updates2,
        w_session,
        "the writer's post-reconnect fold",
        |s| s.chat.messages == 1 && s.graph.nodes == 1 && s.moot.members == 3,
    );
    report_lanes("writer just after reconnecting", &caught_up);
    assert_eq!(
        caught_up.chat.messages, converged.chat.messages,
        "reconnect changed the message count"
    );
    assert_eq!(
        caught_up.graph.nodes, converged.graph.nodes,
        "reconnect changed the node count"
    );
    assert_eq!(
        caught_up.moot.members, converged.moot.members,
        "reconnect changed the member count"
    );
    assert_eq!(
        caught_up.group.epochs, group_epochs_before,
        "reconnect changed the group epoch count"
    );
    let caught_up_counters = lane_ops(&caught_up);

    // A second, deliberately redundant round on the SAME connection:
    // everything it could receive, it already holds.
    std::thread::sleep(Duration::from_millis(500));
    w_worker2.command(PlaceWorkerCommand::Resync {
        session: w_session,
        generation: 1,
    });
    let after_extra = loop {
        match w_updates2.recv_timeout(Duration::from_secs(10)) {
            Ok(Update::PlaceOpened {
                result: Ok(snapshot),
                ..
            }) => break snapshot,
            Ok(_) => continue,
            Err(error) => panic!("the redundant resync never answered: {error}"),
        }
    };
    report_lanes("writer after one redundant round", &after_extra);
    let after_extra_counters = lane_ops(&after_extra);
    eprintln!(
        "K4 after replay: messages {} nodes {} members {} group epochs {}; lanes {after_extra_counters:?}",
        after_extra.chat.messages,
        after_extra.graph.nodes,
        after_extra.moot.members,
        after_extra.group.epochs,
    );
    assert_eq!(
        after_extra.chat.messages, converged.chat.messages,
        "the redundant round changed the message count"
    );
    assert_eq!(
        after_extra.graph.nodes, converged.graph.nodes,
        "the redundant round changed the node count"
    );
    assert_eq!(
        after_extra.moot.members, converged.moot.members,
        "the redundant round changed the member count"
    );
    assert_eq!(
        after_extra.group.epochs, group_epochs_before,
        "the redundant round changed the group epoch count"
    );
    for (name, before_count) in &caught_up_counters {
        let after_count = after_extra_counters
            .iter()
            .find(|(other, _)| other == name)
            .map(|(_, count)| *count)
            .unwrap_or_else(|| panic!("{name} is one of the ten"));
        assert!(
            after_count <= *before_count,
            "{name}: ops_received grew from {before_count} to {after_count} on a redundant round"
        );
    }

    release(&founder.0);
    release(&w_worker2);
    let _ = std::fs::remove_dir_all(&root);
}

/// K3: the place worker walks `moot_port::coop`'s shared conformance
/// harness, declaring membership and grant as separate facts and
/// reading cut after a revoke's rotation. `receipts_duplicate_replay`
/// is `true` because K4 (`a_replayed_operation_adds_no_place_state`)
/// landed; the store-free reference profile coop.rs's own tests use for
/// illustration declares `false`, which is not Turnstone's declaration.
/// The clock is `System`: Turnstone's own is the wall clock in
/// product, and driving it for real (a short-lived grant, `expire`
/// waiting it out) is simpler here than hand-advancing a fixed one
/// through a worker with no such command.
///
/// Two gaps were found and closed on the way to a full walk. `leave`
/// used to stop the walk: `conform`'s `leave` step requires
/// `Verdict::Left`, and `PlaceState` had no case for "this session left
/// a place, retaining its history" distinct from "never joined one".
/// Mark's call was to add `PlaceState::Left` (Turnstone-only;
/// `coop::conform` is unchanged). `rejoin` then stopped it one step
/// later: a local leave used to delete the local place-binding sidecar,
/// which both `Reconnect` and a fresh invitation need (the founder's own
/// `group.add` also refuses a recipient "already present", so re-invite
/// was never going to work either). Mark's second call was to keep the
/// binding, the rendezvous hints and every retained store in place on
/// leave, marking "left" beside them instead of deleting anything, and
/// to add `Rejoin place` as the real way back in. This driver's `leave`
/// and `rejoin` now do the same.
#[test]
fn the_place_worker_conforms_to_the_coop_lifecycle_contract() {
    let root = std::env::temp_dir().join(format!(
        "turnstone-place-coop-contract-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mut driver = ConformanceDriver::open(&root);
    let capabilities = coop::Capabilities {
        grant_is_all_authority: false,
        cuts_reading_on_revoke: true,
        receipts_duplicate_replay: true,
        clock: coop::Clock::System,
    };
    let outcome = coop::conform(&mut driver, &capabilities);
    drop(driver);
    let _ = std::fs::remove_dir_all(&root);
    let conformance = outcome.unwrap_or_else(|failure| panic!("{failure}"));

    if std::env::var("TURNSTONE_WRITE_RECEIPTS").as_deref() == Ok("1") {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("design_docs/receipts/coop_contract_conformance.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&conformance).unwrap())
            .unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
    }

    for step in [
        coop::Step::Fresh,
        coop::Step::InviteWrongTarget,
        coop::Step::Join,
        coop::Step::Leave,
        coop::Step::Rejoin,
        coop::Step::Reconnect,
        coop::Step::Expire,
        coop::Step::ReconnectAfterExpiry,
        coop::Step::Revoke,
        coop::Step::ReplayDuplicate,
    ] {
        assert!(
            conformance.at(step).is_some(),
            "{}: the walk did not reach this step",
            step.name()
        );
    }
    assert_eq!(
        conformance
            .at(coop::Step::Leave)
            .map(|report| report.verdict),
        Some(coop::Verdict::Left)
    );
    assert_eq!(
        conformance
            .at(coop::Step::Rejoin)
            .map(|report| report.verdict),
        Some(coop::Verdict::Joined)
    );
    assert!(
        conformance
            .at(coop::Step::Expire)
            .and_then(|report| report.grant)
            .and_then(|grant| grant.expired_at_ms)
            .is_some(),
        "expired must carry when the grant ended"
    );
    assert_eq!(
        conformance
            .at(coop::Step::Revoke)
            .map(|report| report.reading),
        Some(coop::Reading::Cut),
        "revocation must cut reading, per Turnstone's declared capability"
    );
}
