//! App-spine tests: every one drives `Action` -> `update` -> `Effect` and
//! asserts on app truth, so they read as the spine's own specification.

use super::*;

#[test]
fn place_binding_survives_switch_and_restart_as_a_worker_open() {
    let root = std::env::temp_dir().join(format!("turnstone-place-adopt-{}", uuid::Uuid::new_v4()));
    let mut app = App::test_stub();
    app.data_root = root.clone();

    let personal = crate::panes::SessionId::new();
    std::fs::create_dir_all(session::session_dir(&root, personal)).unwrap();
    app.adopt_session(personal);
    assert_eq!(app.place, crate::place::PlaceState::Personal);
    assert!(
        !session::place_binding_path(&app.session_dir()).exists(),
        "adopting a personal session does not invent a place sidecar"
    );

    let target = crate::panes::SessionId::new();
    let sdir = session::session_dir(&root, target);
    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0x11; 32]),
        crate::place::SharedContainerId([0x22; 32]),
        crate::place::ChatSpaceId([0x33; 32]),
        "commons",
    )
    .unwrap();
    session::save_place_binding(&sdir, &binding).unwrap();
    let key = app.graph_runtimes.visit("https://shared.example");
    let shared_node = app.graph_runtimes.graph().get_node(key).unwrap().id;
    session::save_session_graph(&sdir, app.graph_runtimes.graph());

    let effects = app.adopt_session(target);
    assert!(
        matches!(
            &app.place,
            crate::place::PlaceState::Opening {
                binding: actual,
                generation: 2,
            } if actual == &binding
        ),
        "the app exposes the worker open instead of presenting the cache as current"
    );
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::OpenPlace {
                session,
                generation: 2,
                binding: actual,
            } if *session == target && actual == &binding
        )),
        "adoption asks the shell-owned worker to materialize the retained domains"
    );
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_id(shared_node)
            .is_some(),
        "the cached graph remains immediately available"
    );

    let mut reopened = App::test_stub();
    reopened.data_root = root.clone();
    let effects = reopened.adopt_session(target);
    assert!(
        matches!(
            &reopened.place,
            crate::place::PlaceState::Opening {
                binding: actual,
                generation: 1,
            } if actual == &binding
        ),
        "a fresh app state requests the same durable binding"
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::OpenPlace {
            session,
            generation: 1,
            binding: actual,
        } if *session == target && actual == &binding
    )));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn stale_place_update_from_a_departed_session_is_ignored() {
    let root = std::env::temp_dir().join(format!("turnstone-place-stale-{}", uuid::Uuid::new_v4()));
    let mut app = App::test_stub();
    app.data_root = root.clone();
    let first = crate::panes::SessionId::new();
    let second = crate::panes::SessionId::new();
    let first_binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0x11; 32]),
        crate::place::SharedContainerId([0x12; 32]),
        crate::place::ChatSpaceId([0x13; 32]),
        "first",
    )
    .unwrap();
    let second_binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0x21; 32]),
        crate::place::SharedContainerId([0x22; 32]),
        crate::place::ChatSpaceId([0x23; 32]),
        "second",
    )
    .unwrap();
    session::save_place_binding(&session::session_dir(&root, first), &first_binding).unwrap();
    session::save_place_binding(&session::session_dir(&root, second), &second_binding).unwrap();

    app.adopt_session(first);
    app.adopt_session(second);
    assert_eq!(app.session_id, second);
    let stale = crate::place::OfflinePlaceSnapshot {
        graph: crate::place::GraphCache {
            nodes: 99,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(
        app.apply_update(Update::PlaceOpened {
            session: first,
            generation: 1,
            result: Ok(stale),
        })
        .is_empty(),
        "a stale worker answer produces no follow-up effect"
    );
    assert!(
        matches!(
            &app.place,
            crate::place::PlaceState::Opening {
                binding,
                generation: 2,
            } if binding == &second_binding
        ),
        "the stale answer cannot replace the active session's opening"
    );

    let current = crate::place::OfflinePlaceSnapshot {
        graph: crate::place::GraphCache {
            nodes: 2,
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        app.apply_update(Update::PlaceOpened {
            session: second,
            generation: 2,
            result: Ok(current.clone()),
        }),
        vec![Effect::Redraw]
    );
    assert_eq!(
        app.place,
        crate::place::PlaceState::Offline {
            binding: second_binding,
            generation: 2,
            snapshot: current,
        }
    );
    let _ = std::fs::remove_dir_all(root);
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

fn inline_artifact(bytes: &[u8]) -> crate::place::invite::ArtifactRefV1 {
    crate::place::invite::ArtifactRefV1::Inline {
        media_type: "application/vnd.mere.place-artifact".into(),
        digest: proofs::Digest::blake3(bytes)
            .bytes
            .as_slice()
            .try_into()
            .unwrap(),
        bytes: bytes.to_vec(),
    }
}

#[test]
fn malformed_place_binding_is_visible_without_hiding_cached_graph() {
    let root =
        std::env::temp_dir().join(format!("turnstone-place-failure-{}", uuid::Uuid::new_v4()));
    let mut app = App::test_stub();
    app.data_root = root.clone();
    let target = crate::panes::SessionId::new();
    let sdir = session::session_dir(&root, target);
    std::fs::create_dir_all(&sdir).unwrap();
    std::fs::write(session::place_binding_path(&sdir), b"{ not valid JSON").unwrap();
    let key = app.graph_runtimes.visit("https://cached.example");
    let cached_node = app.graph_runtimes.graph().get_node(key).unwrap().id;
    session::save_session_graph(&sdir, app.graph_runtimes.graph());

    app.adopt_session(target);
    assert!(
        matches!(
            &app.place,
            crate::place::PlaceState::Failed { error }
                if error.contains("place sidecar JSON")
        ),
        "the malformed binding is product-visible: {:?}",
        app.place
    );
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_id(cached_node)
            .is_some(),
        "a binding fault does not discard the cached graph"
    );
    assert!(
        session::place_binding_path(&sdir).exists(),
        "adoption preserves the malformed sidecar for repair"
    );
    let _ = std::fs::remove_dir_all(root);
}

/// The layout round-trip through the facet store: a session saved as
/// graph.json + `arrangement.*` facets (per-node) + `scene.*` facets (on
/// the container id) re-adopts with each node back at its saved position
/// and size, and the scene's own settings (physics damping) restored — the
/// graph itself is position-free, so without the facets every node would
/// park at the origin and the scene would reset to defaults.
#[test]
fn adopt_session_restores_the_saved_canvas_layout_from_facets() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-facet-adopt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    // A manifest so the session has a container id for its `scene.*` facets.
    let container = uuid::Uuid::from_u128(0xc0ffee);
    app.sessions.insert(pandect::GraphSessionManifest::new(
        app.session_id,
        crate::panes::GraphId::from_uuid(container),
    ));
    let sdir = app.session_dir();
    std::fs::create_dir_all(&sdir).unwrap();

    // A one-node session on disk: the graph, its per-node arrangement (a
    // position and a size override), and the scene's own damping setting.
    let key = app.graph_runtimes.visit("https://layout.example");
    let id = app.graph_runtimes.graph().get_node(key).unwrap().id;
    session::save_session_graph(&sdir, app.graph_runtimes.graph());
    let mut facets = pandect::NodeFacetStore::new();
    pandect::write_arrangement_positions(&mut facets, [(id, (444.0, -55.0))]);
    pandect::write_arrangement_sizes(&mut facets, [(id, 96.0)]);
    pandect::write_scene_facets(
        &mut facets,
        container,
        &pandect::SceneFacets {
            physics_damping: 5.5,
            ..pandect::SceneFacets::default()
        },
    );
    // Browser state rides the same store now (web.* facets): live content
    // was ON for this node, so the adopt must respawn it.
    let mut browser = pandect::browser_node_state::BrowserNodeStates::new();
    browser.entry(id).content_on = true;
    pandect::write_web_states(&mut facets, &browser);
    session::save_node_facets(&sdir, &facets);

    // Adopt (the boot/switch seam): the node comes back AND lands where
    // it was left.
    let effects = app.adopt_session(app.session_id);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::SpawnContent { node, .. } if *node == id)),
        "content-on read from the web.content facet respawns on adopt"
    );
    let (restored, _) = app
        .graph_runtimes
        .graph()
        .get_node_by_url("https://layout.example")
        .expect("the graph restored");
    let pos = app
        .graph_runtimes
        .node_position(restored)
        .expect("a restored position");
    assert!(
        (pos.x - 444.0).abs() < 1.0 && (pos.y + 55.0).abs() < 1.0,
        "the facet layout is applied, got {pos:?}"
    );
    let size = app.graph_runtimes.node_size(restored);
    assert!(
        (size - 96.0).abs() < 0.001,
        "the size override rode the facets too, got {size}"
    );
    assert!(
        (app.physics_damping - 5.5).abs() < 0.001,
        "the scene.physics_damping facet restored, got {}",
        app.physics_damping
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// The B1 residency arc, headless: a staged install shows its ask, the
/// confirm mints the denizen (node + binding facet + gate-projected grant
/// in a persisted nested world), and the runtime rebuilds from durable
/// truth alone.
#[test]
fn denizen_installs_after_visible_review() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-denizen-b1-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = app.data_root.join("trail-keeper.lua");
    std::fs::write(&pack, "mere.open('mere://kept/note')").unwrap();

    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    assert!(app.pending_install.is_some());
    let rows = app.denizen_actions();
    assert!(
        rows[0].0.contains("grants:") && rows[0].0.contains("(lua)"),
        "the ask is the first palette row: {rows:?}"
    );
    assert!(
        app.take_events()
            .iter()
            .any(|e| matches!(e, AppEvent::DenizenStaged(_))),
        "staging is observable"
    );

    app.update(Action::ConfirmInstallDenizen);
    assert!(app.pending_install.is_none());
    assert_eq!(app.denizens.residents.len(), 1);
    let (&member, resident) = app.denizens.residents.iter().next().unwrap();
    let binding = pandect::read_denizen_binding(app.graph_runtimes.facets(), member)
        .expect("the binding facet is durable truth");
    assert_eq!(binding.subject, resident.subject.to_hex());
    assert_eq!(binding.kind, pandect::DenizenKind::Scenario);
    assert!(
        binding.legacy_nested_log.is_empty(),
        "the facet is pure agency"
    );
    let borne = app
        .graph_runtimes
        .graph()
        .get_node_key_by_id(member)
        .and_then(|key| app.graph_runtimes.graph().get_node(key))
        .and_then(|node| node.nested.clone())
        .expect("the node BEARS its world");
    assert_eq!(
        borne.as_str(),
        resident.subject.to_hex(),
        "structure on the node"
    );
    assert!(
        resident
            .nested
            .graph()
            .key_of(&servitor::Gate::projection_id(&crate::denizen::world_cap()))
            .is_some(),
        "the grant projection is in the nested world"
    );
    assert!(
        crate::denizen::nested_log_path(&app.session_dir(), &resident.subject.to_hex()).exists(),
        "the nested log persisted at its birth"
    );

    let rebuilt = crate::denizen::rebuild(
        app.graph_runtimes.facets(),
        app.graph_runtimes.graph(),
        &app.session_dir(),
        app.identity.as_ref(),
    );
    assert_eq!(rebuilt.residents.len(), 1);
    assert!(
        rebuilt.legacy_heals.is_empty(),
        "a fresh install needs no heal"
    );
    assert!(
        servitor::AuthorityProvider::covers(
            &rebuilt.authority,
            resident.subject,
            &crate::denizen::world_cap(),
            servitor::Mode::Write
        ),
        "authority derives from the projection, not from a second store"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// The gate refuses a denizen's petition outside its granted scope, and
/// commits an in-scope one attributed — the servitor pipeline live over a
/// resident's actual nested world.
#[test]
fn resident_petitions_run_through_the_gate() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-denizen-gate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = app.data_root.join("keeper.lua");
    std::fs::write(&pack, "mere.open('mere://kept/note')").unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    let (&member, _) = app.denizens.residents.iter().next().unwrap();

    let subject = app.denizens.residents[&member].subject;
    let authority = app.denizens.authority.clone();
    let gate = app.denizens.gate.clone();
    let resident = app.denizens.residents.get_mut(&member).unwrap();

    let rev = resident.nested.revision();
    let committed = gate
        .petition(
            &authority,
            &mut resident.nested,
            subject,
            &servitor::ScopePath::parse(crate::denizen::SCENARIO_SCOPE).unwrap(),
            rev,
            vec![chartulary::EditSpec::InsertNode(
                chartulary::Container::new("scenario/kept-note"),
            )],
        )
        .expect("an in-scope petition commits");
    let entry = &resident.nested.log().entries()[committed.batch.0 as usize];
    assert_eq!(
        entry.author,
        subject.to_author(),
        "attributed to the denizen"
    );

    let rev = resident.nested.revision();
    let err = gate
        .petition(
            &authority,
            &mut resident.nested,
            subject,
            &servitor::ScopePath::parse("notes").unwrap(),
            rev,
            vec![chartulary::EditSpec::InsertNode(
                chartulary::Container::new("notes/sneaky"),
            )],
        )
        .unwrap_err();
    assert!(
        matches!(err, servitor::GateError::Unauthorized { .. }),
        "an ungranted path refuses: {err:?}"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// The ruled install-review condition: a reviewer sees *when this runs* beside
/// *what it may touch*, before either is granted. And confirming registers the
/// watch, which only succeeds because the same install granted read over the
/// region it named.
#[cfg(feature = "piccolo")]
#[test]
fn the_review_names_the_watch_and_confirming_registers_it() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-denizen-review-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = app.data_root.join("filer.lua");
    std::fs::write(
        &pack,
        "-- @watch mere://inbox
mere.snapshot()",
    )
    .unwrap();

    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    let review = crate::denizen::review_line(app.pending_install.as_ref().expect("staged"));
    assert!(
        review.contains("wakes on: mere://inbox"),
        "the ask names the wake: {review}"
    );
    assert!(
        app.watches.is_empty(),
        "and nothing is registered until the review is confirmed"
    );

    app.update(Action::ConfirmInstallDenizen);
    let (_, resident) = app.denizens.residents.iter().next().unwrap();
    let subject = resident.subject;
    let watches = app.watches.watches();
    assert_eq!(watches.len(), 1, "confirming registered the declared watch");
    assert_eq!(watches[0].subject, subject);
    assert_eq!(
        watches[0].self_author,
        subject.to_hex(),
        "labelled in this journal's convention, so it cannot wake itself"
    );
}

/// A body's Actions lower through `update`, which ends in the drain, so
/// running a behavior re-enters it. The guard makes that a no-op rather than
/// letting the clock and app tiers fire in the middle of a graph cascade,
/// outside its rounds and outside its budget.
#[cfg(feature = "piccolo")]
#[test]
fn a_running_behavior_does_not_re_enter_the_drain() {
    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!("turnstone-reentry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    app.now_ms = Some(0);

    // A schedule that is already due, and a graph behavior whose run would
    // re-enter the drain. Without the guard the schedule fires nested.
    let sched = app.data_root.join("ticker.lua");
    std::fs::write(
        &sched,
        "-- @watch every/minute
mere.open('mere://ticked')",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: sched.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);

    let watcher = app.data_root.join("watcher.lua");
    std::fs::write(
        &watcher,
        "-- @watch https://example.com/w/
mere.open('mere://watched')",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: watcher.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);

    // Time has not advanced, so the schedule is not due; the graph behavior
    // is. Its run must not drag the clock tier in with it.
    app.update(Action::OpenAddress("https://example.com/w/one".to_string()));
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://watched")
            .is_some(),
        "the graph behavior ran"
    );
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://ticked")
            .is_none(),
        "and the schedule did not fire inside its cascade"
    );
    assert!(!app.draining, "the flag is cleared when the drain returns");
}

/// A behavior installed today still wakes tomorrow. Watches are registered at
/// install and live in memory, so without persistence a reload silently
/// leaves every table empty and every behavior inert.
///
/// Persisted rather than re-derived from the pack source: re-deriving would
/// restart a graph watch's cursor at zero (re-waking on history it had already
/// considered) and restart a schedule's period (so a daily behavior never
/// fires for anyone who reopens their session each morning).
#[cfg(feature = "piccolo")]
#[test]
fn watches_survive_a_session_reload() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-watch-reload-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    app.now_ms = Some(1_000);

    let pack = app.data_root.join("filer.lua");
    std::fs::write(
        &pack,
        "-- @watch file:///notes/
mere.open('mere://filed')",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    assert_eq!(app.watches.watches().len(), 1);
    let cursor_before = app.watches.watches()[0].cursor;
    let scope_before = app.watches.watches()[0].scope.clone();

    // The reload: the tables come back from disk, not from the pack.
    let (graph, app_tier, time) = crate::denizen::load_watches(&app.session_dir());
    assert_eq!(graph.watches().len(), 1, "the graph watch came back");
    assert!(app_tier.is_empty());
    assert!(time.is_empty());
    assert_eq!(graph.watches()[0].scope, scope_before);
    assert_eq!(
        graph.watches()[0].cursor,
        cursor_before,
        "with its cursor, so it does not re-wake on history it already considered"
    );
    assert_eq!(
        graph.watches()[0].self_author,
        app.watches.watches()[0].self_author,
        "and its author label, so the no-self-wake refusal still holds"
    );

    // And uninstalling clears the persisted copy, not just the live one.
    let member = *app.denizens.residents.keys().next().unwrap();
    app.update(Action::UninstallDenizen { member });
    let (graph, _, _) = crate::denizen::load_watches(&app.session_dir());
    assert!(
        graph.is_empty(),
        "a removed denizen leaves no watch on disk"
    );
}

/// A schedule's phase survives too, which is the half re-deriving would lose.
#[cfg(feature = "piccolo")]
#[test]
fn a_schedules_phase_survives_a_reload() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-phase-reload-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    app.now_ms = Some(500_000);

    let pack = app.data_root.join("gardener.lua");
    std::fs::write(
        &pack,
        "-- @watch every/day
mere.snapshot()",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);

    let (_, _, time) = crate::denizen::load_watches(&app.session_dir());
    assert_eq!(time.watches().len(), 1);
    assert_eq!(
        time.watches()[0].last_fired_ms,
        500_000,
        "the period is measured from when it was installed, not from the reload"
    );
    assert_eq!(time.watches()[0].period, servitor::Period::Day);
}

/// W5, the flagship: a summarizer watching a container writes a note when its
/// members change, attributed to itself, and stops when its grant is revoked
/// while the note it already wrote stays standing.
///
/// The digest is plain text. The intel seam (esp) would replace the body of
/// the summary without changing the shape of any of this: what the slice is
/// proving is the loop, the attribution, and the revocation, not the prose.
#[cfg(feature = "piccolo")]
#[test]
fn a_summarizer_writes_a_note_when_its_neighborhood_changes() {
    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!("turnstone-summary-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();

    // Directory form, per the containment rule.
    let pack = app.data_root.join("summarizer.lua");
    // The digest is what woke it, not a constant. A constant would be written
    // once and then be a no-op forever, because an unchanged body is not a
    // change and correctly journals nothing: the first version of this test
    // asserted against exactly that.
    std::fs::write(
        &pack,
        "-- @watch https://example.com/notes/
         mere.write('mere://summary', 'neighborhood: ' .. mere.trigger())",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });

    // Authoring is never preselected, so the review must be widened on
    // purpose before the summarizer can write anything.
    if let Some(pending) = app.pending_install.as_mut() {
        pending.rings.push(crate::ring::Ring::Author);
    }
    app.update(Action::ConfirmInstallDenizen);
    let (_, resident) = app.denizens.residents.iter().next().unwrap();
    let subject = resident.subject;
    let subject_hex = subject.to_hex();

    app.update(Action::ReseedLayout);
    let before = app.journal.lock().unwrap().entries().len();

    // A member appears in the watched neighborhood.
    app.update(Action::OpenAddress(
        "https://example.com/notes/one".to_string(),
    ));

    let summary = app
        .graph_runtimes
        .graph()
        .get_node_by_url("mere://summary")
        .map(|(_, node)| node.clone());
    assert!(summary.is_some(), "the summary note was written");

    let wrote: Vec<_> = {
        let journal = app.journal.lock().unwrap();
        journal
            .entries()
            .iter()
            .skip(before)
            .filter(|entry| entry.author == subject_hex)
            .map(|entry| entry.delta.clone())
            .collect()
    };
    assert!(
        wrote.iter().any(|delta| matches!(
            delta,
            mere::kernel::graph::capture::CapturedDelta::ReplaySetNodeBodyById { .. }
        )),
        "and the body write is in the journal under the summarizer"
    );

    // Revoking the grant stops the updates. The note it already wrote is
    // untouched: revoking authority destroys nothing.
    app.denizens.authority.revoke_root_grants(subject);
    app.watches.remove_subject(subject);
    let after_revoke = app.journal.lock().unwrap().entries().len();
    app.update(Action::OpenAddress(
        "https://example.com/notes/two".to_string(),
    ));
    let still_writing = {
        let journal = app.journal.lock().unwrap();
        journal
            .entries()
            .iter()
            .skip(after_revoke)
            .any(|entry| entry.author == subject_hex)
    };
    assert!(
        !still_writing,
        "a revoked summarizer writes nothing further"
    );
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://summary")
            .is_some(),
        "and the note it already wrote is still standing"
    );
}

/// Authoring is a grant of its own: a body holding the ordinary control rings
/// cannot write content, which is what keeps `Dispatch` from quietly meaning
/// "may rewrite your notes".
#[cfg(feature = "piccolo")]
#[test]
fn writing_content_needs_the_author_ring() {
    let app = App::test_stub();
    let err = crate::script::run(
        &app,
        "mere.write('mere://x', 'hello')",
        crate::script::ScriptCapabilities::control(),
        200,
        &crate::behaviors::TriggerContext::default(),
    )
    .unwrap_err();
    assert!(err.contains("content.author"), "refused by name: {err}");
}

/// W4: a behavior fires on the clock, and fires identically on replay because
/// the clock is fed in rather than sampled.
#[cfg(feature = "piccolo")]
#[test]
fn a_scheduled_behavior_fires_on_a_fed_clock_and_replays() {
    fn run() -> Vec<bool> {
        let mut app = App::test_stub();
        app.data_root =
            std::env::temp_dir().join(format!("turnstone-schedule-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&app.data_root);
        std::fs::create_dir_all(app.session_dir()).unwrap();
        app.now_ms = Some(1_000);

        let pack = app.data_root.join("gardener.lua");
        std::fs::write(
            &pack,
            "-- @watch every/hour
mere.open('mere://swept')",
        )
        .unwrap();
        app.update(Action::InstallDenizen {
            path: pack.display().to_string(),
        });
        app.update(Action::ConfirmInstallDenizen);
        assert_eq!(app.time_watches.watches().len(), 1, "the schedule stands");

        const HOUR: u64 = 3_600_000;
        [1_000, 1_000 + HOUR / 2, 1_000 + HOUR]
            .into_iter()
            .map(|now| {
                app.now_ms = Some(now);
                app.update(Action::ReseedLayout);
                app.graph_runtimes
                    .graph()
                    .get_node_by_url("mere://swept")
                    .is_some()
            })
            .collect()
    }
    let first = run();
    assert_eq!(
        first,
        vec![false, false, true],
        "install is not a tick; half a period is not a period; the hour is"
    );
    assert_eq!(first, run(), "the same instants fire the same way");
}

/// A host that supplies no clock fires no schedule, rather than reading "no
/// time" as time zero and running everything at once.
#[cfg(feature = "piccolo")]
#[test]
fn a_session_without_a_clock_never_fires_a_schedule() {
    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!("turnstone-noclock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    app.now_ms = None;

    let pack = app.data_root.join("gardener.lua");
    std::fs::write(
        &pack,
        "-- @watch every/minute
mere.open('mere://swept')",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    app.update(Action::ReseedLayout);

    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://swept")
            .is_none()
    );
}

/// The review names the period, because granting a schedule is granting
/// resource and the reviewer is owed the sentence.
#[cfg(feature = "piccolo")]
#[test]
fn the_review_names_the_period_a_schedule_asks_for() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-sched-review-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = app.data_root.join("gardener.lua");
    std::fs::write(
        &pack,
        "-- @watch every/day
mere.snapshot()",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    let review = crate::denizen::review_line(app.pending_install.as_ref().expect("staged"));
    assert!(review.contains("wakes on: every day"), "{review}");
}

/// W3: a behavior wakes on what the *application* did, not on a graph write,
/// and without anyone polling.
#[cfg(feature = "piccolo")]
#[test]
fn a_behavior_wakes_on_an_app_event_without_polling() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-app-watch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();

    // `app/...` is an event scope, not an address: nothing is resolved or
    // minted for it.
    let pack = app.data_root.join("greeter.lua");
    std::fs::write(
        &pack,
        "-- @watch app/address-opened
mere.open('mere://greeted')",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    assert_eq!(app.app_watches.watches().len(), 1, "the app watch stands");
    assert!(
        app.watches.is_empty(),
        "and it did not also register a graph watch"
    );

    let _ = app.take_events();
    app.update(Action::OpenAddress("https://example.com/x".to_string()));

    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://greeted")
            .is_some(),
        "the app event woke the behavior and its Action landed"
    );
}

/// The app tier's own no-self-wake rule: a body cannot be woken by the events
/// its own run produced, which is what keeps an `app/` watch from spinning.
#[cfg(feature = "piccolo")]
#[test]
fn an_app_watch_is_not_woken_by_its_own_events() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-app-selfwake-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();

    // Watches the very event its own body causes. Without attribution this is
    // a spin that only the cascade budget would stop.
    let pack = app.data_root.join("ouroboros.lua");
    std::fs::write(
        &pack,
        "-- @watch app/address-opened
mere.open('mere://again')",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    let _ = app.take_events();

    app.update(Action::OpenAddress("https://example.com/y".to_string()));
    let exhausted = app
        .take_events()
        .iter()
        .any(|event| matches!(event, crate::observe::AppEvent::CascadeExhausted(_)));
    assert!(
        !exhausted,
        "it settled on its own rather than being stopped by the budget"
    );
}

/// The cascade budget is a setting, and a live one: changing it reaches the
/// app without a restart, and it never disables behaviors outright.
#[test]
fn the_cascade_budget_arrives_from_settings_and_changes_live() {
    let mut app = App::test_stub();
    assert_eq!(
        app.cascade_budget,
        servitor::cascade::CascadeBudget::DEFAULT.rounds(),
        "a session with no stored setting runs the default"
    );

    let mut settings = pandect::ApplicationSettings {
        cascade_budget: 2,
        ..pandect::ApplicationSettings::default()
    };
    app.apply_chrome_settings_snapshot(&crate::settings_pane::ChromeSettings::from(&settings));
    assert_eq!(app.cascade_budget, 2);

    // The live half: a second snapshot moves it again, no restart involved.
    settings.cascade_budget = 7;
    app.apply_chrome_settings_snapshot(&crate::settings_pane::ChromeSettings::from(&settings));
    assert_eq!(app.cascade_budget, 7);

    // And a drifted zero leaves behaviors working rather than switching them
    // off, because the consumer floors it.
    settings.cascade_budget = 0;
    app.apply_chrome_settings_snapshot(&crate::settings_pane::ChromeSettings::from(&settings));
    assert_eq!(
        servitor::cascade::CascadeBudget::new(app.cascade_budget).rounds(),
        1
    );
}

/// The inbox rule, end to end: a node appearing under a watched folder wakes
/// the behavior without anyone asking, and its edit is attributed to it.
///
/// The whole chain in one test: containment derived at mint (so the folder
/// owns the new node live), ancestry read as a scope, the watch matched, the
/// cascade run at the drain, the body's Action lowered through the ordinary
/// spine, and the journal recording it under the denizen rather than the user.
///
/// This failed on its first writing, and the cause was neither the wake
/// machinery nor either of the joins I suspected: the folder was addressed as
/// `.../inbox` while `containment_parent_url` names a parent in **directory
/// form**, `.../inbox/`. Two different addresses, so nothing was ever
/// contained by the folder and its watch matched nothing. Recorded because it
/// is the first thing to check whenever a watch looks inert.
#[cfg(feature = "piccolo")]
#[test]
fn a_node_appearing_under_a_watched_folder_wakes_the_behavior() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-inbox-rule-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();

    // A web-shaped folder, because URL-path containment is what the kernel
    // derives; `mere://` is outside `containment_parent_url`'s scheme set.
    //
    // The trailing slash is load-bearing: the rule names a parent in directory
    // form (`/inbox/`), so a folder node stored as `.../inbox` is a different
    // address and nothing is ever contained by it.
    let pack = app.data_root.join("filer.lua");
    std::fs::write(
        &pack,
        "-- @watch https://example.com/inbox/
mere.open('mere://filed')",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    let (_, resident) = app.denizens.residents.iter().next().unwrap();
    let subject_hex = resident.subject.to_hex();
    assert_eq!(app.watches.watches().len(), 1, "the watch is standing");

    // Everything the install itself stirred up is behind us.
    app.update(Action::ReseedLayout);
    let before = app.journal.lock().unwrap().entries().len();

    // The user drops something in the inbox. Nobody asks the behavior to run.
    app.update(Action::OpenAddress(
        "https://example.com/inbox/thing".to_string(),
    ));

    let journal = app.journal.lock().unwrap();
    let mine: Vec<_> = journal
        .entries()
        .iter()
        .skip(before)
        .filter(|entry| entry.author == subject_hex)
        .collect();
    assert!(
        !mine.is_empty(),
        "the behavior woke and wrote, attributed to itself, not to the user"
    );
    drop(journal);
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://filed")
            .is_some(),
        "and its Action landed through the ordinary spine"
    );
}

/// Residency, authority, and standing subscriptions end together: a watch
/// outliving its body would wake nothing, forever.
#[cfg(feature = "piccolo")]
#[test]
fn uninstalling_a_denizen_takes_its_watch_with_it() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-denizen-watch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = app.data_root.join("watcher.lua");
    std::fs::write(&pack, "mere.snapshot()").unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    let (&member, resident) = app.denizens.residents.iter().next().unwrap();
    let subject = resident.subject;

    // Adopted rather than registered: the authority table is the install's,
    // and this test is about removal, not about the containment law (which
    // servitor proves on its own).
    app.watches.adopt(servitor::Watch {
        subject,
        scope: servitor::ScopePath::parse("folder").unwrap(),
        self_author: subject.to_hex(),
        cursor: 0,
    });
    assert_eq!(app.watches.watches().len(), 1);

    app.update(Action::UninstallDenizen { member });
    assert!(
        app.watches.is_empty(),
        "the watch went with the residency it belonged to"
    );
}

/// With the piccolo runtime: a run lowers the body's Actions through the
/// spine, and the journal attributes the captured edits to the subject.
#[cfg(feature = "piccolo")]
#[test]
fn denizen_runs_attributed() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-denizen-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = app.data_root.join("keeper.lua");
    std::fs::write(&pack, "mere.open('mere://kept/note')").unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    let (&member, resident) = app.denizens.residents.iter().next().unwrap();
    let hex = resident.subject.to_hex();

    app.update(Action::RunDenizen { member });
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://kept/note")
            .is_some(),
        "the body's Action landed through the spine"
    );
    let journal = app.journal.lock().unwrap();
    assert!(
        journal.entries().iter().any(|e| e.author == hex),
        "the captured edit reads back attributed to the subject"
    );
    assert_eq!(
        journal.author(),
        mere::kernel::graph::USER_AUTHOR,
        "the author scope restored after the run"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// The fork arm (G4-R R2): forking from a node mints a new session whose
/// manifest carries the parent back-reference, snapshots the connected
/// component (not the rest of the graph), carries the donor's per-node
/// character as facets through the copy's id remap plus the container's
/// scene settings, and opens by session-switch.
#[test]
fn fork_session_snapshots_the_component_with_its_facets() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-fork-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    let donor_container = uuid::Uuid::from_u128(0xd0);
    app.sessions.insert(pandect::GraphSessionManifest::new(
        app.session_id,
        crate::panes::GraphId::from_uuid(donor_container),
    ));
    std::fs::create_dir_all(app.session_dir()).unwrap();

    // A two-node connected component plus a disconnected bystander.
    let a = app.graph_runtimes.visit("https://fork.example/a");
    let a_id = app.graph_runtimes.graph().get_node(a).unwrap().id;
    app.update(Action::OpenAddress("https://fork.example/b".to_string()));
    let _bystander = {
        let mut g = app.graph_runtimes.graph().clone();
        let k = mere::kernel::graph::apply::add_node(
            &mut g,
            Some(uuid::Uuid::from_u128(0x10e)),
            "https://lone.example".to_string(),
            Default::default(),
        );
        let id = g.get_node(k).unwrap().id;
        app.graph_runtimes.set_graph(g);
        id
    };
    // Donor character: live content on `a` (so web.content refreshes true
    // from live truth) and a scene damping.
    app.physics_damping = 4.75;
    app.apply_update(Update::ContentSpawned {
        node: a_id,
        facts: None,
    });

    let donor_session = app.session_id;
    let effects = app.update(Action::ForkNode { member: a_id });
    let Some(crate::action::Effect::SwitchSession { id: fork_id }) = effects
        .iter()
        .find(|e| matches!(e, crate::action::Effect::SwitchSession { .. }))
        .cloned()
    else {
        panic!("fork returns the switch effect: {effects:?}");
    };
    assert_ne!(fork_id, donor_session);
    let fork_manifest = app.sessions.get(fork_id).expect("fork manifest inserted");
    assert_eq!(
        fork_manifest.parent_session,
        Some(donor_session),
        "the weak parent back-reference"
    );
    assert_ne!(
        fork_manifest.root_graph_id,
        crate::panes::GraphId::from_uuid(donor_container),
        "the fork minted its own real GraphId"
    );

    // The persisted fork: the 2-node component (not the bystander), and
    // the carried facets keyed by the REMAPPED ids.
    let fork_dir = session::session_dir(&app.data_root, fork_id);
    let fork_graph = session::load_session_graph(&fork_dir).expect("fork graph persisted");
    assert_eq!(fork_graph.nodes().count(), 2, "the component, nothing else");
    let fork_facets = session::load_node_facets(&fork_dir).expect("fork facets persisted");
    let fork_a = fork_graph
        .nodes()
        .find(|(_, n)| n.url() == "https://fork.example/a")
        .map(|(_, n)| n.id)
        .expect("the seed's copy");
    assert_ne!(fork_a, a_id, "a fork copy is a new entity");
    assert!(
        !pandect::read_arrangement_positions(&fork_facets).is_empty(),
        "the donor layout rode the carry"
    );
    let web = pandect::read_web_states(&fork_facets);
    assert!(
        web.get(fork_a).is_some_and(|s| s.content_on),
        "web.content carried onto the remapped id"
    );
    let scene = pandect::read_scene_facets(&fork_facets, *fork_manifest.root_graph_id.as_uuid());
    assert!(
        (scene.physics_damping - 4.75).abs() < 0.001,
        "scene.* carried donor-container -> fork-container"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// The world-carry: forking from a denizen node re-bears its nested graph
/// on the fork's copy AND copies the world file into the fork's session
/// dir — donor and fork hold independent worlds thereafter (the kernel
/// copy alone would leave the fork's denizen un-resided).
#[test]
fn fork_carries_denizen_worlds_as_real_copies() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-fork-world-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = app.data_root.join("keeper.lua");
    std::fs::write(&pack, "mere.open('mere://kept/note')").unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    let (member, world_id, donor_revision) = {
        let (&member, resident) = app.denizens.residents.iter().next().unwrap();
        (
            member,
            resident.subject.to_hex(),
            resident.nested.revision(),
        )
    };

    let effects = app.fork_session_from(member);
    let Some(crate::action::Effect::SwitchSession { id: fork_id }) = effects
        .iter()
        .find(|e| matches!(e, crate::action::Effect::SwitchSession { .. }))
        .cloned()
    else {
        panic!("fork returns the switch effect: {effects:?}");
    };
    let fork_dir = session::session_dir(&app.data_root, fork_id);
    let fork_graph = session::load_session_graph(&fork_dir).expect("fork graph persisted");
    let (_, fork_node) = fork_graph
        .nodes()
        .find(|(_, n)| n.nested.is_some())
        .expect("the fork's copy bears the world");
    assert_ne!(fork_node.id, member, "a fork copy is a new entity");
    assert_eq!(
        fork_node.nested.as_ref().map(|log| log.as_str()),
        Some(world_id.as_str()),
        "same world identity, re-borne on the copy"
    );
    assert!(
        crate::denizen::nested_log_path(&fork_dir, &world_id).is_file(),
        "the fork owns a real world file"
    );
    assert!(
        crate::denizen::nested_log_path(&app.session_dir(), &world_id).is_file(),
        "the donor keeps its own"
    );
    // The fork rebuilds a full resident from its OWN dir, no legacy heal.
    let fork_facets = session::load_node_facets(&fork_dir).expect("fork facets persisted");
    let rebuilt =
        crate::denizen::rebuild(&fork_facets, &fork_graph, &fork_dir, app.identity.as_ref());
    assert_eq!(rebuilt.residents.len(), 1, "the fork's denizen resides");
    assert!(rebuilt.legacy_heals.is_empty());
    assert_eq!(
        rebuilt.residents.values().next().unwrap().nested.revision(),
        donor_revision,
        "the carried world is the donor's world, bit-for-bit at fork time"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// Overmap O3: closing a session moves its whole directory to the manifest
/// trash (the derived removed-sessions record — no parallel bin), and
/// recovery moves it back with identity intact and switches to it.
#[test]
fn close_session_trashes_and_recover_restores_identity() {
    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!("turnstone-o3-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    // Two real sessions on disk (manifests bound to the root so trash ops
    // have a home), the second is current.
    app.sessions = pandect::ManifestStore::with_root(session::sessions_root(&app.data_root));
    let keeper = crate::panes::SessionId::new();
    let mut keeper_m = pandect::GraphSessionManifest::new(
        keeper,
        crate::panes::GraphId::from_uuid(uuid::Uuid::from_u128(0xa)),
    );
    keeper_m.display_name = Some("keeper".to_string());
    app.sessions.insert(keeper_m);
    let closing_id = crate::panes::SessionId::new();
    let mut closing_m = pandect::GraphSessionManifest::new(
        closing_id,
        crate::panes::GraphId::from_uuid(uuid::Uuid::from_u128(0xb)),
    );
    closing_m.display_name = Some("expedition".to_string());
    app.sessions.insert(closing_m);
    app.sessions.flush_dirty().unwrap();
    app.session_id = closing_id;

    // Close: the action defers the disk half to the shell-ordered effect
    // (bin release first); apply_trash is that effect's app half.
    let effects = app.update(Action::CloseSession);
    assert!(matches!(
        effects[..],
        [crate::action::Effect::TrashSession { closing, next }]
            if closing == closing_id && next == keeper
    ));
    assert!(app.apply_trash(closing_id));
    assert_eq!(app.trash.len(), 1);
    assert_eq!(app.trash[0].session_id, closing_id);
    assert_eq!(app.trash[0].display_name.as_deref(), Some("expedition"));
    assert!(
        app.sessions.get(closing_id).is_none(),
        "gone from the live set"
    );

    // Recover: the manifest re-lists with the SAME id + graph id, the
    // trash cache empties, and the switch adopts it.
    let effects = app.update(Action::RecoverSession(closing_id));
    assert!(matches!(
        effects[..],
        [crate::action::Effect::SwitchSession { id }] if id == closing_id
    ));
    assert!(app.trash.is_empty(), "the trash entry is consumed");
    let recovered = app.sessions.get(closing_id).expect("re-listed");
    assert_eq!(
        recovered.root_graph_id,
        crate::panes::GraphId::from_uuid(uuid::Uuid::from_u128(0xb)),
        "identity intact"
    );
    assert!(
        app.take_events()
            .iter()
            .any(|e| matches!(e, AppEvent::SessionRecovered(l) if l == "expedition")),
        "the recovery event carries the label"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// Committing a `>` registry row lowers the registry Action through the
/// same spine as everything else, and the palette closes.
#[test]
fn committing_an_action_row_runs_the_action_and_closes() {
    let mut app = App::test_stub();
    for action in [
        Action::OmnibarOpen { command: true },
        Action::OmnibarChar('i'),
        Action::OmnibarChar('s'),
        Action::OmnibarChar('o'),
    ] {
        app.update(action);
    }
    assert!(!app.graph_runtimes.is_isometric());
    let effects = app.update(Action::OmnibarCommit);
    assert!(
        app.graph_runtimes.is_isometric(),
        "the committed toggle ran"
    );
    assert!(!app.omnibar.open, "the palette closed on commit");
    assert!(effects.contains(&Effect::Redraw));
}

/// The content flip lowers through the spine: focused node -> Requested +
/// SpawnContent; the port's honest failure folds back; a failed node
/// retries on the next flip.
#[test]
fn content_flip_lowers_and_fails_honestly() {
    use crate::content::NodeContent;
    let mut app = App::test_stub();
    assert!(
        app.update(Action::ToggleNodeContent).is_empty(),
        "no focus, no-op"
    );
    app.graph_runtimes.visit("https://example.com/page");
    let effects = app.update(Action::ToggleNodeContent);
    let Some(Effect::SpawnContent { node, url }) = effects
        .iter()
        .find(|e| matches!(e, Effect::SpawnContent { .. }))
        .cloned()
    else {
        panic!("flip on a focused node spawns: {effects:?}");
    };
    assert_eq!(url, "https://example.com/page");
    assert_eq!(app.content.get(node), Some(&NodeContent::Requested));
    assert!(
        !app.update(Action::ToggleNodeContent)
            .iter()
            .any(|e| matches!(e, Effect::SpawnContent { .. })),
        "flipping an in-flight node closes, never double-spawns"
    );
    app.content.note_requested(node);
    app.apply_update(Update::ContentFailed {
        node,
        error: "port not wired".into(),
    });
    assert!(
        matches!(app.content.get(node), Some(NodeContent::Failed(_))),
        "failure is a surfaced state"
    );
    assert!(
        app.update(Action::ToggleNodeContent)
            .iter()
            .any(|e| matches!(e, Effect::SpawnContent { .. })),
        "a failed node retries on the next flip"
    );
}

/// The tear-out leaf arm (rung 7 depth): the active pane's leaf leaves
/// the primary tree and joins a lens space — SAME pane id (the retained
/// runner never moves; identity is structural). No lens open spawns one.
#[test]
fn tear_out_moves_the_leaf_and_keeps_its_id() {
    let mut app = App::test_stub();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::ROSTER,
    )));
    let roster_id = app
        .frisket
        .iter_leaves()
        .find(|(_, c, _)| matches!(c, PaneContent::Roster))
        .map(|(id, _, _)| id)
        .expect("summoned");
    let effects = app.update(Action::TearOutActivePane);
    // Departure: the primary tree no longer holds a Roster leaf.
    assert!(
        !app.frisket
            .iter_leaves()
            .any(|(_, c, _)| matches!(c, PaneContent::Roster)),
        "the roster left the primary tree"
    );
    // Arrival: a lens space spawned (no lens was open) and holds the SAME
    // pane id — the leaf moved, nothing was recreated.
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::OpenWindow { .. })),
        "tearing out with no lens spawns one: {effects:?}"
    );
    let lens = app.lenses[0].as_ref().expect("lens space seeded");
    let moved = lens
        .iter_leaves()
        .find(|(_, c, _)| matches!(c, PaneContent::Roster))
        .expect("the roster landed in the lens");
    assert_eq!(moved.0, roster_id, "same pane id across the move");
    // The moved pane STAYS active, so pane-anchored ops follow it: a
    // summon lands beside it IN THE LENS (the window as pane host).
    assert_eq!(
        app.active_pane,
        Some(roster_id),
        "the moved pane stays active"
    );
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::TRAIL,
    )));
    let lens = app.lenses[0].as_ref().unwrap();
    assert!(
        lens.iter_leaves()
            .any(|(_, c, _)| matches!(c, PaneContent::Trail)),
        "summon-beside followed the active pane into the lens"
    );
    assert!(
        !app.frisket
            .iter_leaves()
            .any(|(_, c, _)| matches!(c, PaneContent::Trail)),
        "the summoned trail is not in the primary tree"
    );
    // A PRIMARY pane tearing out reuses the open lens (no window spam).
    app.active_pane = None;
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::GLOSS,
    )));
    let effects = app.update(Action::TearOutActivePane);
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::OpenWindow { .. })),
        "an open lens is reused"
    );
    let lens = app.lenses[0].as_ref().unwrap();
    assert!(
        lens.iter_leaves()
            .any(|(_, c, _)| matches!(c, PaneContent::Gloss(_))),
        "the gloss joined the existing lens"
    );
}

