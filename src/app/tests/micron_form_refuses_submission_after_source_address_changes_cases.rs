// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn micron_form_refuses_submission_after_source_address_changes() {
    let mut app = App::test_stub();
    let address = "7fc8950ec4e50695be76eddc8e539ee8:/page/index.mu";
    let source = "`[Submit`:/page/capture.mu`x]\n`<x`seed>\n";
    app.update(Action::OpenAddress(address.into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_fetched(
        node,
        address.into(),
        crate::content::FetchedDocument {
            bytes: source.as_bytes().to_vec(),
            content_type: None,
            body: source.into(),
            effective_url: Some(address.into()),
            acquired_at_ms: 1,
        },
        source.len(),
    );
    app.update(Action::ComposeFocusedMicronForm);
    app.update(Action::OmnibarCommit); // field -> confirm
    app.update(Action::OmnibarInsert("send".into()));
    let graph = app.graph_runtimes.graph_containing_member(node).unwrap();
    assert!(
        app.graph_runtimes
            .canvas_mut(graph)
            .unwrap()
            .navigate_member(node, "7fc8950ec4e50695be76eddc8e539ee8:/page/other.mu")
    );
    let effects = app.update(Action::OmnibarCommit);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::SubmitMicron { .. }))
    );
}

#[test]
fn place_status_refresh_is_local_filtered_and_generation_scoped() {
    use crate::place::{
        OfflinePlaceSnapshot, PlaceLaneSnapshot, PlacePermissionSnapshot, PlaceSyncSnapshot,
    };
    let mut app = App::test_stub();
    let effects = app.update(Action::ShowPlaceStatus);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::ResyncPlace { .. }))
    );
    assert_eq!(
        crate::observe::snapshot(&app).place_status,
        vec!["Personal session: no shared place joined"]
    );

    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0xc1; 32]),
        crate::place::SharedContainerId([0xc2; 32]),
        crate::place::ChatSpaceId([0xc3; 32]),
        "hall",
    )
    .unwrap();
    app.place = crate::place::PlaceState::Offline {
        binding,
        generation: 8,
        snapshot: OfflinePlaceSnapshot::default(),
    };
    let effects = app.update(Action::ShowPlaceStatus);
    assert!(effects.contains(&Effect::ResyncPlace {
        session: app.session_id,
        generation: 8
    }));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::ReconnectPlace { .. }))
    );
    assert!(crate::observe::snapshot(&app).place_status[0].contains("retained data only"));
    app.update(Action::OmnibarInsert("flora".into()));
    let current = OfflinePlaceSnapshot {
        sync: Some(PlaceSyncSnapshot {
            lanes: vec![PlaceLaneSnapshot {
                name: "gemot/flora/v1",
                syncing: false,
                sync_rounds: 2,
                ops_received: 3,
                last_activity_ms: Some(42),
            }],
            projection: None,
            local_rendezvous: vec!["ticket".into()],
            dialed_rendezvous: 1,
        }),
        permissions: Some(PlacePermissionSnapshot {
            message_write: true,
            graph_write: false,
        }),
        ..Default::default()
    };
    app.apply_update(Update::PlaceOpened {
        session: app.session_id,
        generation: 8,
        result: Ok(current),
    });
    assert!(app.omnibar.suggestions.iter().any(|row| matches!(row,
        crate::ui::Suggestion::Prompt(text) if text == "Flora: idle; 2 rounds; 3 accepted ops")));
    let observed = crate::observe::snapshot(&app).place_status;
    assert!(observed[0].contains("peer reachability unknown"));
    assert!(
        observed
            .iter()
            .any(|line| line == "Message permission: effective locally at last refresh")
    );
    assert!(
        observed
            .iter()
            .any(|line| line == "Shared graph permission: not effective locally at last refresh")
    );
    assert!(
        app.apply_update(Update::PlaceOpened {
            session: app.session_id,
            generation: 7,
            result: Ok(OfflinePlaceSnapshot::default()),
        })
        .is_empty()
    );
    assert_eq!(crate::observe::snapshot(&app).place_status, observed);
    app.apply_update(Update::PlaceOpened {
        session: app.session_id,
        generation: 8,
        result: Err("refresh failed".into()),
    });
    assert!(crate::observe::snapshot(&app).place_status[0].contains("refresh failed"));
    app.update(Action::ShowPlaceStatus);
    assert!(app.omnibar.suggestions.iter().any(|row| matches!(row,
        crate::ui::Suggestion::Prompt(text) if text.contains("refresh failed"))));
}

#[test]
fn reconnect_metadata_failure_does_not_remove_the_admitted_binding() {
    let directory = std::env::temp_dir().join(format!(
        "turnstone-reconnect-leave-{}",
        uuid::Uuid::new_v4()
    ));
    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0xb1; 32]),
        crate::place::SharedContainerId([0xb2; 32]),
        crate::place::ChatSpaceId([0xb3; 32]),
        "hall",
    )
    .unwrap();
    crate::session::save_place_binding(&directory, &binding).unwrap();
    std::fs::create_dir(directory.join(crate::place::rendezvous::RENDEZVOUS_FILE)).unwrap();
    assert!(crate::session::remove_place_binding(&directory).is_err());
    assert_eq!(
        crate::session::load_place_binding(&directory).unwrap(),
        Some(binding)
    );
}

