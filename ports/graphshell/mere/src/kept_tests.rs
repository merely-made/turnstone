// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The "Find in what I kept" surface over a door fixture: a transport that
//! answers the real wire types from the real goal-surface fixture, so the
//! panel, the link and the unchanged goal surface are exercised end to end
//! without the user's resident.

use super::*;
use chirograph::{
    CarrierNotice, EndpointDescriptor, ProjectionOffer, ProjectionRequest, ProtocolVersion,
    RetainedTruncation,
};
use graphshell::goal_surface::{GraphFacts, RetainedPages, fixture};
use graphshell::kept_text::KeptPage;
use sceno::{Arrangement, Score};
use std::io::{Read, Write};

/// A resident stand-in: it holds the fixture's page text and graph, answers
/// `RetainedPages` through the same bounded mapping the resident uses, and
/// records the intents it is sent.
struct Door {
    keeping: RetainedKeeping,
    limits_seen: Vec<RetainedPagesLimits>,
    intents: Vec<(String, Vec<u8>)>,
    answer: IntentResult,
    omit_pages: u64,
}

impl Door {
    fn new(keeping: RetainedKeeping) -> Self {
        Self {
            keeping,
            limits_seen: Vec::new(),
            intents: Vec::new(),
            answer: IntentResult::Accepted,
            omit_pages: 0,
        }
    }
}

impl RouteTransport for Door {
    async fn request(&mut self, body: CarrierRequestBody) -> Result<CarrierResponseBody, String> {
        match body {
            CarrierRequestBody::RetainedPages(request) => {
                assert_eq!(request.session.0, MERE_GRAPH, "asked for the attached graph");
                self.limits_seen.push(request.limits);
                let (backend, graph) = fixture::backend_and_graph().await;
                let limits = request.limits.effective();
                let pages = RetainedPages::load_bounded(
                    "resident",
                    &backend,
                    &GraphFacts::read(&graph),
                    door::bounds(limits),
                    false,
                )
                .await?;
                let mut reply = door::to_wire(&pages, request.session, limits, self.keeping.clone());
                reply.truncated = RetainedTruncation {
                    pages_omitted: reply.truncated.pages_omitted + self.omit_pages,
                    ..reply.truncated
                };
                Ok(CarrierResponseBody::RetainedPages(Box::new(reply)))
            },
            CarrierRequestBody::Intent(intent) => {
                assert_eq!(intent.session.0, MERE_GRAPH);
                self.intents.push((intent.intent, intent.payload));
                Ok(CarrierResponseBody::Intent(self.answer.clone()))
            },
            other => Err(format!("unexpected {other:?}")),
        }
    }
    async fn take_notice(&mut self) -> Result<Option<CarrierNotice>, String> {
        Ok(None)
    }
}

fn descriptor() -> EndpointDescriptor {
    EndpointDescriptor {
        label: "Mere: browsing".into(),
        projections: vec![ProjectionOffer {
            label: "Attached session".into(),
            request: ProjectionRequest {
                version: ProtocolVersion::V1,
                session: ProjectionSession(MERE_GRAPH.into()),
                score: Score::new(Arrangement::Spiral(Default::default())),
            },
        }],
    }
}