#[test]
fn summon_enforces_registry_multiplicity_in_one_space() {
    let mut app = App::test_stub();
    for _ in 0..2 {
        app.update(Action::SummonPane(crate::panes::PaneKindId::new(
            crate::panes::kind::ROSTER,
        )));
    }
    assert_eq!(
        app.frisket
            .iter_leaves()
            .filter(|(_, content, _)| matches!(content, PaneContent::Roster))
            .count(),
        1,
        "Roster is per-space-and-context"
    );

    for _ in 0..2 {
        app.update(Action::SummonPane(crate::panes::PaneKindId::new(
            crate::panes::kind::GLOSS,
        )));
    }
    assert_eq!(
        app.frisket
            .iter_leaves()
            .filter(|(_, content, _)| matches!(content, PaneContent::Gloss(_)))
            .count(),
        2,
        "Gloss permits many pane instances"
    );
}

/// The rename flow through the omnibar: BeginRenameSession opens the bar in
/// rename mode seeded with the current label; commit lowers RenameSession
/// and the label updates. An empty name clears back to the derived/uuid
/// fallback.
#[test]
fn rename_session_through_the_omnibar_mode() {
    use crate::ui::OmnibarMode;

    let mut app = App::test_stub();
    let id = app.session_id;
    app.sessions
        .insert(pandect::GraphSessionManifest::new(id, GraphId::nil()));
    // The default label is the uuid prefix.
    assert_eq!(app.session_label(id), id.as_uuid().to_string()[..8]);

    app.update(Action::BeginRenameSession);
    assert!(app.omnibar.open);
    assert!(matches!(app.omnibar.mode, OmnibarMode::RenameSession(rid) if rid == id));

    // Type a new name over the seeded label and commit.
    app.omnibar.text = "Research".to_string();
    app.update(Action::OmnibarCommit);
    assert_eq!(app.session_label(id), "Research");
    assert!(!app.omnibar.open, "commit closes the bar");
    assert!(
        matches!(app.omnibar.mode, OmnibarMode::Address),
        "mode resets"
    );

    // An empty rename clears back to the uuid fallback.
    app.update(Action::RenameSession {
        id,
        name: "   ".to_string(),
    });
    assert_eq!(app.session_label(id), id.as_uuid().to_string()[..8]);
}