#[cfg(windows)]
#[test]
fn reconnect_hint_cleanup_can_fail_at_binding_removal_without_losing_history() {
    use std::os::windows::fs::OpenOptionsExt;
    let directory = std::env::temp_dir().join(format!(
        "turnstone-reconnect-leave-locked-{}",
        uuid::Uuid::new_v4()
    ));
    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0xc1; 32]),
        crate::place::SharedContainerId([0xc2; 32]),
        crate::place::ChatSpaceId([0xc3; 32]),
        "hall",
    )
    .unwrap();
    crate::session::save_place_binding(&directory, &binding).unwrap();
    let hints = directory.join(crate::place::rendezvous::RENDEZVOUS_FILE);
    std::fs::write(&hints, b"contact metadata").unwrap();
    std::fs::write(directory.join("retained-history"), b"history").unwrap();
    let locked = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(crate::session::place_binding_path(&directory))
        .unwrap();
    assert!(crate::session::remove_place_binding(&directory).is_err());
    drop(locked);
    assert_eq!(
        crate::session::load_place_binding(&directory).unwrap(),
        Some(binding)
    );
    assert!(!hints.exists());
    assert_eq!(
        std::fs::read(directory.join("retained-history")).unwrap(),
        b"history"
    );
    assert!(crate::session::remove_place_binding(&directory).unwrap());
    assert_eq!(
        crate::session::load_place_binding(&directory).unwrap(),
        None
    );
}

#[test]
fn micron_form_navigation_reload_and_close_cancel_and_ignore_late_reply() {
    for action in [
        Action::OpenAddress("7fc8950ec4e50695be76eddc8e539ee8:/page/other.mu".into()),
        Action::Reload,
        Action::OmnibarClose,
    ] {
        let mut app = micron_ready_to_send("`[Submit`:/page/capture.mu`fixed=ready]\n");
        app.update(Action::OmnibarInsert("send".into()));
        let effects = app.update(Action::OmnibarCommit);
        let (request, source, target) = effects
            .into_iter()
            .find_map(|effect| match effect {
                Effect::SubmitMicron {
                    request,
                    source,
                    submission,
                } => Some((request, source, submission.target)),
                _ => None,
            })
            .expect("explicit send");
        let effects = app.update(action);
        assert!(effects.contains(&Effect::CancelMicronSubmission { request }));
        if let crate::ui::OmnibarMode::SmolwebSubmissionResult(result) = &app.omnibar.mode {
            assert!(result.message.contains("Cancelled"));
        }
        let before = app.omnibar.clone();
        assert!(
            app.apply_update(Update::SmolwebSubmitted {
                request,
                source,
                target,
                result: Ok(crate::action::SmolwebSubmissionReceipt::Success(
                    crate::action::FetchedPage {
                        bytes: b"late reply".to_vec(),
                        body: "late reply".into(),
                        content_type: None,
                        content_disposition: None,
                    }
                )),
            })
            .is_empty()
        );
        assert_eq!(app.omnibar, before);
    }
}

#[test]
fn micron_form_fixed_variable_action_selection_and_visible_edit_errors() {
    let mut app = micron_ready_to_send(
        "`[First`:/page/capture.mu`fixed=a]\n`[Second`:/page/capture.mu`fixed=b]\n",
    );
    app.update(Action::OmnibarInsert("2".into()));
    app.update(Action::OmnibarCommit);
    assert!(
        matches!(&app.omnibar.mode, crate::ui::OmnibarMode::MicronForm(prompt) if prompt.stage == crate::ui::MicronFormStage::Confirm)
    );
    app.update(Action::OmnibarInsert("send".into()));
    let effects = app.update(Action::OmnibarCommit);
    assert!(effects.iter().any(|effect| matches!(effect, Effect::SubmitMicron { submission, .. } if submission.values.get("var_fixed").is_some_and(|value| value == "b"))));

    let mut app = micron_ready_to_send("`[Submit`:/page/capture.mu`*]\n`<note`>\n");
    app.update(Action::OmnibarInsert("x".repeat(4097)));
    let effects = app.update(Action::OmnibarCommit);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::SubmitMicron { .. }))
    );
    assert!(app.omnibar.suggestions.iter().any(
        |row| matches!(row, crate::ui::Suggestion::Prompt(text) if text.contains("byte limit"))
    ));
}

