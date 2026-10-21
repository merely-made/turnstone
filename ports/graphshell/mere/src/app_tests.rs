// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use mere_view::ArchiveRequest;

fn app() -> (
    App,
    mpsc::Sender<WorkerReply>,
    mpsc::Receiver<WorkerCommand>,
) {
    let (sender, requests) = mpsc::channel();
    let (answers, receiver) = mpsc::channel();
    (
        App {
            coop: None,
            model: MereViewModel::default(),
            view: MereViewState::default(),
            size: (1100, 800),
            selected: None,
            cards: HashMap::new(),
            sender,
            receiver,
            pending: false,
            pending_generation: None,
            next_request_generation: 1,
            queued_activation: None,
            events: Vec::new(),
            graph_epoch: 0,
            saved_focus: None,
            view_saved: false,
            read_only: false,
            persistent_refusal: None,
            owner_offers: Default::default(),
            owner_form: Default::default(),
            owner_record: None,
            owner_waiting: false,
        },
        answers,
        requests,
    )
}

fn set_pending(app: &mut App, generation: u64) {
    app.pending = true;
    app.pending_generation = Some(generation);
    app.next_request_generation = generation + 1;
}

fn reading(selected: Option<&str>) -> Reading {
    Reading {
        model: MereViewModel {
            sessions: vec![mere_view::SessionEntry {
                id: pandect::SessionId(uuid::Uuid::from_u128(7)),
                name: "Attached session".into(),
                detail: None,
                attached: true,
                trashed: false,
                steps: Vec::new(),
            }],
            ..Default::default()
        },
        cards: HashMap::from([(
            "same-record".into(),
            PortableCardV1 {
                title: "Disclosed in both sessions".into(),
                values: Vec::new(),
                badges: Vec::new(),
                media: Vec::new(),
            },
        )]),
        graph_epoch: 7,
        saved_focus: selected.map(str::to_owned),
        read_only: true,
        selected: selected.map(str::to_owned),
        view_saved: selected.is_some(),
        owner_offers: Default::default(),
    }
}

#[test]
fn shared_selection_reset_wins_even_when_the_record_is_still_disclosed() {
    let (mut app, answers, _) = app();
    assert!(
        answers
            .send(reply(None, Ok(reading(Some("same-record")))))
            .is_ok()
    );
    app.drain();
    assert_eq!(app.selected.as_deref(), Some("same-record"));
    assert!(app.view_saved);

    assert!(answers.send(reply(None, Ok(reading(None)))).is_ok());
    app.drain();
    assert!(app.cards.contains_key("same-record"));
    assert!(app.selected.is_none());
    assert!(app.saved_focus.is_none());
    assert!(!app.view_saved);
}

#[test]
fn successful_settled_read_does_not_claim_the_remote_command_is_done() {
    let reply = reply(None, Ok(reading(None)));
    assert!(reply.outcome.is_none());
    assert!(reply.reading.is_some());
}

#[test]
fn a_failed_read_does_not_claim_the_remote_command_was_refused() {
    let answer = reply(Some(3), Err("The reply was lost after dispatch.".into()));
    assert!(answer.outcome.is_none());
    assert!(matches!(answer.reading, Some(Err(_))));
}

#[test]
fn remote_refusal_notice_in_a_settled_read_is_shown_to_the_user() {
    let (mut app, answers, _) = app();
    let mut reading = reading(None);
    reading.model.notice = Some("That session is no longer available.".into());
    assert!(answers.send(reply(None, Ok(reading))).is_ok());
    app.drain();
    assert_eq!(
        app.model.notice.as_deref(),
        Some("That session is no longer available.")
    );
    assert!(app.persistent_refusal.is_none());
}

#[test]
fn failed_reading_removes_all_previously_published_record_state() {
    let (mut app, answers, _) = app();
    assert!(
        answers
            .send(reply(None, Ok(reading(Some("same-record")))))
            .is_ok()
    );
    app.drain();
    assert!(!app.cards.is_empty());
    set_pending(&mut app, 20);

    assert!(
        answers
            .send(reply(Some(20), Err("The admitted route closed.".into())))
            .is_ok()
    );
    app.drain();
    assert!(!app.pending);
    assert!(app.cards.is_empty());
    assert!(app.selected.is_none());
    assert!(app.saved_focus.is_none());
    assert!(!app.view_saved);
    assert_eq!(app.graph_epoch, 0);
    assert!(app.model.graph.nodes.is_empty());
    assert!(app.model.sessions.is_empty());
    assert!(app.model.archive.is_none());
    assert!(matches!(app.model.status, ViewStatus::Unavailable(_)));
}