/// The recycle-bin round trip, app-level (the port simulated by folding
/// its answers): delete stages the focused node's record — ORIGINAL id,
/// title, tags — and drops the node; the Trail derives it into Removed;
/// recover re-mints under the SAME id and Removed derives it away.
#[test]
fn delete_stages_into_the_bin_and_recover_restores_identity() {
    use crate::trail_view::{RowAction, trail_rows};

    let mut app = App::test_stub();
    let url = "https://example.com/gone".to_string();
    app.update(Action::OpenAddress(url.clone()));
    let original = app
        .graph_runtimes
        .focused_member()
        .expect("the opened node is focused");

    let fx = app.update(Action::DeleteFocusedNode);
    assert!(
        app.graph_runtimes.graph().get_node_by_url(&url).is_none(),
        "the node left the graph"
    );
    // The record leaves through the bin port carrying the identity.
    let record = fx
        .iter()
        .find_map(|e| match e {
            Effect::RecordDeleted { record } => Some(record.clone()),
            _ => None,
        })
        .expect("delete stages a bin record: {fx:?}");
    assert_eq!(
        record.node_id, original,
        "the record carries the ORIGINAL id"
    );
    assert_eq!(record.url, url);
    assert!(
        fx.iter().any(|e| matches!(e, Effect::CloseContent { .. })),
        "its content session is closed: {fx:?}"
    );

    // The port answers with the refreshed list (folded as the drain would).
    app.apply_update(Update::BinListed {
        records: vec![record],
    });
    assert!(
        trail_rows(&app)
            .iter()
            .any(|r| matches!(&r.action, RowAction::Recover(id) if id == &original.to_string())),
        "the staged node derives into the Trail's Removed section"
    );

    // Recover BY IDENTITY: same uuid, and Removed derives it away with the
    // record still in the bin (append-only until athanor's pass).
    app.update(Action::RecoverDeletedNode(original));
    assert_eq!(
        app.graph_runtimes.focused_member(),
        Some(original),
        "the node is back under its ORIGINAL id, selected"
    );
    assert!(
        app.graph_runtimes.graph().get_node_by_url(&url).is_some(),
        "the url resolves again"
    );
    assert!(
        !trail_rows(&app)
            .iter()
            .any(|r| matches!(&r.action, RowAction::Recover(_))),
        "Removed derives away once the node is present (record still staged)"
    );
    assert!(!app.removed.is_empty(), "the bin record itself remains");
}

