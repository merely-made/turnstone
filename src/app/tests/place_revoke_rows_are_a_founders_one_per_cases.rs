// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// T1: a founder is offered one revoke row per other member while its place
/// is open, a row lowers the worker's revoke command, and everywhere else the
/// rows are absent and the action refuses out loud.
#[test]
fn place_revoke_rows_are_a_founders_one_per_other_member() {
    use crate::place::PlaceMember;
    use crate::place::PlaceMemberAccess::{Manage, Read, Write};

    let revoke_rows = |app: &App| -> Vec<(String, Action)> {
        app.available_actions()
            .into_iter()
            .filter(|(label, _)| label.starts_with("Revoke place member"))
            .collect()
    };
    let mut app = App::test_stub();
    let local = app.personae_root().unwrap();
    let with_local = |access| {
        vec![
            PlaceMember {
                root: local,
                access,
            },
            PlaceMember {
                root: [0x22; 32],
                access: Write,
            },
            PlaceMember {
                root: [0x33; 32],
                access: Read,
            },
        ]
    };

    // Personal: no rows, and the action refuses.
    assert!(revoke_rows(&app).is_empty());
    assert_eq!(
        app.update(Action::RevokePlaceMember { member: [0x22; 32] }),
        vec![Effect::Redraw]
    );
    assert!(app.take_events().iter().any(|event| matches!(
        event,
        crate::observe::AppEvent::PlaceRefused(reason)
            if reason == "open a place before revoking a member"
    )));

    // A founder: one row per other member, never itself.
    app.place = open_place(local, with_local(Manage));
    let rows = revoke_rows(&app);
    assert_eq!(
        rows,
        vec![
            (
                "Revoke place member 22222222 (writer)".to_string(),
                Action::RevokePlaceMember { member: [0x22; 32] },
            ),
            (
                "Revoke place member 33333333 (reader)".to_string(),
                Action::RevokePlaceMember { member: [0x33; 32] },
            ),
        ]
    );
    for (label, _) in &rows {
        assert!(
            crate::ui::chrome_row_width(label) <= crate::ui::ROW_TEXT_BUDGET,
            "{label}"
        );
    }

    // A writer in the same place, or a founder alone, is offered none.
    app.place = open_place(local, with_local(Write));
    assert!(revoke_rows(&app).is_empty());
    app.place = open_place(
        local,
        vec![PlaceMember {
            root: local,
            access: Manage,
        }],
    );
    assert!(revoke_rows(&app).is_empty());

    // A place that is not open offers none and refuses.
    app.place = crate::place::PlaceState::Degraded {
        binding: open_place(local, Vec::new()).binding().unwrap().clone(),
        generation: 5,
        error: "offline".into(),
    };
    assert!(revoke_rows(&app).is_empty());
    assert_eq!(
        app.update(Action::RevokePlaceMember { member: [0x22; 32] }),
        vec![Effect::Redraw]
    );
    assert!(matches!(
        app.take_events().last(),
        Some(crate::observe::AppEvent::PlaceRefused(_))
    ));

    // The row lowers the worker command; the worker's refusal is a place
    // refusal and changes nothing.
    app.place = open_place(local, with_local(Manage));
    let effects = app.update(Action::RevokePlaceMember { member: [0x22; 32] });
    let request = match effects.as_slice() {
        [
            Effect::RunPlaceCommand {
                generation: 5,
                request,
                command: crate::place::worker::PlaceCommand::RevokeMember { member },
                ..
            },
        ] if *member == [0x22; 32] => *request,
        other => panic!("revoking lowers one worker command: {other:?}"),
    };
    app.apply_update(Update::PlaceCommandDone {
        session: app.session_id,
        generation: 5,
        request,
        result: Err("only a profile that manages this place can revoke a member".into()),
    });
    assert!(app.take_events().iter().any(|event| matches!(
        event,
        crate::observe::AppEvent::PlaceRefused(reason) if reason.contains("manages this place")
    )));
    assert_eq!(revoke_rows(&app).len(), 2);
    assert_eq!(
        crate::ring::ring_of(&Action::RevokePlaceMember { member: [0x22; 32] }),
        crate::ring::Ring::HostOnly
    );
}

