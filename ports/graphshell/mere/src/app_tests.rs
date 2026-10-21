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