/// The envelope lane end to end (participant gate B3): a dropped `.wasm`
/// installs as a component denizen after the same VISIBLE review — whose
/// row now names its ring profile — and running it lowers exactly the
/// emissions its grant covers, attributed, while the ungranted ring and
/// gate management are refused inside the run.
#[cfg(feature = "wasm")]
#[test]
fn a_component_denizen_acts_only_within_its_reviewed_rings() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-component-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = std::path::Path::new("scenarios/fixtures/app_core_guest.wasm");
    assert!(
        pack.exists(),
        "the app-core guest fixture is missing at {}",
        pack.display()
    );

    // Stage: the review names the component and its preselected rings.
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    let review = &app.denizen_actions()[0].0;
    assert!(review.contains("(wasm)"), "the lane is named: {review}");
    for ring in ["navigate", "panes", "dispatch"] {
        assert!(review.contains(ring), "the ask names {ring}: {review}");
    }
    assert!(
        !review.contains("session"),
        "the destructive ring is never preselected: {review}"
    );

    app.update(Action::ConfirmInstallDenizen);
    let (member, subject) = {
        let (&m, r) = app.denizens.residents.iter().next().unwrap();
        (m, r.subject)
    };
    let binding = pandect::read_denizen_binding(app.graph_runtimes.facets(), member).unwrap();
    assert_eq!(binding.kind, pandect::DenizenKind::Pack);
    let file = app
        .graph_runtimes
        .facets()
        .get(
            &member,
            &chartulary::FacetId::new(crate::denizen::COMPONENT_FACET),
        )
        .and_then(|v| v.as_str().map(str::to_string))
        .expect("the component facet points at the stored bytes");
    assert!(
        crate::denizen::component_path(&app.session_dir(), &file).is_file(),
        "the component's bytes live beside the worlds"
    );
    // The grant is exactly the reviewed rings: each ring is its own power,
    // and there is no capability above them that could grant them wholesale.
    let covers = |ring: crate::ring::Ring| {
        servitor::AuthorityProvider::covers(
            &app.denizens.authority,
            subject,
            &ring.cap().expect("a grantable ring"),
            servitor::Mode::Write,
        )
    };
    use crate::ring::Ring;
    assert!(covers(Ring::Navigate) && covers(Ring::Panes) && covers(Ring::Dispatch));
    assert!(!covers(Ring::Session), "an unreviewed ring is ungranted");

    // Run: the guest emits one action per ring. Only the covered ones land.
    let before = app.graph_runtimes.graph().node_count();
    app.update(Action::RunDenizen { member });
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://kept/note")
            .is_some(),
        "the navigate emission lowered through the spine"
    );
    assert_eq!(
        app.graph_runtimes.graph().node_count(),
        before + 1,
        "and nothing else minted a node"
    );
    assert!(
        app.take_events()
            .iter()
            .any(|e| matches!(e, AppEvent::DenizenRan(_))),
        "the run is observable"
    );
    // Attribution: the component's edit reads back under its subject.
    let journal = app.journal.lock().unwrap();
    assert!(
        journal
            .entries()
            .iter()
            .any(|entry| entry.author == subject.to_hex()),
        "the component's graph edit is attributed to its subject"
    );
    drop(journal);
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// The re-root heal: when the profile's root identity changes (the vault
/// superseding the unsealed stopgap key), stored certificates fail as
/// WrongRoot — and the adopt path re-issues from the grant projections
/// under the NEW root, preserving exactly the reviewed grant. No denizen
/// silently loses authority to a key migration.
#[test]
fn a_rerooted_profile_reissues_delegations_from_the_reviewed_projections() {
    use servitor::AuthorityProvider;

    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!("turnstone-reroot-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = app.data_root.join("keeper.lua");
    std::fs::write(&pack, "mere.open('mere://kept/note')").unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    let subject = app.denizens.residents.values().next().unwrap().subject;
    let navigate = crate::ring::Ring::Navigate.cap().unwrap();
    let session_ring = crate::ring::Ring::Session.cap().unwrap();

    // The profile re-roots: a NEW identity (the vault superseding the
    // stopgap seed). The old certificates on disk name the old root.
    let new_root = identity::InMemoryProvider::from_seed([77u8; 32]);
    let rebuilt = crate::denizen::rebuild(
        app.graph_runtimes.facets(),
        app.graph_runtimes.graph(),
        &app.session_dir(),
        &new_root,
    );
    assert_eq!(
        rebuilt.residents.len(),
        1,
        "the resident survives the migration"
    );
    assert!(
        rebuilt
            .authority
            .covers(subject, &navigate, servitor::Mode::Write),
        "the reviewed ring re-rooted under the new identity"
    );
    assert!(
        !rebuilt
            .authority
            .covers(subject, &session_ring, servitor::Mode::Write),
        "and the heal preserves the review exactly: nothing widens"
    );
    // Durable: the certificate file was rewritten under the new root, so
    // the NEXT adopt verifies without healing.
    let stored = crate::denizen::load_certs(&app.session_dir(), &subject.to_hex());
    assert!(!stored.is_empty());
    assert!(
        stored.iter().all(|signed| {
            signed.certificate.issuer
                == identity::IdentityProvider::master_public_key(&new_root).to_bytes()
        }),
        "the stored chain now roots at the new identity"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// Install is a signed delegation from the profile identity, and uninstall
/// REVOKES it (capability-model C4). The arc: install grants exactly the
/// reviewed rings, uninstall revokes them and un-resides the denizen, and
/// what it was authorized to do stops being authorized — without
/// destroying its node or its world.
#[test]
fn install_delegates_from_the_profile_identity_and_uninstall_revokes_it() {
    use servitor::AuthorityProvider;

    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!("turnstone-revoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    // The root identity is the profile's, not a constant.
    let root = identity::IdentityProvider::master_public_key(app.identity.as_ref()).to_bytes();
    assert_eq!(
        app.denizens.authority.root(),
        root,
        "rooted on the profile identity"
    );

    let pack = app.data_root.join("keeper.lua");
    std::fs::write(&pack, "mere.open('mere://kept/note')").unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    let (member, subject) = {
        let (&m, r) = app.denizens.residents.iter().next().unwrap();
        (m, r.subject)
    };
    let navigate = crate::ring::Ring::Navigate.cap().unwrap();
    let session_ring = crate::ring::Ring::Session.cap().unwrap();
    assert!(
        app.denizens
            .authority
            .covers(subject, &navigate, servitor::Mode::Write),
        "the reviewed ring is authorized by a verified certificate chain"
    );
    assert!(
        !app.denizens
            .authority
            .covers(subject, &session_ring, servitor::Mode::Write),
        "an unreviewed ring was never delegated"
    );
    let certs = crate::denizen::certs_path(&app.session_dir(), &subject.to_hex());
    assert!(certs.is_file(), "the signed delegations persisted");

    // Uninstall: revoke and un-reside.
    app.update(Action::UninstallDenizen { member });
    assert!(app.denizens.residents.is_empty(), "no longer resident");
    assert!(
        !app.denizens
            .authority
            .covers(subject, &navigate, servitor::Mode::Write),
        "the delegation is revoked, so the ring is no longer authorized"
    );
    assert!(
        pandect::read_denizen_binding(app.graph_runtimes.facets(), member).is_none(),
        "un-resided: the agency facet is gone"
    );
    assert!(
        !certs.is_file(),
        "and its certificates cannot resurrect it on adopt"
    );
    assert!(
        app.take_events()
            .iter()
            .any(|e| matches!(e, AppEvent::DenizenUninstalled(_))),
        "the uninstall is observable"
    );
    // Nothing was destroyed: the node and its borne world remain.
    assert!(
        app.graph_runtimes.graph().get_node_by_id(member).is_some(),
        "revoking authority does not delete the node"
    );
    assert!(
        crate::denizen::nested_log_path(&app.session_dir(), &subject.to_hex()).is_file(),
        "nor its world"
    );
    // And the palette no longer offers to run it.
    assert!(
        !app.denizen_actions()
            .iter()
            .any(|(label, _)| label.starts_with("Run ")),
        "the Run row is gone with the residency"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// Archive-never-orphan at the node tier: deleting a denizen node moves
/// its world file to the archive slot (nothing orphaned in the live dir,
/// nothing destroyed), the tombstone carries the world id + facet bundle,
/// and recovery restores full residency — world back live, binding facet
/// back, resident rebuilt.
#[test]
fn deleting_a_denizen_archives_its_world_and_recovery_restores_residency() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-bin-world-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = app.data_root.join("keeper.lua");
    std::fs::write(&pack, "mere.open('mere://kept/note')").unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    let (member, world_id, world_revision) = {
        let (&member, resident) = app.denizens.residents.iter().next().unwrap();
        (
            member,
            resident.subject.to_hex(),
            resident.nested.revision(),
        )
    };
    assert!(
        crate::denizen::nested_log_path(&app.session_dir(), &world_id).is_file(),
        "the world is live before the delete"
    );

    // Delete: the install left the denizen node selected.
    let fx = app.update(Action::DeleteFocusedNode);
    let record = fx
        .iter()
        .find_map(|e| match e {
            Effect::RecordDeleted { record } => Some(record.clone()),
            _ => None,
        })
        .expect("delete stages a bin record: {fx:?}");
    assert_eq!(record.node_id, member);
    assert_eq!(record.nested.as_deref(), Some(world_id.as_str()));
    assert!(
        record
            .facets
            .as_ref()
            .and_then(|f| f.get(pandect::DENIZEN_BINDING))
            .is_some(),
        "the tombstone carries the facet bundle incl. the binding"
    );
    assert!(
        !crate::denizen::nested_log_path(&app.session_dir(), &world_id).is_file(),
        "the live slot is empty"
    );
    assert!(
        crate::denizen::archived_world_path(&app.session_dir(), &world_id).is_file(),
        "the world moved to the archive slot, never orphaned"
    );
    assert!(
        app.denizens.residents.is_empty(),
        "the runtime entry left with the node"
    );
    assert!(
        pandect::read_denizen_binding(app.graph_runtimes.facets(), member).is_none(),
        "the live facets went to the tombstone"
    );

    // Recover: full residency returns.
    app.apply_update(Update::BinListed {
        records: vec![record],
    });
    app.update(Action::RecoverDeletedNode(member));
    assert!(
        crate::denizen::nested_log_path(&app.session_dir(), &world_id).is_file(),
        "the world is live again"
    );
    assert!(
        !crate::denizen::archived_world_path(&app.session_dir(), &world_id).is_file(),
        "the archive slot emptied"
    );
    assert!(
        pandect::read_denizen_binding(app.graph_runtimes.facets(), member).is_some(),
        "the binding facet restored"
    );
    let resident = app
        .denizens
        .residents
        .get(&member)
        .expect("the recovered denizen resides again");
    assert_eq!(
        resident.nested.revision(),
        world_revision,
        "the same world, not a fresh one"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// Empty-the-bin is athanor's oven on command: it lowers the EmptyRecycleBin
/// effect (the actor clears the store) only when there is something to
/// forget, and folding the port's empty answer clears the mirror.
#[test]
fn empty_recycle_bin_forgets_on_command() {
    let mut app = App::test_stub();
    // An empty bin is a no-op: no effect, no event (honest, no placebo).
    let fx = app.update(Action::EmptyRecycleBin);
    assert!(
        !fx.iter().any(|e| matches!(e, Effect::EmptyRecycleBin)),
        "nothing to empty lowers no effect: {fx:?}"
    );

    // Stage two records (as the bin port's answer would), then empty.
    app.apply_update(Update::BinListed {
        records: vec![
            crate::action::RemovedRecord {
                node_id: uuid::Uuid::new_v4(),
                url: "https://a.test".into(),
                title: None,
                tags: Vec::new(),
                deleted_at_ms: 2,
                nested: None,
                facets: None,
            },
            crate::action::RemovedRecord {
                node_id: uuid::Uuid::new_v4(),
                url: "https://b.test".into(),
                title: None,
                tags: Vec::new(),
                deleted_at_ms: 1,
                nested: None,
                facets: None,
            },
        ],
    });
    let fx = app.update(Action::EmptyRecycleBin);
    assert!(
        fx.iter().any(|e| matches!(e, Effect::EmptyRecycleBin)),
        "a non-empty bin lowers the clear effect: {fx:?}"
    );
    // The store's empty answer (folded as the drain would) clears the mirror.
    app.apply_update(Update::BinListed {
        records: Vec::new(),
    });
    assert!(
        app.removed.is_empty(),
        "the mirror is empty after the bin clears"
    );
}

/// The rung7_lens_ops receipt's exact op sequence, app-level: tear out
/// the roster, summon the trail beside it (in the lens), reweight, close
/// the ACTIVE pane. The close must remove the TRAIL (the summon made it
/// active), never the roster.
#[test]
/// The gloss-composite's add/remove: a Gloss pane starts as a bare
/// minimap, the palette offers pane-scoped section rows, toggling one
/// edits THAT LEAF (so it persists with the layout), and toggling again
/// removes it.
#[test]
fn composing_a_gloss_pane_toggles_sections_on_its_own_leaf() {
    let mut app = App::test_stub();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::GLOSS,
    )));
    let pane = app.active_pane.expect("the summoned gloss is active");

    // At base it is a minimap: no composed sections.
    let sections = |app: &App| match app.pane_content(pane) {
        Some(PaneContent::Gloss(cfg)) => cfg.sections.clone(),
        _ => panic!("the active pane is a Gloss"),
    };
    assert!(sections(&app).is_empty(), "base is a bare minimap");

    // The palette offers an ADD row per provider while it is active.
    let offered = app.session_actions();
    assert!(
        offered
            .iter()
            .any(|(label, _)| label == "Gloss: add section — Removed"),
        "pane-scoped add row is offered: {offered:?}"
    );

    // Toggling composes it onto the leaf, and persists (SaveSession).
    let fx = app.update(Action::TogglePaneSection {
        pane,
        section: "removed".to_string(),
    });
    assert_eq!(sections(&app), vec!["removed".to_string()]);
    assert!(
        fx.iter().any(|e| matches!(e, Effect::SaveSession)),
        "the composition persists with the layout: {fx:?}"
    );
    // Now the palette offers REMOVE for it.
    assert!(
        app.session_actions()
            .iter()
            .any(|(label, _)| label == "Gloss: remove section — Removed")
    );

    // Toggling again removes it, back to the bare minimap.
    app.update(Action::TogglePaneSection {
        pane,
        section: "removed".to_string(),
    });
    assert!(sections(&app).is_empty(), "toggled back off");
}

/// One catalog, offered in one order: the contextual rows LEAD the static
/// registry (a pending grant review must be first), and every consumer
/// reads this same list — the `>` lane filters it, the snapshot reports it,
/// and the automation runner resolves a label through it. Composing it
/// twice is how the runner and the palette come to disagree.
#[test]
fn available_actions_lead_with_the_contextual_rows() {
    let mut app = App::test_stub();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::GLOSS,
    )));
    let rows = app.available_actions();

    // The contextual rows (here, the active Gloss's composition rows) come
    // first; the static registry follows.
    let first_static = rows
        .iter()
        .position(|(label, _)| label == "Fit view")
        .expect("the static registry is in the catalog");
    let a_contextual = rows
        .iter()
        .position(|(label, _)| label.starts_with("Gloss: add section"))
        .expect("the active pane's rows are in the catalog");
    assert!(
        a_contextual < first_static,
        "contextual rows lead the static registry: {rows:?}"
    );

    // The catalog is exactly the two sources, nothing invented or dropped.
    assert_eq!(
        rows.len(),
        app.session_actions().len() + crate::action::palette_actions().len()
    );

    // And the snapshot reports THAT list, by label and in that order, so an
    // automation lane sees what a person would.
    let snap = crate::observe::snapshot(&app);
    let labels: Vec<String> = rows.into_iter().map(|(label, _)| label).collect();
    assert_eq!(snap.available_actions, labels);
}

#[test]
fn publishing_pane_is_a_summonable_window_control() {
    let mut app = App::test_stub();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::PUBLISHING,
    )));

    assert!(
        app.frisket
            .iter_leaves()
            .any(|(_, content, _)| content.kind_id().as_str() == crate::panes::kind::PUBLISHING)
    );
    assert!(
        crate::action::palette_actions()
            .iter()
            .any(|(label, action)| {
                *label == "Open Publishing pane"
                    && matches!(action, Action::SummonPane(id) if id.as_str() == crate::panes::kind::PUBLISHING)
            })
    );
}