/// T3: the writer invitation prompt reads an optional ` for <n><s|m|h|d>`
/// from the right, so a path with spaces survives and no suffix means no
/// expiry; a reader invitation with a suffix, or a zero lifetime, is refused.
#[test]
fn place_invite_lifetime_suffix_is_read_from_the_right() {
    use crate::action::PlaceArtifactKind;
    use crate::place::PlaceInviteAccess::{Reader, Writer};

    let mut app = App::test_stub();
    let local = app.personae_root().unwrap();
    app.place = open_place(local, Vec::new());
    let commit = |app: &mut App, begin: Action, line: &str| -> Vec<Effect> {
        app.update(begin);
        app.omnibar.text = line.to_string();
        app.update(Action::OmnibarCommit)
    };
    let reads = |effects: &[Effect]| -> Vec<(String, PlaceArtifactKind)> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::ReadPlaceArtifact { path, kind } => Some((path.clone(), *kind)),
                _ => None,
            })
            .collect()
    };

    for (line, path, lifetime_ms) in [
        ("offer.json for 90s", "offer.json", Some(90_000)),
        ("offer.json for 10m", "offer.json", Some(600_000)),
        ("offer.json for 2h", "offer.json", Some(7_200_000)),
        ("offer.json for 7d", "offer.json", Some(604_800_000)),
        (
            "C:/my offers/for you.json for 10m",
            "C:/my offers/for you.json",
            Some(600_000),
        ),
        (
            "C:/my offers/offer for you.json",
            "C:/my offers/offer for you.json",
            None,
        ),
        ("offer.json", "offer.json", None),
    ] {
        assert_eq!(
            reads(&commit(&mut app, Action::BeginInviteToPlace, line)),
            vec![(
                path.to_string(),
                PlaceArtifactKind::Prekey {
                    access: Writer,
                    lifetime_ms,
                },
            )],
            "{line}"
        );
    }

    // The read hands the lifetime on to the worker unchanged.
    let effects = app.update(Action::InviteToPlaceWithPrekey {
        prekey: vec![4, 5, 6],
        out: "offer.json.invite.json".into(),
        access: Writer,
        lifetime_ms: Some(600_000),
    });
    assert!(matches!(
        effects.as_slice(),
        [Effect::InvitePlace {
            access: Writer,
            lifetime_ms: Some(600_000),
            ..
        }]
    ));

    // A reader invitation carries no grant, so a lifetime on it is refused.
    app.take_events();
    let effects = commit(
        &mut app,
        Action::BeginInviteToPlaceAsReader,
        "offer.json for 10m",
    );
    assert!(reads(&effects).is_empty(), "{effects:?}");
    assert!(app.take_events().iter().any(|event| matches!(
        event,
        crate::observe::AppEvent::PlaceRefused(reason)
            if reason == "a reader invitation carries no grant to bound"
    )));
    assert_eq!(
        reads(&commit(
            &mut app,
            Action::BeginInviteToPlaceAsReader,
            "offer.json"
        )),
        vec![(
            "offer.json".to_string(),
            PlaceArtifactKind::Prekey {
                access: Reader,
                lifetime_ms: None,
            },
        )]
    );

    // A zero lifetime would issue a grant already expired.
    let effects = commit(&mut app, Action::BeginInviteToPlace, "offer.json for 0s");
    assert!(reads(&effects).is_empty(), "{effects:?}");
    assert!(app.take_events().iter().any(|event| matches!(
        event,
        crate::observe::AppEvent::PlaceRefused(reason)
            if reason == "a grant lifetime must be longer than zero"
    )));

    let hint = crate::ui::PlacePrompt::Invite.hint();
    assert!(hint.contains(" for "), "the hint names the suffix: {hint}");
    assert!(
        crate::ui::chrome_row_width(hint) <= crate::ui::ROW_TEXT_BUDGET,
        "{hint}"
    );
}

/// T2: an invitation refused as expired is reported with its expiry time, in
/// the status line and in the place-refused event.
#[test]
fn an_expired_invitation_is_reported_with_its_time() {
    let mut app = App::test_stub();
    app.place = crate::place::PlaceState::Joining { generation: 3 };
    // The worker's wording for an invitation whose bound was 1050 ms.
    let refusal = format!("invitation expired at {}", crate::place::utc_ms(1_050));
    assert_eq!(refusal, "invitation expired at 1970-01-01T00:00:01Z");

    app.apply_update(Update::PlaceJoined {
        session: app.session_id,
        generation: 3,
        result: Err(refusal.clone()),
    });
    let lines = app.place.status_lines();
    assert_eq!(
        lines,
        vec!["Place not joined: invitation expired at 1970-01-01T00:00:01Z".to_string()]
    );
    assert!(crate::ui::chrome_row_width(&lines[0]) <= crate::ui::ROW_TEXT_BUDGET);
    let events: Vec<String> = app
        .take_events()
        .iter()
        .map(crate::observe::AppEvent::describe)
        .collect();
    assert!(
        events.contains(&"place-refused invitation expired at 1970-01-01T00:00:01Z".to_string()),
        "{events:?}"
    );
}