#[test]
fn read_only_view_requests_still_reach_the_shared_controller() {
    let (mut app, _, requests) = app();
    app.read_only = true;
    app.request(MereViewRequest::Activate("same-record".into()));
    assert!(app.pending);
    assert!(matches!(
        requests.try_recv(),
        Ok(invocation) if matches!(decode_invocation(&invocation.invocation), Ok(MereViewRequest::Activate(key)) if key == "same-record")
    ));
}

#[test]
fn a_click_during_an_operation_is_forwarded_once_that_reading_arrives() {
    let (mut app, answers, requests) = app();
    assert!(
        answers
            .send(reply(None, Ok(reading(Some("same-record")))))
            .is_ok()
    );
    app.drain();
    set_pending(&mut app, 20);
    app.request(MereViewRequest::Activate("same-record".into()));
    assert_eq!(app.selected.as_deref(), Some("same-record"));
    assert!(requests.try_recv().is_err());

    assert!(answers.send(reply(Some(20), Ok(reading(None)))).is_ok());
    app.drain();
    assert!(matches!(
        requests.try_recv(),
        Ok(invocation) if matches!(decode_invocation(&invocation.invocation), Ok(MereViewRequest::Activate(key)) if key == "same-record")
    ));
    assert!(app.pending);
    assert!(app.queued_activation.is_none());
}

#[test]
fn buffered_notice_readings_do_not_release_queued_activation_or_a_new_mutation() {
    let (mut app, answers, requests) = app();
    assert!(
        answers
            .send(reply(None, Ok(reading(Some("same-record")))))
            .is_ok()
    );
    app.drain();

    set_pending(&mut app, 20);
    app.request(MereViewRequest::Activate("same-record".into()));
    assert_eq!(
        app.queued_activation.as_ref().map(|a| a.key.as_str()),
        Some("same-record")
    );

    // These notices were already queued when the user requested activation.
    // Neither can settle generation 20 or release the latest click.
    assert!(answers.send(reply(None, Ok(reading(None)))).is_ok());
    assert!(answers.send(reply(None, Ok(reading(None)))).is_ok());
    app.drain();
    assert!(app.pending);
    assert_eq!(app.pending_generation, Some(20));
    assert!(app.queued_activation.is_some());
    assert!(requests.try_recv().is_err());

    assert!(answers.send(reply(Some(20), Ok(reading(None)))).is_ok());
    app.drain();
    let command = requests
        .try_recv()
        .expect("activation dispatched after matching reply");
    assert_eq!(command.generation, 21);
    assert!(matches!(
        decode_invocation(&command.invocation),
        Ok(MereViewRequest::Activate(key)) if key == "same-record"
    ));
    assert!(app.pending);
    assert_eq!(app.pending_generation, Some(21));

    app.request(MereViewRequest::Mint);
    assert!(requests.try_recv().is_err());
    assert!(app.pending);
    assert_eq!(app.pending_generation, Some(21));
    assert!(
        app.model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("busy"))
    );
}

#[test]
fn a_queued_click_does_not_cross_a_session_epoch_change() {
    let (mut app, answers, requests) = app();
    assert!(
        answers
            .send(reply(None, Ok(reading(Some("same-record")))))
            .is_ok()
    );
    app.drain();
    set_pending(&mut app, 20);
    app.request(MereViewRequest::Activate("same-record".into()));
    let mut next_session = reading(None);
    next_session.graph_epoch += 1;
    assert!(answers.send(reply(Some(20), Ok(next_session))).is_ok());
    app.drain();
    assert!(requests.try_recv().is_err());
    assert!(app.selected.is_none());
    assert!(app.queued_activation.is_none());
}