#[test]
fn shared_knot_pane_is_a_summonable_reader_control() {
    let mut app = App::test_stub();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::SHARED_KNOT,
    )));

    assert!(
        app.frisket
            .iter_leaves()
            .any(|(_, content, _)| content.kind_id().as_str() == crate::panes::kind::SHARED_KNOT)
    );
    assert!(
        crate::action::palette_actions()
            .iter()
            .any(|(label, action)| {
                *label == "Open Shared Knot pane"
                    && matches!(action, Action::SummonPane(id) if id.as_str() == crate::panes::kind::SHARED_KNOT)
            })
    );
}

/// Composition ORDER is the config's order, so reordering is the same leaf
/// edit as add/remove: it moves within the stack, clamps at the ends
/// rather than wrapping, and the palette only offers a move that would do
/// something.
#[test]
fn moving_a_composed_section_reorders_that_leaf_and_clamps() {
    let mut app = App::test_stub();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::GLOSS,
    )));
    let pane = app.active_pane.expect("the summoned gloss is active");
    let sections = |app: &App| match app.pane_content(pane) {
        Some(PaneContent::Gloss(cfg)) => cfg.sections.clone(),
        _ => panic!("the active pane is a Gloss"),
    };
    let mv = |app: &mut App, section: &str, delta: i32| {
        app.update(Action::MovePaneSection {
            pane,
            section: section.to_string(),
            delta,
        })
    };

    // With ONE section there is nothing to reorder, so no move row.
    app.update(Action::TogglePaneSection {
        pane,
        section: "removed".to_string(),
    });
    assert!(
        !app.session_actions()
            .iter()
            .any(|(label, _)| label.starts_with("Gloss: move section")),
        "a lone section offers no move"
    );

    // Compose a second: it stacks BELOW, in config order.
    app.update(Action::TogglePaneSection {
        pane,
        section: "nodes".to_string(),
    });
    assert_eq!(sections(&app), vec!["removed", "nodes"]);

    // Moving it up swaps the stack, and persists with the layout.
    let fx = mv(&mut app, "nodes", -1);
    assert_eq!(sections(&app), vec!["nodes", "removed"]);
    assert!(
        fx.iter().any(|e| matches!(e, Effect::SaveSession)),
        "a reorder persists like any leaf edit: {fx:?}"
    );

    // At the top, up is a no-op: clamped, NOT wrapped to the bottom. It
    // reports no move, so the receipt cannot mistake it for one.
    let fx = mv(&mut app, "nodes", -1);
    assert_eq!(
        sections(&app),
        vec!["nodes", "removed"],
        "clamped at the top"
    );
    assert!(
        !fx.iter().any(|e| matches!(e, Effect::SaveSession)),
        "a no-op move saves nothing: {fx:?}"
    );
    // And the palette does not offer it.
    assert!(
        !app.session_actions()
            .iter()
            .any(|(label, _)| label == "Gloss: move section up — Nodes"),
        "no up-row on the first section"
    );

    // An id this pane has not composed moves nothing.
    let fx = mv(&mut app, "recent", 1);
    assert_eq!(sections(&app), vec!["nodes", "removed"]);
    assert!(!fx.iter().any(|e| matches!(e, Effect::SaveSession)));
}

