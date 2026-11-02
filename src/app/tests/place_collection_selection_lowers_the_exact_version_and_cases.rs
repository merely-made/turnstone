// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn place_collection_selection_lowers_the_exact_version_and_clear() {
    let mut app = App::test_stub();
    assert_eq!(
        app.update(Action::SetPlaceCollection(None)),
        vec![Effect::Redraw]
    );
    assert!(matches!(
        app.take_events().last(),
        Some(crate::observe::AppEvent::PlaceRefused(reason))
            if reason == "this session is not in a place"
    ));

    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0x31; 32]),
        crate::place::SharedContainerId([0x32; 32]),
        crate::place::ChatSpaceId([0x33; 32]),
        "hall",
    )
    .unwrap();
    app.place = crate::place::PlaceState::Offline {
        binding,
        generation: 7,
        snapshot: crate::place::OfflinePlaceSnapshot::default(),
    };
    let requested = crate::place::PlaceCollectionVersion {
        moot: crate::place::PlaceId([0x31; 32]),
        collection: crate::place::PlaceCollectionId([0x41; 32]),
        frontier: vec![[0x51; 32], [0x52; 32]],
        membership_commitment: [0x61; 32],
    };
    assert_eq!(
        app.update(Action::SetPlaceCollection(Some(requested.clone()))),
        vec![Effect::SetPlaceCollection {
            session: app.session_id,
            generation: 7,
            selection: Some(requested),
        }]
    );
    assert_eq!(
        app.update(Action::SetPlaceCollection(None)),
        vec![Effect::SetPlaceCollection {
            session: app.session_id,
            generation: 7,
            selection: None,
        }]
    );
}

#[test]
fn leave_place_is_correlated_and_failure_keeps_a_retryable_degraded_binding() {
    let mut app = App::test_stub();
    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0x81; 32]),
        crate::place::SharedContainerId([0x82; 32]),
        crate::place::ChatSpaceId([0x83; 32]),
        "hall",
    )
    .unwrap();
    app.place = crate::place::PlaceState::Offline {
        binding,
        generation: 12,
        snapshot: crate::place::OfflinePlaceSnapshot::default(),
    };
    assert_eq!(
        app.update(Action::LeavePlace),
        vec![Effect::LeavePlace {
            session: app.session_id,
            generation: 12,
            retained: crate::place::PlaceLeftSummary::default(),
        }]
    );
    assert!(matches!(
        app.place,
        crate::place::PlaceState::Offline { .. }
    ));

    let retained = crate::place::PlaceLeftSummary::default();
    assert!(
        app.finish_leave_place(app.session_id, 11, retained, Ok(true))
            .is_empty()
    );
    assert!(matches!(
        app.place,
        crate::place::PlaceState::Offline { .. }
    ));

    assert_eq!(
        app.finish_leave_place(app.session_id, 12, retained, Err("release timeout".into())),
        vec![Effect::Redraw]
    );
    let retry_generation = match &app.place {
        crate::place::PlaceState::Degraded { generation, .. } => *generation,
        other => panic!("failed leave must remain retryable and visible: {other:?}"),
    };
    assert_ne!(retry_generation, 12);
    assert!(
        app.finish_leave_place(app.session_id, 12, retained, Ok(true))
            .is_empty()
    );
    assert_eq!(app.place.generation(), Some(retry_generation));

    let retryable_binding = app
        .place
        .binding()
        .expect("Degraded still names it")
        .clone();
    assert_eq!(
        app.finish_leave_place(app.session_id, retry_generation, retained, Ok(true)),
        vec![Effect::Redraw]
    );
    // The retry's leave completed from Degraded, which carries no snapshot,
    // so the retained summary reads zero rather than a guess.
    assert_eq!(
        app.place,
        crate::place::PlaceState::Left {
            binding: retryable_binding,
            retained: crate::place::PlaceLeftSummary::default(),
        }
    );
}

#[test]
fn failed_leave_from_joining_cannot_adopt_a_late_join_answer() {
    let mut app = App::test_stub();
    app.place = crate::place::PlaceState::Joining { generation: 4 };
    let effects = app.update(Action::LeavePlace);
    let retained = crate::place::PlaceLeftSummary::default();
    assert_eq!(
        effects,
        vec![Effect::LeavePlace {
            session: app.session_id,
            generation: 4,
            retained,
        }]
    );
    app.finish_leave_place(app.session_id, 4, retained, Err("release timeout".into()));
    assert!(matches!(app.place, crate::place::PlaceState::Failed { .. }));
    assert!(
        app.finish_leave_place(app.session_id, 4, retained, Ok(true))
            .is_empty()
    );
}