#[test]
fn a_queued_click_does_not_cross_sessions_with_the_same_epoch() {
    let (mut app, answers, requests) = app();
    assert!(
        answers
            .send(reply(None, Ok(reading(Some("same-record")))))
            .is_ok()
    );
    app.drain();
    set_pending(&mut app, 20);
    app.request(MereViewRequest::Activate("same-record".into()));
    let mut next_session = reading(None);
    next_session.model.sessions[0].id = pandect::SessionId(uuid::Uuid::from_u128(8));
    assert!(answers.send(reply(Some(20), Ok(next_session))).is_ok());
    app.drain();
    assert!(requests.try_recv().is_err());
    assert!(app.selected.is_none());
    assert!(app.queued_activation.is_none());
}

#[test]
fn pending_layout_and_refresh_are_refused_without_reaching_the_worker() {
    let (mut app, answers, requests) = app();
    assert!(answers.send(reply(None, Ok(reading(None)))).is_ok());
    app.drain();
    set_pending(&mut app, 20);

    app.request(MereViewRequest::Layout("phyllotaxis.default".into()));
    assert!(requests.try_recv().is_err());
    assert!(
        app.model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("busy"))
    );

    app.model.notice = None;
    app.request(MereViewRequest::Host("refresh".into()));
    assert!(requests.try_recv().is_err());
    assert!(
        app.model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("busy"))
    );
    assert!(answers.send(reply(Some(20), Ok(reading(None)))).is_ok());
    app.drain();
    assert!(
        app.model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("busy"))
    );
}

#[test]
fn pending_mutation_is_refused_and_does_not_overtake_in_flight_work() {
    let (mut app, answers, requests) = app();
    assert!(answers.send(reply(None, Ok(reading(None)))).is_ok());
    app.drain();
    set_pending(&mut app, 20);

    app.request(MereViewRequest::Mint);
    assert!(requests.try_recv().is_err());
    assert!(app.pending);
    assert!(
        app.model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("busy"))
    );
}

#[test]
fn queued_activation_does_not_clear_a_newer_busy_refusal() {
    let (mut app, answers, requests) = app();
    assert!(answers.send(reply(None, Ok(reading(None)))).is_ok());
    app.drain();
    set_pending(&mut app, 20);

    app.request(MereViewRequest::Activate("same-record".into()));
    app.request(MereViewRequest::Mint);
    assert!(
        app.model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("busy"))
    );
    assert!(requests.try_recv().is_err());

    assert!(answers.send(reply(Some(20), Ok(reading(None)))).is_ok());
    app.drain();
    let command = requests.try_recv().expect("queued activation dispatched");
    assert_eq!(command.generation, 21);
    assert!(matches!(
        decode_invocation(&command.invocation),
        Ok(MereViewRequest::Activate(key)) if key == "same-record"
    ));
    assert!(app.pending);
    assert_eq!(app.pending_generation, Some(21));
    assert!(
        app.model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("busy"))
    );

    assert!(
        answers
            .send(reply(Some(21), Ok(reading(Some("same-record")))))
            .is_ok()
    );
    app.drain();
    assert!(!app.pending);
    assert!(
        app.model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("busy"))
    );
}

#[test]
fn busy_refusal_preserves_the_last_reading_and_does_not_claim_done() {
    let (mut app, answers, requests) = app();
    assert!(
        answers
            .send(reply(None, Ok(reading(Some("same-record")))))
            .is_ok()
    );
    app.drain();
    let epoch = app.graph_epoch;
    set_pending(&mut app, 20);

    app.request(MereViewRequest::Archive(ArchiveRequest::Save(
        pandect::SessionId(uuid::Uuid::from_u128(7)),
    )));
    assert!(requests.try_recv().is_err());
    assert_eq!(app.graph_epoch, epoch);
    assert!(app.cards.contains_key("same-record"));
    assert!(app.pending);
}

