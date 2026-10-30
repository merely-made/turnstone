// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
    let mere::canvas::dynamics_spec::Realization::Integrate { damping } =
        app.graph_runtimes.dynamics_spec().unwrap().realization;
    assert_eq!(
        damping,
        Some(f64::from(5.5_f32)),
        "the earlier scene.physics_damping facet carried into the scene"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

#[test]
fn boot_launch_fetch_precedes_the_restored_content_spawn() {
    let root = std::env::temp_dir().join(format!("turnstone-boot-fetch-{}", uuid::Uuid::new_v4()));
    let url = "7fc8950ec4e50695be76eddc8e539ee8:/page/turnstone.mu";
    let mut app = App::test_stub();
    app.data_root = root.clone();
    app.sessions = pandect::ManifestStore::with_root(session::sessions_root(&root));
    let session_id = crate::panes::SessionId::new();
    let mut manifest = pandect::GraphSessionManifest::new(session_id, crate::panes::GraphId::new());
    manifest.storage_path = Some(session::session_dir(&root, session_id));
    app.sessions.insert(manifest);
    app.sessions.flush_dirty().unwrap();

    let key = app.graph_runtimes.visit(url);
    let node = app.graph_runtimes.graph().get_node(key).unwrap().id;
    let session_dir = session::session_dir(&root, session_id);
    session::save_session_graph(&session_dir, app.graph_runtimes.graph());
    let mut facets = pandect::NodeFacetStore::new();
    let mut browser = pandect::browser_node_state::BrowserNodeStates::new();
    browser.entry(node).content_on = true;
    pandect::write_web_states(&mut facets, &browser);
    session::save_node_facets(&session_dir, &facets);

    // This is the exact boot sequence after profile and identity setup. The
    // restored content produces SpawnContent, then the command-line address
    // is applied to that same persisted node.
    let mut effects = app.adopt_session(session_id);
    app.apply_launch_address(url, &mut effects);

    let fetches: Vec<_> = effects
        .iter()
        .enumerate()
        .filter_map(|(index, effect)| match effect {
            Effect::FetchPage {
                request,
                node: actual,
                url: actual_url,
                ..
            } if *actual == node && actual_url == url => Some((index, *request)),
            _ => None,
        })
        .collect();
    assert_eq!(
        fetches.len(),
        1,
        "one launch address has one fetch identity"
    );
    let spawn = effects
        .iter()
        .position(|effect| {
            matches!(
                effect,
                Effect::SpawnContent { node: actual, url: actual_url }
                    if *actual == node && actual_url == url
            )
        })
        .expect("restored content-on node respawns");
    assert!(
        fetches[0].0 < spawn,
        "the pending request must exist before SpawnContent: {effects:?}"
    );
    assert_eq!(app.content.active_fetch(node), Some(fetches[0].1));
    let _ = std::fs::remove_dir_all(root);
}

/// The B1 residency arc, headless: a staged install shows its ask, the
/// confirm mints the participant (node + binding facet + gate-projected grant
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
        app.personae_root(),
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

/// Vault lock ruling 35: with the vault locked the identity is pending, not
/// replaced. Participants keep residing but nothing they hold verifies, a new
/// install refuses rather than signing with a fallback key, nothing on disk
/// is rewritten, and the unlocked root brings their authority back.
#[test]
fn a_locked_vault_holds_participants_until_the_root_is_adopted() {
    use servitor::AuthorityProvider;

    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!("turnstone-pending-{}", std::process::id()));
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
    let admissions = crate::resident_admission::path(&app.session_dir());
    let admitted = std::fs::read(&admissions).unwrap();

    // The vault locks: the app runs with its identity pending.
    let root = app.identity.take().expect("the stub starts unlocked");
    app.denizens = crate::denizen::rebuild(
        app.graph_runtimes.facets(),
        app.graph_runtimes.graph(),
        &app.session_dir(),
        app.personae_root(),
    );
    assert_eq!(app.denizens.residents.len(), 1, "still resident");
    assert!(
        !app.denizens
            .authority
            .covers(subject, &navigate, servitor::Mode::Write),
        "no chain verifies without the root"
    );
    assert_eq!(
        std::fs::read(&admissions).unwrap(),
        admitted,
        "nothing rewritten"
    );
    assert_eq!(app.personae_root(), None);
    assert_eq!(
        app.gemini_identity_for("gemini://capsule.example/"),
        Ok(None)
    );

    let other = app.data_root.join("other.lua");
    std::fs::write(&other, "mere.open('mere://kept/other')").unwrap();
    app.update(Action::InstallDenizen {
        path: other.display().to_string(),
    });
    app.take_events();
    app.update(Action::ConfirmInstallDenizen);
    assert_eq!(app.denizens.residents.len(), 1, "no install while pending");
    assert!(app.take_events().iter().any(|event| matches!(
        event,
        AppEvent::DenizenRefused(reason) if reason == crate::denizen::PENDING_IDENTITY
    )));

    // The vault unlocks: the same root returns, and with it the authority.
    app.adopt_profile_root(root.clone());
    assert!(
        app.denizens
            .authority
            .covers(subject, &navigate, servitor::Mode::Write),
        "the stored chain verifies under the adopted root"
    );
    assert!(
        app.take_events()
            .iter()
            .any(|event| matches!(event, AppEvent::ProfileIdentityUnlocked(_)))
    );
    assert!(
        app.adopt_profile_root(root).is_empty(),
        "a second root for the same run is ignored"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// The gate refuses a participant's petition outside its granted scope, and
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