/// DR-C receipt through the production custody binding, rather than a fixture root.
#[test]
#[ignore = "run mere/scripts/dr_c_receipts.py against the isolated receipt keeper"]
fn dr_c_turnstone_identity_stays_pending() {
    let mode = std::env::var("DR_C_RECEIPT_MODE").expect("receipt mode");
    assert!(mode == "absent" || mode == "locked");
    if mode == "locked" {
        let mut client = djinn::door::custody_client::BlockingCustodyClient::open(
            djinn::door::app_admission::AppId::new(crate::identity::CUSTODY_APP),
        )
        .unwrap();
        let status = client.status().unwrap();
        assert_eq!(
            status.lock,
            dramatis::view::VaultLockView::Locked
        );
        assert!(status.persona_public_key.is_none());
        assert!(!client.roster().unwrap().entries.is_empty());
    }
    let temp = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(temp.path().to_path_buf());
    app.identity = crate::identity::bind();
    assert!(
        app.identity.is_none(),
        "Turnstone must not bind a fallback root"
    );
    app.denizens = crate::denizen::Denizens::pending();
    assert_eq!(app.personae_root(), None);
    assert_eq!(app.gemini_identity_for("gemini://dr-c.example/"), Ok(None));
    let pack = temp.path().join("pending.lua");
    std::fs::write(&pack, "mere.open('mere://receipt/public')").unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.take_events();
    app.update(Action::ConfirmInstallDenizen);
    assert!(
        app.denizens.residents.is_empty(),
        "pending must not sign an install"
    );
    assert!(app.take_events().iter().any(|event| matches!(event,
        AppEvent::DenizenRefused(reason) if reason == crate::denizen::PENDING_IDENTITY)));
    assert!(
        !crate::resident_admission::path(&app.session_dir()).exists(),
        "no authority record may be written while pending"
    );
    assert_eq!(
        std::fs::read(pack).unwrap(),
        b"mere.open('mere://receipt/public')"
    );
}

#[cfg(feature = "peers")]
#[test]
fn place_contact_refresh_is_local_scoped_and_rechecks_the_request_session() {
    let mut app = App::test_stub();
    app.update(Action::BeginRefreshPlaceContact);
    assert!(!app.omnibar.open);
    let binding = crate::place::worker::tests::binding(0x83);
    app.place = crate::place::PlaceState::Offline {
        binding: binding.clone(),
        generation: 4,
        snapshot: Default::default(),
    };
    app.next_place_generation = 4;
    app.update(Action::BeginRefreshPlaceContact);
    assert_eq!(
        app.omnibar.mode,
        crate::ui::OmnibarMode::Place(crate::ui::PlacePrompt::RefreshContact)
    );
    app.omnibar.text = "current-card.json".into();
    let effects = app.update(Action::OmnibarCommit);
    assert!(matches!(&effects[..], [Effect::ReadPlaceArtifact {
        path, kind: crate::action::PlaceArtifactKind::Contact { session, generation: 4 },
    }, Effect::Redraw] if path == "current-card.json" && *session == app.session_id));
    let card = crate::place::PlaceCardV1 {
        version: crate::place::PLACE_CARD_VERSION,
        binding: binding.clone(),
        founder_root: "01".repeat(32),
        rendezvous: vec![crate::place::invite::RendezvousV1 {
            carrier: crate::place::invite::P2PANDA_ENDPOINT_TICKET.into(),
            hint: "new-endpoint".into(),
        }],
    };
    let before = app.place.clone();
    assert!(
        app.update(Action::ReconnectPlaceWithCard {
            session: app.session_id,
            generation: 3,
            card: Box::new(card.clone()),
        })
        .is_empty()
    );
    assert_eq!(app.place, before);
    let mut foreign = card.clone();
    foreign.binding.root = crate::place::SharedContainerId([9; 32]);
    app.update(Action::ReconnectPlaceWithCard {
        session: app.session_id,
        generation: 4,
        card: Box::new(foreign),
    });
    assert_eq!(app.place, before);
    let effects = app.update(Action::ReconnectPlaceWithCard {
        session: app.session_id,
        generation: 4,
        card: Box::new(card.clone()),
    });
    assert!(matches!(&effects[..], [Effect::ReconnectPlace {
        session, generation: 5, binding: actual, contact: Some(actual_card),
    }] if *session == app.session_id && actual == &binding && **actual_card == card));
}