fn owner_reading(bounded: bool) -> Reading {
    let mut reading = reading(Some("same-record"));
    reading
        .cards
        .get_mut("same-record")
        .unwrap()
        .values
        .push(chirograph::CardValueV1 {
            label: "Address".into(),
            value: "https://owner.test/record".into(),
        });
    let mut action = graphshell::handlers::HandlerOffer {
        id: "fixture".into(),
        label: "Open with fixture".into(),
        explanation: "Owner supplied".into(),
        address_kinds: vec![],
    }
    .advertised_action();
    if bounded {
        action.intent = chirograph::IntentReference("fixture:choose".into());
        action.payload_schema = "fixture.choose/v1".into();
        action.input_form = Some(
            chirograph::ActionFormV1::new("fixture.choose/v1").with_field(
                chirograph::ActionFormFieldV1::choice(
                    "choice",
                    "Choice",
                    [chirograph::ActionFormChoiceV1::new(
                        "owner-value",
                        "Owner choice",
                    )],
                ),
            ),
        );
    }
    reading.owner_offers.insert(
        "same-record".into(),
        (
            graphshell_client::ActionDraftTarget {
                session: chirograph::ProjectionSession("mere.graph".into()),
                target: sceno::InstanceId(42),
                observed_epoch: scenotime::SceneEpoch(7),
                observed_revision: scenotime::Revision(9),
            },
            vec![action],
        ),
    );
    reading
}

#[test]
fn selected_owner_action_uses_existing_address_contract_and_exact_target() {
    let (mut app, answers, requests) = app();
    answers.send(reply(None, Ok(owner_reading(false)))).unwrap();
    app.drain();
    app.open_owner_action("graphshell.address.open/fixture");
    let WorkerCommand {
        owner:
            Some(OwnerActionTarget {
                key,
                target,
                action,
            }),
        invocation: command,
        ..
    } = requests.try_recv().unwrap()
    else {
        panic!("owner request")
    };
    assert_eq!(key, "same-record");
    assert_eq!(target.target, sceno::InstanceId(42));
    assert_eq!(target.observed_revision, scenotime::Revision(9));
    assert_eq!(command.id, action.intent.0);
    assert_eq!(command.schema_id, graphshell::handlers::OPEN_ADDRESS_SCHEMA);
    let payload: graphshell::handlers::OpenAddressV1 =
        serde_json::from_slice(&command.arguments).unwrap();
    assert_eq!(payload.address, "https://owner.test/record");
    assert_eq!(payload.handler, "fixture");
    assert!(app.owner_waiting);
    let mut reply = owner_reading(false);
    reply.model.notice = Some("Request accepted by the owner.".into());
    answers
        .send(super::reply(app.pending_generation, Ok(reply)))
        .unwrap();
    app.drain();
    assert!(!app.owner_waiting);
    assert_eq!(app.owner_form.status, "Request accepted by the owner.");
}

#[test]
fn owner_form_requires_exact_choices_and_does_not_follow_a_changed_target() {
    let (mut app, answers, requests) = app();
    answers.send(reply(None, Ok(owner_reading(true)))).unwrap();
    app.drain();
    app.open_owner_action("fixture:choose");
    app.submit_owner_action();
    assert!(requests.try_recv().is_err());
    let draft = app.owner_form.draft.as_mut().unwrap();
    assert!(draft.choose("choice", "invented").is_err());
    draft.choose("choice", "owner-value").unwrap();
    let mut changed = owner_reading(true);
    changed
        .owner_offers
        .get_mut("same-record")
        .unwrap()
        .0
        .observed_epoch = scenotime::SceneEpoch(8);
    answers.send(reply(None, Ok(changed))).unwrap();
    app.drain();
    app.submit_owner_action();
    assert!(requests.try_recv().is_err());
    assert!(app.owner_form.status.contains("changed"));
    assert_eq!(
        app.owner_form.draft.as_ref().unwrap().values()["choice"],
        "owner-value"
    );
}