/// Composition is a property of a PANE, not of the Gloss: the Overmap
/// composes the same sections, through the same renderer, with the same
/// leaf config and the same actions. Its palette rows name it, derived from
/// the pane's own tag rather than a second table.
#[test]
fn the_overmap_composes_sections_too() {
    let mut app = App::test_stub();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::OVERMAP,
    )));
    let pane = app.active_pane.expect("the summoned overmap is active");

    // A fresh Overmap composes nothing (its swatch fills the pane).
    assert_eq!(
        app.pane_content(pane).and_then(|c| c.composition()),
        Some(&crate::panes::PaneComposition::default())
    );
    // The palette offers ITS rows, named for IT.
    assert!(
        app.session_actions()
            .iter()
            .any(|(label, _)| label == "Overmap: add section — Removed"),
        "the pane-scoped rows name the pane: {:?}",
        app.session_actions()
    );

    // The same action composes it, onto ITS OWN leaf.
    app.update(Action::TogglePaneSection {
        pane,
        section: "removed".to_string(),
    });
    match app.pane_content(pane) {
        Some(PaneContent::Overmap(cfg)) => assert_eq!(cfg.sections, vec!["removed"]),
        other => panic!("expected a composed overmap, got {other:?}"),
    }
}

fn lens_ops_close_removes_the_summoned_pane() {
    let mut app = App::test_stub();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::ROSTER,
    )));
    app.update(Action::TearOutActivePane);
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::TRAIL,
    )));
    app.update(Action::SetActivePaneDivider(0.7));
    app.update(Action::CloseActivePane);
    let lens = app.lenses[0].as_ref().unwrap();
    let tags: Vec<&str> = lens.iter_leaves().map(|(_, c, _)| c.tag()).collect();
    assert!(
        tags.contains(&"roster") && !tags.contains(&"trail"),
        "close removes the summoned trail, not the roster: {tags:?}"
    );
}

/// The nav row (r3 owed): Back re-selects without refetching, Forward
/// redoes, a new open truncates the forward branch, and Reload refetches
/// the focused node and respawns its live content.
#[test]
fn back_forward_and_reload_flow_through_the_spine() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/a".to_string()));
    app.update(Action::OpenAddress("https://example.com/b".to_string()));
    // Back: the previous node re-selects, with NO fetch effect.
    let effects = app.update(Action::NavBack);
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::FetchPage { .. })),
        "Back never refetches: {effects:?}"
    );
    assert_eq!(
        app.graph_runtimes.focused_url(),
        Some("https://example.com/a")
    );
    // Forward redoes.
    app.update(Action::NavForward);
    assert_eq!(
        app.graph_runtimes.focused_url(),
        Some("https://example.com/b")
    );
    // Back then a new open: the forward branch truncates.
    app.update(Action::NavBack);
    app.update(Action::OpenAddress("https://example.com/c".to_string()));
    assert!(!app.history.can_forward(), "a new open truncates forward");
    assert!(app.history.can_back());
    // Reload: a fetch effect for the focused node; with live content, a
    // close + respawn pair.
    let node = app.graph_runtimes.focused_member().unwrap();
    app.apply_update(Update::ContentSpawned { node, facts: None });
    let effects = app.update(Action::Reload);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::FetchPage { .. }))
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::CloseContent { .. }))
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::SpawnContent { .. }))
    );
    let described: Vec<String> = app
        .take_events()
        .iter()
        .map(crate::observe::AppEvent::describe)
        .collect();
    assert!(described.iter().any(|e| e.starts_with("nav-back ")));
    assert!(described.iter().any(|e| e.starts_with("nav-forward ")));
    assert!(described.iter().any(|e| e.starts_with("reloaded ")));
}

