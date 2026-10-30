// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The same vocabulary from inside a place: the card carries public ids and
/// this bind's own rendezvous, an invitation answer is written where the
/// person asked, a message goes to the binding's default channel, and a
/// stale answer writes nothing.
#[test]
fn place_artifacts_are_written_where_the_person_asked() {
    use crate::ui::{OmnibarMode, PlacePrompt};

    let mut app = App::test_stub();
    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0x91; 32]),
        crate::place::SharedContainerId([0x92; 32]),
        crate::place::ChatSpaceId([0x93; 32]),
        "general",
    )
    .unwrap();
    app.place = crate::place::PlaceState::Offline {
        binding: binding.clone(),
        generation: 4,
        snapshot: crate::place::OfflinePlaceSnapshot {
            sync: Some(crate::place::PlaceSyncSnapshot {
                lanes: Vec::new(),
                projection: None,
                local_rendezvous: vec!["ticket-one".into()],
                dialed_rendezvous: 0,
            }),
            ..Default::default()
        },
    };

    // The card: public ids, the founder's root, the current rendezvous.
    app.update(Action::BeginExportPlaceCard);
    assert_eq!(
        app.omnibar.mode,
        OmnibarMode::Place(PlacePrompt::ExportCard)
    );
    app.omnibar.text = "place-card.json".to_string();
    let effects = app.update(Action::OmnibarCommit);
    let bytes = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::WritePlaceArtifact { path, bytes } if path == "place-card.json" => Some(bytes),
            _ => None,
        })
        .expect("the card is written where it was asked for");
    let card: crate::place::PlaceCardV1 = serde_json::from_slice(bytes).unwrap();
    card.validate().unwrap();
    assert_eq!(card.binding, binding);
    assert_eq!(card.rendezvous.len(), 1);
    assert_eq!(card.rendezvous[0].hint, "ticket-one");
    assert_eq!(
        card.founder_root,
        crate::place::hex32(&app.personae_root().unwrap()),
        "the card names this profile's own root"
    );

    // A message commits on the binding's default channel.
    app.update(Action::BeginSendPlaceMessage);
    app.omnibar.text = "hello".to_string();
    let effects = app.update(Action::OmnibarCommit);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::RunPlaceCommand {
            command: crate::place::worker::PlaceCommand::SendMessage { channel, body },
            ..
        } if channel == "general" && body == "hello"
    )));

    // An invitation answer is written to the path the offer named.
    let effects = app.update(Action::InviteToPlaceWithPrekey {
        prekey: vec![1, 2, 3],
        out: "offer.json.invite.json".into(),
        access: crate::place::PlaceInviteAccess::Writer,
        lifetime_ms: None,
    });
    assert_eq!(
        effects,
        vec![Effect::InvitePlace {
            session: app.session_id,
            generation: 4,
            prekey: vec![1, 2, 3],
            access: crate::place::PlaceInviteAccess::Writer,
            lifetime_ms: None,
        }]
    );
    // A stale answer writes nothing and does not consume the pending path.
    assert!(
        app.apply_update(Update::PlaceInvited {
            session: app.session_id,
            generation: 3,
            result: Err("stale".into()),
        })
        .is_empty()
    );
    assert!(
        app.apply_update(Update::PlaceInvited {
            session: app.session_id,
            generation: 4,
            result: Err("no such member".into()),
        })
        .iter()
        .all(|effect| matches!(effect, Effect::Redraw))
    );
    assert!(app.take_events().iter().any(|event| matches!(
        event,
        crate::observe::AppEvent::PlaceRefused(reason) if reason.contains("no such member")
    )));
}