#[test]
fn place_collection_completion_is_correlated_and_failure_keeps_the_snapshot() {
    let mut app = App::test_stub();
    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0x71; 32]),
        crate::place::SharedContainerId([0x72; 32]),
        crate::place::ChatSpaceId([0x73; 32]),
        "hall",
    )
    .unwrap();
    let prior = crate::place::OfflinePlaceSnapshot {
        graph: crate::place::GraphCache {
            nodes: 2,
            ..Default::default()
        },
        ..Default::default()
    };
    app.place = crate::place::PlaceState::Offline {
        binding: binding.clone(),
        generation: 9,
        snapshot: prior.clone(),
    };

    assert!(
        app.apply_update(Update::PlaceCollectionSet {
            session: app.session_id,
            generation: 8,
            result: Ok(crate::place::OfflinePlaceSnapshot::default()),
        })
        .is_empty()
    );
    assert_eq!(
        app.place,
        crate::place::PlaceState::Offline {
            binding: binding.clone(),
            generation: 9,
            snapshot: prior.clone(),
        }
    );

    assert_eq!(
        app.apply_update(Update::PlaceCollectionSet {
            session: app.session_id,
            generation: 9,
            result: Err("collection storage unavailable".into()),
        }),
        vec![Effect::Redraw]
    );
    assert_eq!(
        app.place,
        crate::place::PlaceState::Offline {
            binding: binding.clone(),
            generation: 9,
            snapshot: prior,
        }
    );
    assert!(matches!(
        app.take_events().last(),
        Some(crate::observe::AppEvent::PlaceRefused(reason))
            if reason == "collection storage unavailable"
    ));

    let reminted = crate::place::OfflinePlaceSnapshot {
        captured_selection: crate::place::CapturedCollectionSelection::Collection {
            requested: crate::place::PlaceCollectionVersion {
                moot: crate::place::PlaceId([0x71; 32]),
                collection: crate::place::PlaceCollectionId([0x81; 32]),
                frontier: vec![[0x91; 32]],
                membership_commitment: [0xa1; 32],
            },
            status: crate::place::CapturedCollectionSelectionStatus::Ready {
                name: "field notes".into(),
                effective_contributions: 1,
                pending_facts: 0,
            },
        },
        ..Default::default()
    };
    assert_eq!(
        app.apply_update(Update::PlaceCollectionSet {
            session: app.session_id,
            generation: 9,
            result: Ok(reminted.clone()),
        }),
        vec![Effect::Redraw]
    );
    assert_eq!(
        app.place,
        crate::place::PlaceState::Offline {
            binding,
            generation: 9,
            snapshot: reminted,
        }
    );
}

#[test]
fn a_refused_invitation_leaves_no_binding_in_app_state() {
    let mut app = App::test_stub();
    let invite = Box::new(crate::place::invite::PlaceInviteV1 {
        version: crate::place::invite::PLACE_INVITE_VERSION,
        binding: crate::place::PlaceBindingV1::new(
            crate::place::PlaceId([0x71; 32]),
            crate::place::SharedContainerId([0x72; 32]),
            crate::place::ChatSpaceId([0x73; 32]),
            "hall",
        )
        .unwrap(),
        founder: [0x74; 32],
        inviter: [0x75; 32],
        inviter_prekey: inline_artifact(b"prekey"),
        governance: inline_artifact(b"drop"),
        key_welcome: inline_artifact(b"control"),
        key_direct: inline_artifact(b"direct"),
        expected_epoch: [0x76; 32],
        membership_heads: vec![[0x77; 32]],
        not_after_ms: u64::MAX,
        rendezvous: Vec::new(),
        projection_grant: None,
    });

    let effects = app.join_place(invite);
    let generation = match &app.place {
        // While admission runs the app names a generation and nothing else.
        // The envelope names a place; that is not the same as belonging to one.
        crate::place::PlaceState::Joining { generation } => *generation,
        other => panic!("join must begin in Joining, got {other:?}"),
    };
    assert!(app.place.binding().is_none());
    assert!(matches!(
        effects.as_slice(),
        [Effect::JoinPlace { generation: g, .. }] if *g == generation
    ));

    assert_eq!(
        app.apply_update(Update::PlaceJoined {
            session: app.session_id,
            generation,
            result: Err("Gemot membership does not contain this Personae root".into()),
        }),
        vec![Effect::Redraw]
    );
    // Failed, not Degraded: a refusal means there is no place, so nothing may
    // carry a binding that admission never granted.
    assert!(matches!(app.place, crate::place::PlaceState::Failed { .. }));
    assert!(app.place.binding().is_none());
}