/// The workbench lane end to end at the App tier: opening the focused
/// node tiles it, summons the Workbench pane, and spawns its content;
/// stacking collapses cells; closing empties honestly.
#[test]
fn workbench_actions_flow_through_the_spine() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("mere://alpha".to_string()));
    let a = app.graph_runtimes.focused_member().unwrap();
    let effects = app.update(Action::OpenInWorkbench);
    assert!(app.active_workbench().unwrap().is_tiled());
    assert_eq!(app.active_workbench().unwrap().tile_count(), 1);
    assert!(
        app.frisket
            .iter_leaves()
            .any(|(_, c, _)| matches!(c, PaneContent::Workbench)),
        "opening a tile summons the Workbench pane"
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::SpawnContent { .. })),
        "a tile wants live content: {effects:?}"
    );
    // Re-opening the same node adds nothing.
    app.update(Action::OpenInWorkbench);
    assert_eq!(app.active_workbench().unwrap().tile_count(), 1);
    // A second node tiles beside it; stacking collapses to one cell.
    app.update(Action::OpenAddress("mere://beta".to_string()));
    let b = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::OpenInWorkbench);
    assert_eq!(app.active_workbench().unwrap().slot_count(), 2);
    app.update(Action::WorkbenchStackOnto {
        dragged: b,
        target: a,
    });
    assert_eq!(app.active_workbench().unwrap().slot_count(), 1);
    assert_eq!(app.active_workbench().unwrap().tile_count(), 2);
    // Activate the buried tab; close the focused (beta) tile.
    app.update(Action::WorkbenchActivate(a));
    app.update(Action::CloseWorkbenchTile);
    assert_eq!(app.active_workbench().unwrap().tile_count(), 1);
    assert!(app.active_workbench().unwrap().has_tile(a));
}

/// Two graph panes may name different graph/Forme sources while followers
/// resolve the last graph-pane context in their own space. Camera/selection
/// and Workbench state stay behind those pane and Forme boundaries.
#[test]
fn graph_views_workbenches_and_followers_are_pane_scoped() {
    let mut app = App::test_stub();
    let graph_a = app.graph_runtimes.active_graph();
    let member_a = {
        let canvas = app.graph_runtimes.canvas_mut(graph_a).unwrap();
        let key = canvas.visit("mere://graph-a");
        canvas.graph().get_node(key).unwrap().id
    };

    let graph_b = crate::panes::GraphId::from_uuid(uuid::Uuid::from_u128(0xb));
    let mut canvas_b = mere::canvas::Canvas::new();
    let member_b = {
        let key = canvas_b.visit("mere://graph-b");
        canvas_b.graph().get_node(key).unwrap().id
    };
    app.graph_runtimes
        .activate_or_insert(graph_b, None, canvas_b);
    assert!(app.graph_runtimes.activate(graph_a));

    let first = crate::panes::PaneId(0);
    let second = crate::panes::PaneId(1);
    let follower = crate::panes::PaneId(2);
    app.frisket.root = crate::panes::PaneNode::Split {
        axis: crate::panes::SplitAxis::Horizontal,
        ratio: 0.5,
        first: Box::new(crate::panes::PaneNode::Leaf {
            pane_id: first,
            content: PaneContent::Orrery,
            graph_id: graph_a,
        }),
        second: Box::new(crate::panes::PaneNode::Split {
            axis: crate::panes::SplitAxis::Vertical,
            ratio: 0.5,
            first: Box::new(crate::panes::PaneNode::Leaf {
                pane_id: second,
                content: PaneContent::Orrery,
                graph_id: graph_b,
            }),
            second: Box::new(crate::panes::PaneNode::Leaf {
                pane_id: follower,
                content: PaneContent::Roster,
                graph_id: graph_b,
            }),
        }),
    };
    app.next_pane_id = 3;
    app.index_pane_spaces();

    assert!(
        app.with_graph_pane(first, |canvas| canvas.select_member(member_a))
            .unwrap()
    );
    assert_eq!(app.graph_pane_focused_member(first), Some(member_a));
    assert_eq!(
        app.follower_context(follower).unwrap().member,
        Some(member_a)
    );

    assert!(
        app.with_graph_pane(second, |canvas| canvas.select_member(member_b))
            .unwrap()
    );
    assert_eq!(app.graph_pane_focused_member(second), Some(member_b));
    assert_eq!(
        app.follower_context(follower).unwrap().member,
        Some(member_b)
    );

    app.workbench_for_pane_mut(first)
        .unwrap()
        .open_tile(member_a);
    app.workbench_for_pane_mut(second)
        .unwrap()
        .open_tile(member_b);
    assert!(app.workbench_for_pane(first).unwrap().has_tile(member_a));
    assert!(!app.workbench_for_pane(first).unwrap().has_tile(member_b));
    assert!(app.workbench_for_pane(second).unwrap().has_tile(member_b));
}
/// The browser-state sidecar (rung 6): content-on mirrors live truth at
/// refresh, prunes vanished nodes, and round-trips through the store.
#[test]
fn browser_states_refresh_and_round_trip() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/a".to_string()));
    let a = app.graph_runtimes.focused_member().unwrap();
    app.apply_update(Update::ContentSpawned {
        node: a,
        facts: None,
    });
    app.update(Action::OpenAddress("https://example.com/b".to_string()));
    app.refresh_browser_states();
    assert!(app.browser.get(a).is_some_and(|b| b.content_on));
    assert!(
        app.browser
            .get(app.graph_runtimes.focused_member().unwrap())
            .is_none(),
        "a node without content stays out of the sidecar"
    );
    // Round trip through the converged store: web.* facets in facets.json.
    let dir = std::env::temp_dir().join(format!("turnstone-bn-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut facets = pandect::NodeFacetStore::new();
    pandect::write_web_states(&mut facets, &app.browser);
    crate::session::save_node_facets(&dir, &facets);
    let reloaded = crate::session::load_node_facets(&dir).unwrap_or_default();
    let restored = pandect::read_web_states(&reloaded);
    assert!(restored.get(a).is_some_and(|b| b.content_on));
    // Content off -> the refresh clears the flag.
    app.content.note_closed(a);
    app.refresh_browser_states();
    assert!(
        !app.browser.get(a).is_some_and(|b| b.content_on),
        "closed content clears the flag on the next refresh"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The workbench sidecar round-trips through the persistence port,
/// pruned to present members (platen's canonical pair underneath).
#[test]
fn workbench_persists_and_restores_pruned() {
    let dir = std::env::temp_dir().join(format!("turnstone-wb-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
    let mut wb = mere::platen::Workbench::new();
    wb.ensure_tiled();
    wb.open_tile(a);
    wb.open_tile(b);
    crate::session::save_workbench(&dir, &wb);
    // Both present: both tiles come back.
    let present: std::collections::HashSet<_> = [a, b].into_iter().collect();
    let restored = crate::session::load_workbench(&dir, &present);
    assert_eq!(restored.tile_count(), 2);
    // b's node vanished between sessions: its tile is reconciled away.
    let present: std::collections::HashSet<_> = [a].into_iter().collect();
    let restored = crate::session::load_workbench(&dir, &present);
    assert_eq!(restored.tile_count(), 1);
    assert!(restored.has_tile(a));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Committing a find-lane node row selects without fetching.
#[test]
fn committing_a_node_row_selects_without_fetch_effects() {
    let mut app = App::test_stub();
    app.graph_runtimes.visit("https://example.com/meerkats");
    app.update(Action::OmnibarOpen { command: false });
    for c in "meer".chars() {
        app.update(Action::OmnibarChar(c));
    }
    let effects = app.update(Action::OmnibarCommit);
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::FetchPage { .. })),
        "selecting an existing node must not refetch: {effects:?}"
    );
    assert!(!app.omnibar.open);
}

#[test]
fn omnibar_transcript_freezes_its_open_context_and_repeats_it() {
    let mut app = App::test_stub();
    let original_session = app.session_id;
    app.update(Action::OmnibarOpen { command: false });
    app.update(Action::OmnibarInsert(
        "https://example.com/field-notes".into(),
    ));
    app.update(Action::OmnibarCommit);

    let first = app
        .shell_transcript()
        .entries()
        .next()
        .expect("a committed address enters the shell transcript")
        .clone();
    assert_eq!(first.target.context.session, Some(original_session));
    assert!(matches!(
        first.outcome,
        crate::shell_services::ShellOutcome::Completed { .. }
    ));

    // A changed host session does not retarget an already-composed command.
    app.session_id = crate::panes::SessionId::new();
    app.update(Action::RepeatShellEntry(first.id));
    let repeated = app
        .shell_transcript()
        .entries()
        .last()
        .expect("repeat makes a separately correlated attempt");
    assert_ne!(repeated.id, first.id);
    assert_eq!(repeated.target.context.session, Some(original_session));

    app.update(Action::OpenShellEntryTarget(first.id));
    assert_eq!(
        app.take_requested_shell_context(),
        Some(first.target),
        "A2 receives the original target rather than a canvas-derived replacement"
    );
}

/// The recall lane's ask: typing a needle lowers one RecallQuery, and the
/// answer becomes rows below the go row (search wiring W2).
#[test]
fn typing_asks_for_recall_and_the_answer_becomes_rows() {
    let mut app = App::test_stub();
    app.update(Action::OmnibarOpen { command: false });
    app.update(Action::OmnibarChar('r'));
    let effects = app.update(Action::OmnibarChar('s'));

    let asked: Vec<&String> = effects
        .iter()
        .filter_map(|e| match e {
            Effect::RecallQuery { query } => Some(query),
            _ => None,
        })
        .collect();
    assert_eq!(asked, vec!["rs"], "one ask, carrying the current needle");

    app.apply_update(Update::RecallHits {
        query: "rs".to_string(),
        hits: vec![crate::action::RecallHit {
            url: "https://rust-lang.org/".to_string(),
            title: Some("Rust".to_string()),
            at_ms: 42,
        }],
    });
    let rows = &app.omnibar.suggestions;
    assert!(
        rows.iter().any(|s| matches!(
            s,
            crate::ui::Suggestion::Recall { url, .. } if url == "https://rust-lang.org/"
        )),
        "the recalled page is offered: {rows:?}"
    );
}

/// Recall never displaces what the line already commits to: with an
/// address-shaped needle, the go row keeps the first-committed position and
/// the recalled pages sit under it.
#[test]
fn recall_never_outranks_the_typed_address() {
    let mut app = App::test_stub();
    app.update(Action::OmnibarOpen { command: false });
    for c in "rust-lang.org".chars() {
        app.update(Action::OmnibarChar(c));
    }
    app.apply_update(Update::RecallHits {
        query: "rust-lang.org".to_string(),
        hits: vec![crate::action::RecallHit {
            url: "https://docs.rs/".to_string(),
            title: None,
            at_ms: 42,
        }],
    });

    let rows = &app.omnibar.suggestions;
    let go = rows
        .iter()
        .position(|s| matches!(s, crate::ui::Suggestion::Go { .. }))
        .expect("an address-shaped needle offers its go row");
    let recalled = rows
        .iter()
        .position(|s| matches!(s, crate::ui::Suggestion::Recall { .. }))
        .expect("and the recalled page is offered too");
    assert!(go < recalled, "the typed address commits first: {rows:?}");
}

/// A late answer to text the user has typed past is dropped: the lane can
/// never show hits for a needle that is no longer in the line.
#[test]
fn superseded_recall_answers_drop() {
    let mut app = App::test_stub();
    app.update(Action::OmnibarOpen { command: false });
    app.update(Action::OmnibarChar('r'));
    app.update(Action::OmnibarChar('s'));
    app.update(Action::OmnibarChar('t'));

    app.apply_update(Update::RecallHits {
        query: "rs".to_string(),
        hits: vec![crate::action::RecallHit {
            url: "https://stale.example/".to_string(),
            title: None,
            at_ms: 1,
        }],
    });
    assert!(
        !app.omnibar
            .suggestions
            .iter()
            .any(|s| matches!(s, crate::ui::Suggestion::Recall { .. })),
        "the answer to 'rs' is not shown against 'rst'"
    );
}

/// The lane's footing inside the configured row budget: a graph full of
/// matches displaces its least-recent node rows rather than swallowing every
/// recalled page, and the total still honors the setting.
#[test]
fn recall_keeps_a_footing_inside_the_row_budget() {
    let mut state = crate::ui::OmnibarState {
        open: true,
        text: "node".to_string(),
        ..crate::ui::OmnibarState::default()
    };
    let mut canvas = mere::canvas::Canvas::new();
    for i in 0..6 {
        canvas.visit(&format!("https://node{i}.example/"));
    }
    let recall: Vec<crate::action::RecallHit> = (0..5)
        .map(|i| crate::action::RecallHit {
            url: format!("https://recalled{i}.example/"),
            title: None,
            at_ms: i,
        })
        .collect();

    crate::ui::recompute_suggestions_with_limit(&mut state, &canvas, &[], &recall, 7);

    assert!(state.suggestions.len() <= 7, "the configured budget holds");
    let recalled = state
        .suggestions
        .iter()
        .filter(|s| matches!(s, crate::ui::Suggestion::Recall { .. }))
        .count();
    assert!(
        recalled > 0,
        "a full find lane does not swallow recall: {:?}",
        state.suggestions
    );
    assert!(
        state
            .suggestions
            .iter()
            .any(|s| matches!(s, crate::ui::Suggestion::Node { .. })),
        "and node matches keep the larger share"
    );
}

/// The `>` actions lane searches intents, not pages: it asks for no recall,
/// and a needle narrowed below the floor drops the cached hits.
#[test]
fn the_actions_lane_and_short_needles_never_recall() {
    let mut app = App::test_stub();
    app.update(Action::OmnibarOpen { command: true });
    let effects = app.update(Action::OmnibarChar('f'));
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::RecallQuery { .. })),
        "the > lane asks for no page recall"
    );

    app.update(Action::OmnibarClose);
    app.update(Action::OmnibarOpen { command: false });
    app.update(Action::OmnibarChar('a'));
    app.update(Action::OmnibarChar('b'));
    app.apply_update(Update::RecallHits {
        query: "ab".to_string(),
        hits: vec![crate::action::RecallHit {
            url: "https://ab.example/".to_string(),
            title: None,
            at_ms: 1,
        }],
    });
    assert!(
        !app.recall.is_empty(),
        "hits cached for the two-char needle"
    );
    app.update(Action::OmnibarBackspace);
    assert!(
        app.recall.is_empty(),
        "narrowing below the floor drops the pages the line is no longer about"
    );
    assert!(
        !app.omnibar
            .suggestions
            .iter()
            .any(|s| matches!(s, crate::ui::Suggestion::Recall { .. })),
        "and the rows go with them in the same keystroke, not the next one"
    );
}