/// The reader invitation and the share row: both are place-shaped, so both
/// are offered only inside a place, the reader prompt commits the access its
/// label promises, and Escape leaves the session exactly as it was.
#[test]
fn place_reader_invitation_and_share_rows_act_only_inside_a_place() {
    use crate::ui::{OmnibarMode, PlacePrompt};

    let mut app = App::test_stub();
    let labels = |app: &App| -> Vec<String> {
        app.available_actions()
            .into_iter()
            .map(|(label, _)| label)
            .collect()
    };

    // Personal: neither row is offered, and both actions refuse out loud
    // rather than opening a prompt that could only fail.
    for label in ["Invite to place as reader", "Share focused node"] {
        assert!(
            !labels(&app).iter().any(|row| row == label),
            "{label} cannot act on a personal session"
        );
    }
    for action in [Action::BeginInviteToPlaceAsReader, Action::ShareFocusedNode] {
        assert_eq!(app.update(action), vec![Effect::Redraw]);
        assert!(!app.omnibar.open);
        assert!(matches!(
            app.take_events().last(),
            Some(crate::observe::AppEvent::PlaceRefused(_))
        ));
    }

    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0xa1; 32]),
        crate::place::SharedContainerId([0xa2; 32]),
        crate::place::ChatSpaceId([0xa3; 32]),
        "general",
    )
    .unwrap();
    app.place = crate::place::PlaceState::Offline {
        binding,
        generation: 7,
        snapshot: crate::place::OfflinePlaceSnapshot::default(),
    };
    for label in ["Invite to place as reader", "Share focused node"] {
        assert!(
            labels(&app).iter().any(|row| row == label),
            "missing {label} inside a place"
        );
    }

    // Each invite prompt carries its own access into the read.
    for (action, prompt, access) in [
        (
            Action::BeginInviteToPlaceAsReader,
            PlacePrompt::InviteReader,
            crate::place::PlaceInviteAccess::Reader,
        ),
        (
            Action::BeginInviteToPlace,
            PlacePrompt::Invite,
            crate::place::PlaceInviteAccess::Writer,
        ),
    ] {
        app.update(action);
        assert!(app.omnibar.open);
        assert_eq!(app.omnibar.mode, OmnibarMode::Place(prompt));
        app.omnibar.text = "offer.json".to_string();
        let effects = app.update(Action::OmnibarCommit);
        assert!(
            effects.contains(&Effect::ReadPlaceArtifact {
                path: "offer.json".into(),
                kind: crate::action::PlaceArtifactKind::Prekey {
                    access,
                    lifetime_ms: None,
                },
            }),
            "{prompt:?} lowered {effects:?}"
        );
        assert!(!app.omnibar.open, "commit closes the bar");
    }

    // Escape cancels without lowering anything.
    app.update(Action::BeginInviteToPlaceAsReader);
    assert_eq!(
        app.omnibar.mode,
        OmnibarMode::Place(PlacePrompt::InviteReader)
    );
    app.update(Action::OmnibarClose);
    assert!(!app.omnibar.open);
    assert_eq!(app.omnibar.mode, OmnibarMode::Address);

    // Sharing: nothing focused is a refusal, not an empty address.
    assert_eq!(app.update(Action::ShareFocusedNode), vec![Effect::Redraw]);
    assert!(app.take_events().iter().any(|event| matches!(
        event,
        crate::observe::AppEvent::PlaceRefused(reason) if reason.contains("no focused node")
    )));

    // A focused node with an address lowers the place command unchanged.
    app.update(Action::OpenAddress("https://example.com/page".to_string()));
    let effects = app.update(Action::ShareFocusedNode);
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::RunPlaceCommand {
                generation: 7,
                command: crate::place::worker::PlaceCommand::ShareNode { address },
                ..
            } if address == "https://example.com/page"
        )),
        "{effects:?}"
    );
}

/// The status row elides a ~200-character ticket; the copy action is the one
/// route to the whole of it. Offered only where there IS a ticket, and
/// refusing out loud where there is not.
#[test]
fn copying_the_local_rendezvous_carries_the_whole_ticket() {
    let ticket: String = "abcdefghijklmnopqrstuvwxyz234567"
        .chars()
        .cycle()
        .take(200)
        .collect();

    // Personal: no row, and the action refuses rather than copying nothing.
    let mut app = App::test_stub();
    assert!(
        !app.available_actions()
            .iter()
            .any(|(label, _)| label == "Copy local rendezvous")
    );
    assert_eq!(
        app.update(Action::CopyLocalRendezvous),
        vec![Effect::Redraw]
    );
    assert!(app.take_events().iter().any(|event| matches!(
        event,
        crate::observe::AppEvent::PlaceRefused(reason) if reason.contains("no local rendezvous")
    )));

    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0xc1; 32]),
        crate::place::SharedContainerId([0xc2; 32]),
        crate::place::ChatSpaceId([0xc3; 32]),
        "hall",
    )
    .unwrap();

    // Open but ticketless: still no row, still a refusal.
    app.place = crate::place::PlaceState::Offline {
        binding: binding.clone(),
        generation: 2,
        snapshot: crate::place::OfflinePlaceSnapshot::default(),
    };
    assert!(
        !app.available_actions()
            .iter()
            .any(|(label, _)| label == "Copy local rendezvous")
    );
    assert_eq!(
        app.update(Action::CopyLocalRendezvous),
        vec![Effect::Redraw]
    );
    assert!(app.take_events().iter().any(|event| matches!(
        event,
        crate::observe::AppEvent::PlaceRefused(reason) if reason.contains("no local rendezvous")
    )));

    // Live: the row is offered and the effect carries the FULL ticket, while
    // the status row and the palette label stay one line.
    app.place = crate::place::PlaceState::Offline {
        binding,
        generation: 2,
        snapshot: crate::place::OfflinePlaceSnapshot {
            sync: Some(crate::place::PlaceSyncSnapshot {
                lanes: Vec::new(),
                projection: None,
                local_rendezvous: vec![ticket.clone(), format!("{ticket}-second")],
                dialed_rendezvous: 0,
            }),
            ..Default::default()
        },
    };
    assert!(app.available_actions().contains(&(
        "Copy local rendezvous".to_string(),
        Action::CopyLocalRendezvous,
    )));

    let effects = app.update(Action::CopyLocalRendezvous);
    let copied = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::CopyText(text) => Some(text),
            _ => None,
        })
        .expect("the copy lowers to a clipboard effect");
    assert_eq!(*copied, format!("{ticket}\n{ticket}-second"));
    assert!(
        app.take_events()
            .iter()
            .any(|event| matches!(event, crate::observe::AppEvent::PlaceRendezvousCopied(2)))
    );

    // The observation keeps the whole ticket; the row a person reads does not.
    let facts = crate::observe::place_facts(&app).expect("a live place reports facts");
    assert_eq!(facts.local_rendezvous[0], ticket);
    let row = app
        .place
        .status_lines()
        .into_iter()
        .find(|line| line.starts_with("Local rendezvous:"))
        .expect("the literal prefix scenarios wait on survives");
    assert!(
        !row.contains(&ticket),
        "the row does not carry the whole ticket"
    );
    assert!(row.contains('…'));
}