fn link(door: Door) -> Link<Door> {
    Link::attach(door, &descriptor()).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn the_surface_finds_a_body_only_goal_in_pages_read_over_the_door() {
    let mut link = link(Door::new(RetainedKeeping::On));
    let mut panel = Panel::detached();
    assert_eq!(panel.state_text(), "Keep page text: not read yet.");
    panel.apply(link.run(Command::Refresh, None).await);
    assert_eq!(panel.surface.pages().sources().len(), 5);
    assert_eq!(panel.state_text(), "Keep page text: on. 5 pages kept.");
    assert!(panel.omitted.is_empty());
    panel.surface.goal.insert_str("keep a starter alive while travelling");
    panel.surface.run();
    let run = panel.surface.run.as_ref().expect("a run over what was kept");
    assert!(run.matched >= 1, "{:?}", panel.surface.notice);
    let cited: Vec<_> = run
        .steps
        .iter()
        .filter(|step| matches!(step.outcome, graphshell::goal_surface::Outcome::Visited(_)))
        .map(|step| step.address.as_str())
        .collect();
    assert!(cited.contains(&fixture::STARTER), "{cited:?}");
    // The viewer asked with defaults; the endpoint, not the viewer, bounds.
    assert_eq!(link.transport.limits_seen, vec![ASK]);
}

#[tokio::test(flavor = "current_thread")]
async fn the_panel_says_when_keeping_is_off_paused_or_the_read_was_partial() {
    let mut panel = Panel::detached();
    let mut off = link(Door::new(RetainedKeeping::Off));
    panel.apply(off.run(Command::Refresh, None).await);
    assert!(panel.state_text().contains("off. Nothing is being kept."));
    let mut paused = link(Door::new(RetainedKeeping::Paused {
        reason: "The vault is locked.".into(),
    }));
    panel.apply(paused.run(Command::Refresh, None).await);
    let text = panel.state_text();
    assert!(text.contains("paused") && text.contains("The vault is locked."), "{text}");
    assert!(text.contains("Nothing is written until this clears."));
    let mut partial = link(Door::new(RetainedKeeping::On));
    partial.transport.omit_pages = 4;
    panel.apply(partial.run(Command::Refresh, None).await);
    assert!(panel.omitted.contains("4 page(s)"), "{}", panel.omitted);
}

#[tokio::test(flavor = "current_thread")]
async fn turning_keeping_on_and_keeping_a_page_send_the_owner_checked_intents() {
    let mut link = link(Door::new(RetainedKeeping::On));
    let mut panel = Panel::detached();
    panel.apply(link.run(Command::SetKeep(true), None).await);
    assert_eq!(panel.notice, "Keeping page text is on. Pages you ask to keep are stored.");
    let page = KeptPage {
        url: "http://127.0.0.1:1/a".into(),
        text: "body text".into(),
        at_ms: 0,
        private: false,
    };
    let record = graphshell::kept_text::FetchRecordV1 {
        requested: page.url.clone(),
        final_url: page.url.clone(),
        fetched_at_ms: 7,
        status: 200,
        content_type: Some("text/html".into()),
        html_bytes: 99,
        text_bytes: 9,
        text_version: "v".into(),
    };
    panel.apply(
        link.run(Command::Keep(page.url.clone()), Some((page, record)))
            .await,
    );
    assert_eq!(panel.notice, "Kept the text of http://127.0.0.1:1/a.");
    let sent: Vec<_> = link.transport.intents.iter().map(|(i, _)| i.as_str()).collect();
    assert_eq!(sent, [KEEP_SETTING_INTENT, KEEP_PAGES_INTENT]);
    let keep: KeepPagesV1 = serde_json::from_slice(&link.transport.intents[1].1).unwrap();
    assert_eq!(keep.pages.len(), 1);
    assert_eq!(keep.pages[0].text, "body text");
    assert_eq!(keep.fetches.len(), 1, "the fetch travels with the page");
    assert_eq!(keep.fetches[0].html_bytes, 99);
    // A refusal is the person's to read, verbatim, and does not hide the state.
    link.transport.answer = IntentResult::Rejected {
        reason: "Keeping page text is paused. The vault is locked.".into(),
    };
    panel.apply(link.run(Command::SetKeep(true), None).await);
    assert!(panel.notice.contains("paused"), "{}", panel.notice);
}

#[tokio::test(flavor = "current_thread")]
async fn a_mere_without_an_attached_graph_session_is_refused_at_attach() {
    let none = EndpointDescriptor {
        label: "x".into(),
        projections: Vec::new(),
    };
    let refusal = Link::attach(Door::new(RetainedKeeping::On), &none).err().unwrap();
    assert!(refusal.contains("no attached graph session"));
}

/// A local fixture server: the first response is the page, every other path 404s.
fn serve(pages: Vec<(&'static str, &'static str, String)>) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        for _ in 0..pages.len() + 1 {
            let Ok((mut stream, _)) = listener.accept() else { return };
            let mut buffer = [0_u8; 2048];
            let read = stream.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
            let reply = match pages.iter().find(|(p, _, _)| *p == path) {
                Some((_, kind, body)) => format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                ),
                None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            };
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    (base, handle)
}

#[test]
fn a_capture_is_one_requested_address_with_visible_text_only() {
    let (base, server) = serve(vec![
        (
            "/page",
            "text/html; charset=utf-8",
            "<!doctype html><title>Tide tables</title><style>.x{color:red}</style><body><p>Spring tides follow the new moon.</p><script>var hidden=1</script></body>".into(),
        ),
        ("/data", "application/json", "{\"a\":1}".into()),
    ]);
    let (page, record) = capture(&format!("{base}/page")).expect("an HTML page is captured");
    assert_eq!(record.requested, format!("{base}/page"));
    assert_eq!(record.final_url, page.url);
    assert_eq!(record.status, 200);
    assert_eq!(record.text_bytes, page.text.len() as u64);
    assert!(record.html_bytes > record.text_bytes && record.fetched_at_ms > 0);
    assert_eq!(record.text_version, graphshell::kept_text::text_version(&page.text));
    assert!(page.text.contains("Spring tides follow the new moon."), "{}", page.text);
    assert!(!page.text.contains("hidden") && !page.text.contains("color:red"));
    assert!(page.url.ends_with("/page"));
    let refusal = capture(&format!("{base}/data")).unwrap_err();
    assert!(refusal.contains("HTML"), "{refusal}");
    assert!(capture("file:///etc/passwd").unwrap_err().contains("http"));
    assert!(capture("not an address").is_err());
    drop(server);
}