/// The row count is the configured maximum clamped by the window: a tall
/// window offers the whole setting, a short one offers what fits, and an
/// explicit setting is never raised by geometry.
#[test]
fn the_row_count_follows_the_viewport_under_the_configured_ceiling() {
    use crate::panes::ChromePlacement;
    use crate::ui::visible_row_limit;

    let tall = visible_row_limit(10, &ChromePlacement::Overlay, 1080.0, 1.0);
    assert_eq!(
        tall, 10,
        "a tall window offers the whole configured maximum"
    );

    let short = visible_row_limit(10, &ChromePlacement::Overlay, 400.0, 1.0);
    assert!(
        (3..10).contains(&short),
        "a short window offers fewer rows, not the ceiling: {short}"
    );

    let cramped = visible_row_limit(10, &ChromePlacement::Overlay, 120.0, 1.0);
    assert_eq!(cramped, 3, "a window with no room still offers the floor");

    // Zoom only bites once the rows stop fitting: a 1080px window still has
    // room for the whole ceiling at 2.5x, so compare inside a window where
    // the text size is what runs out of room.
    let plain = visible_row_limit(10, &ChromePlacement::Overlay, 600.0, 1.0);
    let zoomed = visible_row_limit(10, &ChromePlacement::Overlay, 600.0, 2.5);
    assert!(
        zoomed < plain,
        "bigger text means fewer rows in the same window: {zoomed} vs {plain}"
    );

    assert_eq!(
        visible_row_limit(1, &ChromePlacement::Overlay, 1080.0, 1.0),
        1,
        "geometry never raises a deliberate setting above itself"
    );
}

#[test]
fn configured_row_limit_applies_to_the_live_omnibar_projection() {
    let mut app = App::test_stub();
    let mut chrome = app.shell_chrome_config().clone();
    chrome.omnibar.row_limit = 1;
    app.set_shell_chrome_config(chrome);
    app.update(Action::OmnibarOpen { command: true });
    assert_eq!(app.omnibar.suggestions.len(), 1);
}

#[test]
fn live_settings_snapshot_reconfigures_the_running_chrome_value_once() {
    let settings = pandect::ApplicationSettings {
        theme_id: Some("theme:night".into()),
        theme_mode: Some("light".into()),
        ui_zoom: 1.5,
        shellbar_edge: pandect::ShellbarEdge::Top,
        shellbar_hidden: true,
        ..pandect::ApplicationSettings::default()
    };
    let snapshot = crate::settings_pane::ChromeSettings::from(&settings);
    let mut app = App::test_stub();

    assert!(app.apply_chrome_settings_snapshot(&snapshot));
    assert_eq!(
        app.shell_chrome_config().shellbar.placement,
        crate::panes::ChromePlacement::Docked(crate::panes::ChromeEdge::Top)
    );
    assert!(!app.shell_chrome_config().shellbar.visible);
    assert_eq!(
        app.shell_chrome_config().appearance.theme_id.as_deref(),
        Some("theme:night")
    );
    assert_eq!(
        app.shell_chrome_config().appearance.theme_mode,
        crate::shell_services::ThemeMode::Light
    );
    assert_eq!(app.shell_chrome_config().appearance.zoom(), 1.5);
    assert!(
        !app.apply_chrome_settings_snapshot(&snapshot),
        "unchanged snapshots do not schedule an unbounded redraw loop"
    );
}

#[test]
fn actor_fetched_body_is_retained_for_the_requested_content_spawn() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("gemini://capsule.test/".into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_requested(node);

    let effects = app.apply_update(Update::PageFetched {
        node,
        url: "gemini://capsule.test/".into(),
        result: Ok(crate::action::FetchedPage {
            content_type: Some("text/gemini".into()),
            body: "# One request".into(),
        }),
    });

    assert_eq!(
        app.content
            .fetched(node, "gemini://capsule.test/")
            .map(|document| document.body.as_str()),
        Some("# One request")
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::SpawnContent { node: actual, url }
            if *actual == node && url == "gemini://capsule.test/"
    )));
}

#[test]
fn ordinary_smolweb_input_navigates_and_refetches_with_a_percent_encoded_query() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("gemini://capsule.test/search".into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_requested(node);

    let effects = app.apply_update(Update::SmolwebInputRequested {
        node,
        url: "gemini://capsule.test/search".into(),
        input_url: "gemini://capsule.test/search".into(),
        prompt: "Search terms".into(),
        sensitive: false,
    });
    assert!(matches!(
        app.content.get(node),
        Some(crate::content::NodeContent::AwaitingInput)
    ));
    assert!(
        effects.iter().any(
            |effect| matches!(effect, Effect::CloseContent { node: actual } if *actual == node)
        )
    );
    assert!(matches!(
        &app.omnibar.mode,
        crate::ui::OmnibarMode::SmolwebInput(input)
            if input.prompt == "Search terms" && !input.sensitive
    ));

    app.update(Action::OmnibarInsert("two words/?".into()));
    let effects = app.update(Action::OmnibarCommit);
    let target = "gemini://capsule.test/search?two%20words%2F%3F";
    assert_eq!(app.graph_runtimes.focused_url(), Some(target));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::FetchPage { node: actual, url, owner_url, .. }
            if *actual == node && url == target && owner_url == target
    )));
    assert!(matches!(
        app.content.get(node),
        Some(crate::content::NodeContent::Requested)
    ));
}

#[test]
fn sensitive_smolweb_input_is_masked_and_never_enters_graph_or_events() {
    let mut app = App::test_stub();
    let owner = "gemini://capsule.test/login";
    app.update(Action::OpenAddress(owner.into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    let _ = app.take_events();
    app.apply_update(Update::SmolwebInputRequested {
        node,
        url: owner.into(),
        input_url: owner.into(),
        prompt: "Password".into(),
        sensitive: true,
    });

    app.update(Action::OmnibarInsert("hunter 2".into()));
    let snapshot = crate::observe::snapshot(&app);
    assert_eq!(snapshot.omnibar.text, "\u{2022}".repeat(8));
    assert!(!format!("{snapshot:?}").contains("hunter"));
    assert!(
        crate::a11y::a11y_lines(&app)
            .iter()
            .all(|line| !line.contains("hunter"))
    );
    assert!(
        app.recall_query.is_empty(),
        "private input never asks recall"
    );

    let effects = app.update(Action::OmnibarCommit);
    assert_eq!(app.graph_runtimes.focused_url(), Some(owner));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::FetchPage { node: actual, url, owner_url, .. }
            if *actual == node
                && url == "gemini://capsule.test/login?hunter%202"
                && owner_url == owner
    )));
    let described: Vec<_> = app
        .take_events()
        .into_iter()
        .map(|event| event.describe())
        .collect();
    assert!(described.iter().all(|event| !event.contains("hunter")));
}

#[test]
fn gemini_identity_is_confirmed_once_and_reused_only_for_its_capsule() {
    let mut app = App::test_stub();
    let owner = "gemini://capsule.test/account";
    let target = "gemini://capsule.test/private";
    app.update(Action::OpenAddress(owner.into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_requested(node);

    let effects = app.apply_update(Update::GeminiIdentityRequested {
        node,
        url: owner.into(),
        identity_url: target.into(),
        prompt: "Identity required".into(),
    });
    assert!(matches!(
        app.content.get(node),
        Some(crate::content::NodeContent::AwaitingIdentity)
    ));
    assert!(
        effects.iter().any(
            |effect| matches!(effect, Effect::CloseContent { node: actual } if *actual == node)
        )
    );
    assert!(matches!(
        &app.omnibar.mode,
        crate::ui::OmnibarMode::GeminiIdentity(input)
            if input.identity_url == target && input.prompt == "Identity required"
    ));
    assert_eq!(app.graph_runtimes.focused_url(), Some(owner));

    let effects = app.update(Action::OmnibarCommit);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::FetchPage {
            node: actual,
            url,
            owner_url,
            identity: Some(identity),
        } if *actual == node
            && url == target
            && owner_url == owner
            && identity.origin() == "gemini://capsule.test"
    )));
    assert!(matches!(
        app.content.get(node),
        Some(crate::content::NodeContent::Requested)
    ));
    assert_eq!(app.graph_runtimes.focused_url(), Some(owner));

    let same_capsule =
        app.fetch_page_effect(node, "gemini://capsule.test/again".into(), owner.into());
    assert!(matches!(
        same_capsule,
        Effect::FetchPage {
            identity: Some(_),
            ..
        }
    ));
    let other_capsule = app.fetch_page_effect(
        node,
        "gemini://other.test/".into(),
        "gemini://other.test/".into(),
    );
    assert!(matches!(
        other_capsule,
        Effect::FetchPage { identity: None, .. }
    ));

    let described: Vec<_> = app
        .take_events()
        .into_iter()
        .map(|event| event.describe())
        .collect();
    assert!(
        described
            .iter()
            .any(|event| event.contains("gemini-identity-requested"))
    );
    assert!(
        described
            .iter()
            .any(|event| event.contains("gemini-identity-bound"))
    );
}
