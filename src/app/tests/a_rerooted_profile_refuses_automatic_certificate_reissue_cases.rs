// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A changed profile root cannot use a grant projection to mint replacement
/// authority. The resident remains visible, but its old chain is refused until
/// an owner explicitly reviews and installs again.
#[test]
fn a_rerooted_profile_refuses_automatic_certificate_reissue() {
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

    // The profile re-roots: a NEW identity (djinn speaking as another
    // persona). The old certificates on disk name the old root.
    let new_root = identity::InMemoryProvider::from_seed([77u8; 32]);
    let rebuilt = crate::denizen::rebuild(
        app.graph_runtimes.facets(),
        app.graph_runtimes.graph(),
        &app.session_dir(),
        Some(identity::IdentityProvider::master_public_key(&new_root).to_bytes()),
    );
    assert_eq!(
        rebuilt.residents.len(),
        1,
        "the resident remains inspectable after the migration"
    );
    assert!(
        !rebuilt
            .authority
            .covers(subject, &navigate, servitor::Mode::Write),
        "a mismatched chain cannot be silently replaced from projections"
    );
    assert!(
        !rebuilt
            .authority
            .covers(subject, &session_ring, servitor::Mode::Write),
        "and the heal preserves the review exactly: nothing widens"
    );
    // Durable: the original chain remains evidence, not material for an
    // automatic authority repair.
    let stored = crate::denizen::load_certs(&app.session_dir(), &subject.to_hex());
    assert!(!stored.is_empty());
    assert!(
        stored.iter().all(|signed| signed.certificate.issuer
            != identity::IdentityProvider::master_public_key(&new_root).to_bytes()),
        "the old chain is retained intact rather than reissued"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

#[test]
fn corrupt_admission_state_refuses_install_without_replacing_tombstones() {
    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!(
        "turnstone-corrupt-admission-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir().join("denizens")).unwrap();
    std::fs::write(
        crate::resident_admission::path(&app.session_dir()),
        "{ malformed",
    )
    .unwrap();
    let pack = app.data_root.join("keeper.lua");
    std::fs::write(&pack, "mere.open('mere://kept/note')").unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    assert!(app.denizens.residents.is_empty());
    assert!(crate::resident_admission::load(&app.session_dir()).is_err());
    let _ = std::fs::remove_dir_all(&app.data_root);
}

#[test]
fn swapped_script_body_is_refused_before_runtime_execution() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-swapped-body-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = app.data_root.join("keeper.lua");
    std::fs::write(&pack, "mere.open('mere://kept/note')").unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    let member = *app.denizens.residents.keys().next().unwrap();
    app.graph_runtimes
        .edit_facets(|facets| {
            facets.set(
                member,
                chartulary::FacetId::new(crate::denizen::SCENARIO_SOURCE_FACET),
                serde_json::json!("mere.open('mere://swapped')"),
                &chartulary::AcceptAll,
            )
        })
        .unwrap()
        .unwrap();
    app.update(Action::RunDenizen { member });
    assert!(app.take_events().iter().any(
        |event| matches!(event, AppEvent::DenizenRefused(reason) if reason.contains("body changed"))
    ));
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// Install is a signed delegation from the profile identity, and uninstall
/// REVOKES it (capability-model C4). The arc: install grants exactly the
/// reviewed rings, uninstall revokes them and un-resides the participant, and
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
    let root = app.personae_root().unwrap();
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

/// Archive-never-orphan at the node tier: deleting a participant node moves
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

    // Delete: the install left the participant node selected.
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

    // New Gloss panes compose Downloads beside their minimap.
    let sections = |app: &App| match app.pane_content(pane) {
        Some(PaneContent::Gloss(cfg)) => cfg.sections.clone(),
        _ => panic!("the active pane is a Gloss"),
    };
    assert_eq!(sections(&app), vec!["downloads"]);

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
    assert_eq!(
        sections(&app),
        vec!["downloads".to_string(), "removed".to_string()]
    );
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

    // Removing one section retains the initial Downloads configuration.
    app.update(Action::TogglePaneSection {
        pane,
        section: "removed".to_string(),
    });
    assert_eq!(sections(&app), vec!["downloads"]);
    let fx = app.update(Action::TogglePaneSection {
        pane,
        section: "downloads".into(),
    });
    assert!(sections(&app).is_empty(), "Downloads can be removed too");
    assert!(
        fx.iter()
            .any(|effect| matches!(effect, Effect::SaveSession))
    );
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
        .position(|(label, _)| label == "Fit to view")
        .expect("the static registry is in the catalog");
    let a_contextual = rows
        .iter()
        .position(|(label, _)| label.starts_with("Gloss: add section"))
        .expect("the active pane's rows are in the catalog");
    assert!(
        a_contextual < first_static,
        "contextual rows lead the static registry: {rows:?}"
    );

    // The catalog is exactly its sources, nothing invented or dropped: the
    // session rows, the situational place rows, then the static registry.
    assert_eq!(
        rows.len(),
        app.session_actions().len()
            + app.place_founding_actions().len()
            + crate::action::palette_actions().len()
    );

    // And the snapshot reports THAT list, by label and in that order, so an
    // automation lane sees what a person would.
    let snap = crate::observe::snapshot(&app);
    let labels: Vec<String> = rows.into_iter().map(|(label, _)| label).collect();
    assert_eq!(snap.available_actions, labels);
}