#[test]
fn native_owner_controls_route_real_genet_clicks_through_the_existing_form() {
    use genet_scripted_dom::{NodeId, ScriptedDom};
    use layout_dom_api::{LayoutDom, Namespace};

    fn text(dom: &ScriptedDom, node: NodeId) -> String {
        let mut content = dom.text(node).unwrap_or("").to_owned();
        for child in dom.dom_children(node) {
            content.push_str(&text(dom, child));
        }
        content
    }
    fn find(dom: &ScriptedDom, attribute: Option<(&str, &str)>, label: Option<&str>) -> NodeId {
        let mut nodes = vec![dom.document()];
        while let Some(node) = nodes.pop() {
            let attr_matches = attribute.is_none_or(|(name, value)| {
                dom.attribute(node, &Namespace::from(""), &name.into()) == Some(value)
            });
            if dom
                .element_name(node)
                .is_some_and(|name| name.local.to_string() == "button")
                && attr_matches
                && label.is_none_or(|label| text(dom, node) == label)
            {
                return node;
            }
            nodes.extend(dom.dom_children(node));
        }
        panic!("owner control not found: {attribute:?} {label:?}")
    }

    let (mut app, answers, requests) = app();
    answers.send(reply(None, Ok(owner_reading(true)))).unwrap();
    app.drain();
    let dom: cambium::DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
    let mut runner =
        cambium::GenetAppRunner::new(dom.clone(), owner_actions::view as fn(&App) -> Child, app);
    let action = find(
        &dom.borrow(),
        Some(("data-owner-action", "fixture:choose")),
        None,
    );
    let _ = runner.dispatch_click(action, cambium::PointerClick::at((1., 1.)));
    assert!(
        requests.try_recv().is_err(),
        "opening a form invokes nothing"
    );
    let choice = find(&dom.borrow(), None, Some("Owner choice"));
    let _ = runner.dispatch_click(choice, cambium::PointerClick::at((1., 1.)));
    let submit = find(&dom.borrow(), Some(("data-owner-submit", "true")), None);
    let _ = runner.dispatch_click(submit, cambium::PointerClick::at((1., 1.)));
    let WorkerCommand {
        owner: Some(OwnerActionTarget { target, .. }),
        invocation: command,
        ..
    } = requests.try_recv().unwrap()
    else {
        panic!("owner command")
    };
    assert_eq!(target.target, sceno::InstanceId(42));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&command.arguments).unwrap()["choice"],
        "owner-value"
    );
    assert!(runner.state().owner_waiting);
}

#[test]
fn owner_pending_action_waits_for_its_matching_worker_generation() {
    let (mut app, answers, requests) = app();
    answers.send(reply(None, Ok(owner_reading(false)))).unwrap();
    app.drain();
    app.open_owner_action("graphshell.address.open/fixture");
    let command = requests.try_recv().unwrap();
    assert!(command.owner.is_some());
    answers.send(reply(None, Ok(owner_reading(false)))).unwrap();
    app.drain();
    assert!(app.pending && app.owner_waiting);
    assert_eq!(app.pending_generation, Some(command.generation));
    assert_eq!(app.owner_form.status, "Requesting action from owner…");
    let mut reading = owner_reading(false);
    reading.model.notice = Some("Request accepted by the owner.".into());
    let answer = reply(Some(command.generation), Ok(reading));
    assert!(
        answer.outcome.is_none(),
        "a reading cannot claim terminal completion"
    );
    answers.send(answer).unwrap();
    app.drain();
    assert!(!app.pending && !app.owner_waiting);
    assert_eq!(app.owner_form.status, "Request accepted by the owner.");
}

#[test]
fn owner_worker_commands_keep_the_owner_declaration_and_refuse_mismatches() {
    let (mut app, answers, requests) = app();
    answers.send(reply(None, Ok(owner_reading(false)))).unwrap();
    app.drain();
    app.open_owner_action("graphshell.address.open/fixture");
    let packet = requests.try_recv().unwrap();
    assert!(matches!(
        decode_worker_command(&packet),
        Ok(WorkerRequest::Owner { .. })
    ));
    assert!(
        decode_invocation(&packet.invocation).is_err(),
        "owner data is not recoded as a local view command"
    );
    for field in 0..3 {
        let mut invocation = packet.invocation.clone();
        match field {
            0 => invocation.id.push_str("-wrong"),
            1 => invocation.schema_id.push_str("-wrong"),
            _ => invocation.effect = cambium::EffectClass::DomainTruth,
        }
        let invalid = WorkerCommand {
            generation: packet.generation,
            invocation,
            owner: packet.owner.clone(),
        };
        assert!(matches!(
            decode_worker_command(&invalid),
            Err(Outcome::Refused { .. })
        ));
    }
}
