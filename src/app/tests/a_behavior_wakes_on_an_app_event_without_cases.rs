// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
/// spine, and the journal recording it under the participant rather than the user.
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
    let expected_author = participant_author(resident);
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
        .filter(|entry| entry.author == expected_author)
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
    let expected_author = participant_author(resident);

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
        journal
            .entries()
            .iter()
            .any(|e| e.author == expected_author),
        "the captured edit reads back attributed to the subject"
    );
    assert_eq!(
        journal.author(),
        &mere::kernel::graph::Author::user(),
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
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    app.sessions = pandect::ManifestStore::with_root(session::sessions_root(&app.data_root));
    let donor_container = uuid::Uuid::from_u128(0xd0);
    app.sessions.insert(pandect::GraphSessionManifest::new(
        app.session_id,
        crate::panes::GraphId::from_uuid(donor_container),
    ));
    std::fs::create_dir_all(app.session_dir()).unwrap();

    // A two-node connected component plus a disconnected bystander.
    // Use the pane's visit lane for both opens so its saved selection carries
    // the first member into the second open's browse-trail relation.
    app.update(Action::OpenAddress("https://fork.example/a".to_string()));
    let a_id = app
        .graph_runtimes
        .graph()
        .get_node_by_url("https://fork.example/a")
        .unwrap()
        .1
        .id;
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
    app.graph_runtimes.set_physics_damping(4.75);
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
        fork_manifest
            .picked_from
            .as_ref()
            .map(|picked| picked.session),
        Some(donor_session),
        "a torn-out session records where it was picked from"
    );
    assert_eq!(
        fork_manifest
            .picked_from
            .as_ref()
            .map(|picked| picked.container),
        Some(donor_container)
    );
    assert_eq!(fork_manifest.parent_session, None, "a pick is not a fork");
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
    let scene =
        crate::scene_state::read_scene(&fork_facets, *fork_manifest.root_graph_id.as_uuid())
            .expect("the donor scene rode the carry")
            .expect("and reads");
    assert!(
        scene.cartography.members().contains(&fork_a),
        "the donor layout rode the carry under the remapped ids"
    );
    assert!(
        !scene.cartography.members().contains(&a_id),
        "and no donor id came with it"
    );
    let web = pandect::read_web_states(&fork_facets);
    assert!(
        web.get(fork_a).is_some_and(|s| s.content_on),
        "web.content carried onto the remapped id"
    );
    let damping = mere::canvas::saved_scene::scene_spec(&scene)
        .unwrap()
        .realization;
    let mere::canvas::dynamics_spec::Realization::Integrate { damping } = damping;
    assert_eq!(
        damping,
        Some(f64::from(4.75_f32)),
        "the scene carried donor-container -> fork-container"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// The world-carry: forking from a participant node re-bears its nested graph
/// on the fork's copy AND copies the world file into the fork's session
/// dir — donor and fork hold independent worlds thereafter (the kernel
/// copy alone would leave the fork's participant un-resided).
#[test]
fn fork_carries_denizen_worlds_as_real_copies() {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    app.sessions = pandect::ManifestStore::with_root(session::sessions_root(&app.data_root));
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
        crate::denizen::rebuild(&fork_facets, &fork_graph, &fork_dir, app.personae_root());
    assert_eq!(rebuilt.residents.len(), 1, "the fork's denizen resides");
    assert!(rebuilt.legacy_heals.is_empty());
    let fork_member = rebuilt.residents.keys().next().copied().unwrap();
    assert_eq!(
        rebuilt.admissions.get(fork_member).unwrap().binding.id,
        crate::resident_admission::resident_id(fork_member),
        "the fork remaps the host instance identity with its new graph node"
    );
    assert!(
        !servitor::AuthorityProvider::covers(
            &rebuilt.authority,
            rebuilt.residents.values().next().unwrap().subject,
            &crate::denizen::read_cap(),
            servitor::Mode::Read,
        ),
        "copying graph state does not silently copy or mint delegated authority"
    );
    assert_eq!(
        rebuilt.residents.values().next().unwrap().nested.revision(),
        donor_revision,
        "the carried world is the donor's world, bit-for-bit at fork time"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}