#[test]
fn a_stale_join_answer_cannot_bind_a_session() {
    let mut app = App::test_stub();
    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0x81; 32]),
        crate::place::SharedContainerId([0x82; 32]),
        crate::place::ChatSpaceId([0x83; 32]),
        "hall",
    )
    .unwrap();
    app.place = crate::place::PlaceState::Joining { generation: 7 };

    assert!(
        app.apply_update(Update::PlaceJoined {
            session: app.session_id,
            generation: 6,
            result: Ok((binding, crate::place::OfflinePlaceSnapshot::default())),
        })
        .is_empty(),
        "an answer from an earlier join produces no effect"
    );
    assert!(matches!(
        app.place,
        crate::place::PlaceState::Joining { generation: 7 }
    ));
}

#[test]
fn shared_nodes_arrive_without_disturbing_the_local_canvas() {
    let mut app = App::test_stub();
    // The person's own work: a node they opened and selected.
    let mine = app.graph_runtimes.visit("https://mine.example");
    let mine_id = app.graph_runtimes.graph().get_node(mine).unwrap().id;
    let before = app.graph_runtimes.graph().nodes().count();

    let shared = crate::place::projection::SharedGraph {
        nodes: vec![
            crate::place::projection::SharedNode {
                id: "https://theirs.example".into(),
                address: "https://theirs.example".into(),
            },
            // Already present locally: reconciliation must not duplicate it.
            crate::place::projection::SharedNode {
                id: "https://mine.example".into(),
                address: "https://mine.example".into(),
            },
        ],
    };

    assert_eq!(
        app.reconcile_shared_graph(&shared),
        1,
        "only the missing one"
    );
    assert_eq!(app.graph_runtimes.graph().nodes().count(), before + 1);
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("https://theirs.example")
            .is_some(),
        "the shared address arrived"
    );
    assert_eq!(
        app.graph_runtimes.selected_members(),
        vec![mine_id],
        "a background reconcile must not move the person's selection"
    );

    // Idempotent: converging again changes nothing, which is what lets this
    // run on every resync.
    assert_eq!(app.reconcile_shared_graph(&shared), 0);
    assert_eq!(app.graph_runtimes.graph().nodes().count(), before + 1);

    // A node leaving the place does not remove the person's copy of it.
    assert_eq!(
        app.reconcile_shared_graph(&crate::place::projection::SharedGraph::default()),
        0
    );
    assert_eq!(app.graph_runtimes.graph().nodes().count(), before + 1);
}

#[test]
fn a_held_document_is_refused_outside_a_place_and_its_node_stays() {
    // T5c's address half. A place-held address names the mere that holds it,
    // and a personal session has nobody to ask. The refusal is loud and the
    // node is kept: the person asked for this document, and it opens the
    // moment they are in the place that can reach it.
    let mut app = App::test_stub();
    let address = crate::knot_authoring::place_held_address(&[0x7c; 32], "field.knot");
    assert!(crate::knot_authoring::is_knot_address(&address));

    let before = app.graph_runtimes.graph().nodes().count();
    app.update(Action::OpenAddress(address.clone()));

    assert!(
        app.take_events().iter().any(|event| matches!(
            event,
            crate::observe::AppEvent::PlaceRefused(reason)
                if reason.contains("held by another mere")
        )),
        "a foreign holder in a personal session is refused with a reason"
    );
    assert_eq!(
        app.graph_runtimes.graph().nodes().count(),
        before + 1,
        "and the node the person asked for stays"
    );
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url(&address)
            .is_some()
    );
}

#[test]
fn a_shared_knot_address_needs_no_path_of_its_own() {
    // T4's Commons half. A Knot document is an addressed node in the shared
    // graph, so it rides the ordinary share path and arrives through the
    // ordinary reconcile. The plan forbids inventing OpenSharedKnot or
    // SaveSharedKnot, and this is the assertion that keeps that honest: if
    // Knot ever needs its own share path, this test is what breaks.
    let mut app = App::test_stub();
    let shared = crate::place::projection::SharedGraph {
        nodes: vec![
            crate::place::projection::SharedNode {
                id: "file:///vault/minutes.knot".into(),
                address: "file:///vault/minutes.knot".into(),
            },
            crate::place::projection::SharedNode {
                id: "https://example.test/page".into(),
                address: "https://example.test/page".into(),
            },
        ],
    };

    assert_eq!(app.reconcile_shared_graph(&shared), 2);
    let knot = app
        .graph_runtimes
        .graph()
        .get_node_by_url("file:///vault/minutes.knot");
    assert!(knot.is_some(), "the Knot address reconciled like any other");

    // And it is recognizable as Knot, so the existing OpenAddress route sends
    // it to the authoring engine without the place port routing anything.
    assert!(crate::knot_authoring::is_knot_address(
        "file:///vault/minutes.knot"
    ));
    assert!(!crate::knot_authoring::is_knot_address(
        "https://example.test/page"
    ));
}