#[test]
fn micron_form_rejects_refreshed_source_before_send_and_on_reply() {
    let original = "`[Submit`:/page/capture.mu`fixed=ready]\n";
    for after_send in [false, true] {
        let mut app = micron_ready_to_send(original);
        let source = app.graph_runtimes.focused_member().unwrap();
        let address = "7fc8950ec4e50695be76eddc8e539ee8:/page/index.mu";
        app.update(Action::OmnibarInsert("send".into()));
        let effects = if after_send {
            app.update(Action::OmnibarCommit)
        } else {
            Vec::new()
        };
        app.content.note_fetched(
            source,
            address.into(),
            crate::content::FetchedDocument {
                bytes: b"replacement".to_vec(),
                content_type: None,
                body: "replacement".into(),
                effective_url: Some(address.into()),
                acquired_at_ms: 2,
            },
            11,
        );
        if after_send {
            let request = effects
                .into_iter()
                .find_map(|effect| match effect {
                    Effect::SubmitMicron { request, .. } => Some(request),
                    _ => None,
                })
                .unwrap();
            let before = app.omnibar.clone();
            assert!(
                app.apply_update(Update::SmolwebSubmitted {
                    request,
                    source: Some(source),
                    target: address.into(),
                    result: Err("old reply".into()),
                })
                .is_empty()
            );
            assert_eq!(app.omnibar, before);
        } else {
            let effects = app.update(Action::OmnibarCommit);
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::SubmitMicron { .. }))
            );
            assert!(app.omnibar.suggestions.iter().any(|row| matches!(row, crate::ui::Suggestion::Prompt(text) if text.contains("page changed"))));
        }
    }
}

/// The founding vocabulary through the omnibar: every `Begin*` opens the
/// prompt its label promises, Enter commits the typed line into the paired
/// action, Escape leaves the session exactly as it was, and a row offered in
/// the wrong situation refuses out loud instead of lowering an effect.
#[test]
fn place_founding_prompts_commit_escape_and_refuse() {
    use crate::ui::{OmnibarMode, PlacePrompt};

    let mut app = App::test_stub();

    // A personal session is offered the three rows it can act on.
    let labels: Vec<String> = app
        .available_actions()
        .into_iter()
        .map(|(label, _)| label)
        .collect();
    for label in ["Found place", "Join place", "Offer place pre-key"] {
        assert!(labels.iter().any(|row| row == label), "missing {label}");
    }
    for label in ["Export place card", "Invite to place", "Send place message"] {
        assert!(
            !labels.iter().any(|row| row == label),
            "{label} cannot act on a personal session"
        );
    }

    // Found: prompt, type, commit.
    app.update(Action::BeginFoundPlace);
    assert!(app.omnibar.open);
    assert_eq!(
        app.omnibar.mode,
        OmnibarMode::Place(PlacePrompt::FoundPlace)
    );
    app.omnibar.text = "Hearth".to_string();
    let session = app.session_id;
    let effects = app.update(Action::OmnibarCommit);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::FoundPlace { name, session: s, .. } if name == "Hearth" && *s == session
    )));
    assert!(matches!(
        app.place,
        crate::place::PlaceState::Joining { .. }
    ));
    assert!(!app.omnibar.open, "commit closes the bar");

    // A refused founding leaves no place at all, and says so.
    let generation = app.place.generation().unwrap();
    app.apply_update(Update::PlaceFounded {
        session,
        generation,
        result: Err("disk full".into()),
    });
    assert!(matches!(app.place, crate::place::PlaceState::Failed { .. }));
    assert!(app.take_events().iter().any(|event| matches!(
        event,
        crate::observe::AppEvent::PlaceRefused(reason) if reason.contains("disk full")
    )));

    // Escape cancels without lowering anything.
    app.place = crate::place::PlaceState::Personal;
    app.update(Action::BeginJoinPlaceFile);
    assert_eq!(app.omnibar.mode, OmnibarMode::Place(PlacePrompt::Join));
    app.update(Action::OmnibarClose);
    assert!(!app.omnibar.open);
    assert_eq!(app.omnibar.mode, OmnibarMode::Address);

    // Reading is a shell effect, and the kind is named by the prompt.
    assert_eq!(
        app.update(Action::JoinPlaceFile {
            path: "invite.json".into()
        }),
        vec![Effect::ReadPlaceArtifact {
            path: "invite.json".into(),
            kind: crate::action::PlaceArtifactKind::Invite,
        }]
    );
    assert_eq!(
        app.update(Action::OfferPlacePrekey {
            path: "card.json".into()
        }),
        vec![Effect::ReadPlaceArtifact {
            path: "card.json".into(),
            kind: crate::action::PlaceArtifactKind::Card,
        }]
    );

    // The place-side rows refuse a personal session rather than pretending.
    for action in [
        Action::BeginExportPlaceCard,
        Action::BeginInviteToPlace,
        Action::BeginSendPlaceMessage,
    ] {
        assert_eq!(app.update(action), vec![Effect::Redraw]);
        assert!(!app.omnibar.open);
        assert!(matches!(
            app.take_events().last(),
            Some(crate::observe::AppEvent::PlaceRefused(_))
        ));
    }
    assert_eq!(
        app.update(Action::InviteToPlace {
            path: "offer.json".into(),
            access: crate::place::PlaceInviteAccess::Writer,
            lifetime_ms: None,
        }),
        vec![Effect::Redraw]
    );
}
