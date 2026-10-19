// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn app() -> (
    App,
    mpsc::Sender<Result<Reading, String>>,
    mpsc::Receiver<MereViewRequest>,
) {
    let (sender, requests) = mpsc::channel();
    let (answers, receiver) = mpsc::channel();
    (
        App {
            model: MereViewModel::default(),
            view: MereViewState::default(),
            size: (1100, 800),
            selected: None,
            cards: HashMap::new(),
            sender,
            receiver,
            pending: false,
            queued_activation: None,
            events: Vec::new(),
            graph_epoch: 0,
            saved_focus: None,
            view_saved: false,
            read_only: false,
        },
        answers,
        requests,
    )
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
    assert!(answers.send(Ok(reading(Some("same-record")))).is_ok());
    app.drain();
    assert_eq!(app.selected.as_deref(), Some("same-record"));
    assert!(app.view_saved);

    assert!(answers.send(Ok(reading(None))).is_ok());
    app.drain();
    assert!(app.cards.contains_key("same-record"));
    assert!(app.selected.is_none());
    assert!(app.saved_focus.is_none());
    assert!(!app.view_saved);
}

#[test]
fn failed_reading_removes_all_previously_published_record_state() {
    let (mut app, answers, _) = app();
    assert!(answers.send(Ok(reading(Some("same-record")))).is_ok());
    app.drain();
    assert!(!app.cards.is_empty());
    app.pending = true;

    assert!(
        answers
            .send(Err("The admitted route closed.".into()))
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
        Ok(MereViewRequest::Activate(key)) if key == "same-record"
    ));
}

#[test]
fn a_click_during_an_operation_is_forwarded_once_that_reading_arrives() {
    let (mut app, answers, requests) = app();
    assert!(answers.send(Ok(reading(Some("same-record")))).is_ok());
    app.drain();
    app.pending = true;
    app.request(MereViewRequest::Activate("same-record".into()));
    assert_eq!(app.selected.as_deref(), Some("same-record"));
    assert!(requests.try_recv().is_err());

    assert!(answers.send(Ok(reading(None))).is_ok());
    app.drain();
    assert!(matches!(
        requests.try_recv(),
        Ok(MereViewRequest::Activate(key)) if key == "same-record"
    ));
    assert!(app.pending);
    assert!(app.queued_activation.is_none());
}

#[test]
fn a_queued_click_does_not_cross_a_session_epoch_change() {
    let (mut app, answers, requests) = app();
    assert!(answers.send(Ok(reading(Some("same-record")))).is_ok());
    app.drain();
    app.pending = true;
    app.request(MereViewRequest::Activate("same-record".into()));
    let mut next_session = reading(None);
    next_session.graph_epoch += 1;
    assert!(answers.send(Ok(next_session)).is_ok());
    app.drain();
    assert!(requests.try_recv().is_err());
    assert!(app.selected.is_none());
    assert!(app.queued_activation.is_none());
}

#[test]
fn a_queued_click_does_not_cross_sessions_with_the_same_epoch() {
    let (mut app, answers, requests) = app();
    assert!(answers.send(Ok(reading(Some("same-record")))).is_ok());
    app.drain();
    app.pending = true;
    app.request(MereViewRequest::Activate("same-record".into()));
    let mut next_session = reading(None);
    next_session.model.sessions[0].id = pandect::SessionId(uuid::Uuid::from_u128(8));
    assert!(answers.send(Ok(next_session)).is_ok());
    app.drain();
    assert!(requests.try_recv().is_err());
    assert!(app.selected.is_none());
    assert!(app.queued_activation.is_none());
}