/// A pre-key offer is composed from the worker's answer and this profile's
/// own root, and lands beside the card it answers.
#[test]
fn place_prekey_offer_is_written_beside_its_card() {
    let mut app = App::test_stub();
    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0xa1; 32]),
        crate::place::SharedContainerId([0xa2; 32]),
        crate::place::ChatSpaceId([0xa3; 32]),
        "general",
    )
    .unwrap();
    let card = crate::place::PlaceCardV1 {
        version: crate::place::PLACE_CARD_VERSION,
        binding: binding.clone(),
        founder_root: crate::place::hex32(&[0xb1; 32]),
        rendezvous: Vec::new(),
    };
    let effects = app.update(Action::OfferPlacePrekeyForCard {
        card: Box::new(card),
        out: "card.json.prekey.json".into(),
    });
    let generation = match effects.as_slice() {
        [
            Effect::OfferPlacePrekey {
                generation, moot, ..
            },
        ] => {
            assert_eq!(*moot, binding.moot.0);
            *generation
        },
        other => panic!("offering a pre-key lowers one worker command: {other:?}"),
    };

    let effects = app.apply_update(Update::PlacePrekeyOffered {
        session: app.session_id,
        generation,
        result: Ok(vec![7, 8, 9]),
    });
    let bytes = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::WritePlaceArtifact { path, bytes } if path == "card.json.prekey.json" => {
                Some(bytes)
            },
            _ => None,
        })
        .expect("the offer lands beside the card");
    let offer: crate::place::PlacePrekeyOfferV1 = serde_json::from_slice(bytes).unwrap();
    assert_eq!(offer.moot, binding.moot);
    assert_eq!(
        offer.root,
        crate::place::hex32(&app.personae_root().unwrap())
    );
    assert_eq!(offer.prekey_bytes().unwrap(), vec![7, 8, 9]);

    // The answer was consumed; a replay writes nothing.
    assert!(app.pending_place_artifact.is_none());
    assert!(
        app.apply_update(Update::PlacePrekeyOffered {
            session: app.session_id,
            generation,
            result: Ok(vec![7, 8, 9]),
        })
        .is_empty()
    );
}

/// T5c's rootless half, at the route the shell keys on: with no vault
/// configured, `from_env` must still hand the shell an engine to register.
/// `Ok(None)` is what left a returning joiner's `knot://<holder>/...` address
/// falling through to `host.external-protocol` in run 23.
#[cfg(feature = "authoring")]
#[test]
fn a_profile_with_no_vault_still_registers_a_knot_engine() {
    if std::env::var_os("TURNSTONE_KNOT_ROOT").is_some()
        || std::env::var_os("TURNSTONE_KNOT_MODE").is_some()
    {
        return;
    }
    let engine = crate::knot_authoring::KnotAuthoringEngine::from_env(
        std::sync::Arc::new(|| {}),
        Default::default(),
    )
    .expect("no root is not an error");
    assert!(
        engine.is_some(),
        "a rootless profile registers a visit-only engine, never nothing"
    );
}

/// Why run 23's reader could not click a Roster row (`click-row field.knot`
/// and a coordinate click on the same pixels both left focus on reader.html).
/// `Place status` opens the omnibar and leaves it open; `Shell::deliver_press`
/// (src/shell/input.rs) consumes the next press to dismiss it, so the row
/// never sees a click. The row path itself is sound — see
/// `cambium_pane::tests::a_row_in_a_long_narrow_roster_navigates_to_its_own_url`.
/// A scenario that has run `act Place status` must `key escape` before any
/// click verb.
#[test]
fn place_status_leaves_the_omnibar_open_over_a_pane_open() {
    let mut app = App::test_stub();
    app.update(Action::ShowPlaceStatus);
    assert!(app.omnibar.open, "Place status shows in the omnibar");

    // Opening a pane afterwards does not put it away, so the omnibar is still
    // standing when the next pointer press arrives.
    let open_roster = app
        .available_actions()
        .into_iter()
        .find(|(label, _)| label == "Open Roster pane")
        .map(|(_, action)| action)
        .expect("the Roster pane is in the palette");
    app.update(open_roster);
    assert!(
        app.omnibar.open,
        "opening a pane does not dismiss the omnibar; the next press does"
    );
}