#[test]
fn toolbar_address_navigation_reads_without_changing_graph_command_semantics() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/first".into()));
    let first = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::SetWorkspaceMap(false));
    app.update(Action::OpenAddress(
        "https://example.test/independent".into(),
    ));
    assert_eq!(app.browser_command_target().unwrap().0, first);
    app.update(Action::OmnibarOpen { command: false });
    app.omnibar.text = "https://example.test/typed".into();
    app.omnibar.suggestions.clear();
    app.update(Action::OmnibarCommit);
    let target = app.browser_command_target().unwrap();
    assert_ne!(target.0, first);
    assert_eq!(target.1, "https://example.test/typed");
    assert!(!app.map_navigation_active());
}

#[test]
fn toolbar_repeated_map_returns_keep_a_real_graph_scene() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/a".into()));
    let first = app.graph_runtimes.focused_member().unwrap();
    let pane = app.default_graph_pane();
    app.with_graph_pane(pane, |canvas| canvas.set_graph_grammar(true));
    app.graph_pane_frame(pane, 1024, 521).unwrap();
    app.update(Action::SetWorkspaceMap(false));
    app.content.note_live(first, None);
    // Changing the graph selection retains the previously read member. That
    // decoration used to make the native translator refuse the whole scene.
    app.update(Action::OpenAddress("https://example.test/b".into()));
    for _ in 0..3 {
        app.update(Action::SetWorkspaceMap(true));
        assert!(
            !crate::observe::snapshot(&app)
                .surfaces
                .iter()
                .any(|kind| kind == "content")
        );
        for _ in 0..5 {
            let (scene, _) = app.graph_pane_frame(pane, 1024, 521).unwrap();
            let canvas = app
                .graph_runtimes
                .canvas(app.graph_for_pane(pane).unwrap())
                .unwrap();
            assert_eq!(canvas.last_translation_error(), None);
            assert!(!scene.ops.is_empty());
        }
        app.update(Action::SetWorkspaceMap(false));
        app.update(Action::ToggleToolPane(crate::panes::PaneKindId::new(
            crate::panes::kind::INSPECTOR,
        )));
        app.update(Action::ToggleToolPane(crate::panes::PaneKindId::new(
            crate::panes::kind::INSPECTOR,
        )));
    }
}

#[test]
fn graph_navigation_switch_keeps_map_camera_and_reading_selection() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/first".into()));
    let first = app.graph_runtimes.focused_member().unwrap();
    let map = app.default_graph_pane();
    app.with_graph_pane(map, |canvas| {
        canvas.frame(900, 600);
        canvas.center_on_selected();
        let mut view = canvas.viewport();
        view.offset.0 += 100.0;
        view.offset.1 += 80.0;
        canvas.set_viewport(view);
    });
    let camera = app
        .with_graph_pane(map, |canvas| canvas.viewport())
        .unwrap();
    app.update(Action::ReadGraphMember(first));
    app.update(Action::OpenAddress("https://example.test/second".into()));
    let second = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::WorkbenchActivate(first));
    app.update(Action::SetWorkspaceMap(true));
    assert_eq!(app.graph_pane_focused_member(map), Some(first));
    assert_eq!(app.browser_command_target().unwrap().0, first);
    assert_eq!(
        app.with_graph_pane(map, |canvas| canvas.viewport()),
        Some(camera)
    );
    app.graph_pane_select_member(map, second);
    app.update(Action::SetWorkspaceMap(false));
    assert_eq!(app.browser_command_target().unwrap().0, second);
    assert_eq!(app.graph_runtimes.graph().nodes().count(), 2);
    app.with_graph_pane(map, |canvas| {
        let mut view = canvas.viewport();
        view.offset = (-9000.0, -9000.0);
        view.zoom = 0.7;
        canvas.set_viewport(view);
    });
    app.update(Action::SetWorkspaceMap(true));
    app.with_graph_pane(map, |canvas| {
        let view = canvas.viewport();
        assert_eq!(view.zoom, 0.7, "offscreen reveal keeps the chosen zoom");
        let (x, y) = canvas.focused_screen_position().unwrap();
        assert!(x >= 24.0 && x <= view.view.0 as f32 - 24.0);
        assert!(y >= 24.0 && y <= view.view.1 as f32 - 24.0);
    });
}
