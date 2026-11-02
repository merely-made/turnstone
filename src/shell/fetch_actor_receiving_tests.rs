// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Mere's bounded fetch actor at the product's receiving boundary: refusals,
//! queue expiry, Stop and exit each settle the requesting page, through the
//! same effect, command and update adapters the shell uses.

use super::*;
use crate::app::App;
use crate::browse::PendingFetches;
use fetch_port::FetchPort;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const ANSWER: Duration = Duration::from_secs(5);

fn port(max_active: usize, max_pending: usize, queue_ms: u64) -> (FetchPort, Receiver<FetchUpdate>) {
    let wake: armillary::Wake = Arc::new(|| {});
    let (actor, updates) = fetch::spawn_fetcher_with_admission(
        wake,
        fetch::Stores::in_memory(),
        fetch::ClientFetchPolicy::default(),
        fetch::FetchActorPolicy {
            max_active,
            max_pending,
            queue_timeout: Duration::from_millis(queue_ms),
        },
    )
    .unwrap();
    (FetchPort::Network(actor), updates)
}

/// Accepts and holds connections without answering, so an admitted read keeps
/// its running slot until it is cancelled.
fn silent_origin() -> (u16, mpsc::Receiver<()>) {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (contact, contacts) = mpsc::channel();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            held.push(stream);
            if contact.send(()).is_err() {
                break;
            }
        }
    });
    (port, contacts)
}

struct Browser {
    app: App,
    pending: PendingFetches,
    _directory: tempfile::TempDir,
}

impl Browser {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        Self {
            app: App::test_stub_at(directory.path().join("app")),
            pending: PendingFetches::default(),
            _directory: directory,
        }
    }

    /// Apply an action and send its fetch commands; false if any was refused.
    fn run(&mut self, action: Action, port: &FetchPort) -> bool {
        let effects = self.app.update(action);
        effects
            .iter()
            .flat_map(|effect| browse::fetch_commands_for(effect, &mut self.pending))
            .fold(true, |admitted, command| port.command(command) && admitted)
    }

    fn open(&mut self, address: &str, port: &FetchPort) -> (uuid::Uuid, fetch::FetchRequestId, bool) {
        let admitted = self.run(Action::OpenAddress(address.into()), port);
        let node = self.app.graph_runtimes.focused_member().unwrap();
        let request = self.app.content.active_fetch(node).expect("page fetch starts");
        (node, request, admitted)
    }

    fn receive(&mut self, updates: &Receiver<FetchUpdate>) -> Answer {
        let update = updates.recv_timeout(ANSWER).expect("the actor answers");
        let answer = match &update {
            FetchUpdate::Page(fetch::FetchOutcome {
                request,
                result: Err(fetch::FetchFailure::Failed(reason)),
                ..
            }) => Answer::Failed(*request, reason.clone()),
            FetchUpdate::Page(fetch::FetchOutcome {
                request,
                result: Err(fetch::FetchFailure::Cancelled),
                ..
            }) => Answer::Cancelled(*request),
            _ => Answer::Other,
        };
        if let Some(update) = browse::update_from_fetch(update, &mut self.pending) {
            self.app.apply_update(update);
        }
        answer
    }

    fn loading(&self, node: uuid::Uuid) -> bool {
        self.app.content.fetch_in_progress(node) || self.app.content.active_fetch(node).is_some()
    }
}

/// A page answer as it left the actor, read before the app adapter consumes it.
#[derive(Debug, PartialEq)]
enum Answer {
    Failed(fetch::FetchRequestId, String),
    Cancelled(fetch::FetchRequestId),
    Other,
}

impl Answer {
    fn failure(&self) -> Option<(fetch::FetchRequestId, &str)> {
        match self {
            Self::Failed(request, reason) => Some((*request, reason.as_str())),
            _ => None,
        }
    }
}

#[test]
fn refused_admission_settles_the_page_without_contacting_its_origin() {
    let (origin, contacts) = silent_origin();
    let (port, updates) = port(0, 0, 1000);
    let mut browser = Browser::new();
    let (node, request, admitted) =
        browser.open(&format!("gopher://127.0.0.1:{origin}/0/refused.txt"), &port);
    assert!(!admitted, "zero running capacity refuses the page");
    let answer = browser.receive(&updates);
    let (answered, reason) = answer.failure().expect("a correlated page failure");
    assert_eq!(answered, request);
    assert!(reason.contains("capacity exhausted"), "{reason}");
    assert!(!browser.loading(node), "the refused page is not left loading");
    assert!(contacts.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn queue_expiry_settles_the_waiting_page_and_exit_stops_the_running_one() {
    let (origin, contacts) = silent_origin();
    let (mut port, updates) = port(1, 1, 200);
    let mut browser = Browser::new();
    let (running, running_request, admitted) =
        browser.open(&format!("gopher://127.0.0.1:{origin}/0/running.txt"), &port);
    assert!(admitted);
    contacts.recv_timeout(ANSWER).expect("the running page reaches its origin");
    let (waiting, waiting_request, admitted) =
        browser.open(&format!("gopher://127.0.0.1:{origin}/0/waiting.txt"), &port);
    assert!(admitted, "the second page takes the pending slot");
    assert_ne!(running, waiting);
    assert_eq!(browser.app.content.active_fetch(running), Some(running_request));

    let answer = browser.receive(&updates);
    let (answered, reason) = answer.failure().expect("a correlated page failure");
    assert_eq!(answered, waiting_request);
    assert!(reason.contains("queue timed out"), "{reason}");
    assert!(!browser.loading(waiting), "the expired page is not left loading");
    assert!(browser.loading(running), "the running page keeps its slot");
    assert!(contacts.try_recv().is_err(), "the expired page never reached its origin");

    let started = Instant::now();
    port.close_and_join(Duration::from_millis(1500));
    assert!(started.elapsed() < Duration::from_millis(1500), "exit joins within its bound");
    assert_eq!(browser.receive(&updates), Answer::Cancelled(running_request));
    assert!(!browser.loading(running), "exit settles the running page as stopped");
    assert!(!port.command(FetchCommand::CancelPage { request: running_request }));
}

#[test]
fn stop_releases_the_running_slot_for_the_next_page() {
    let (origin, contacts) = silent_origin();
    let (port, updates) = port(1, 0, 1000);
    let mut browser = Browser::new();
    let (stopped, _, admitted) =
        browser.open(&format!("gopher://127.0.0.1:{origin}/0/stopped.txt"), &port);
    assert!(admitted);
    contacts.recv_timeout(ANSWER).expect("the first page reaches its origin");
    assert!(browser.run(Action::Stop, &port), "Stop is a control, not admitted work");
    assert!(!browser.loading(stopped));
    // The actor's cancellation answer arrives after the app already retired it.
    browser.receive(&updates);

    // A slot freed only by task destruction may lag the cancellation answer.
    let deadline = Instant::now() + ANSWER;
    let next = loop {
        let (node, _, admitted) =
            browser.open(&format!("gopher://127.0.0.1:{origin}/0/next.txt"), &port);
        if admitted {
            break node;
        }
        let refused = browser.receive(&updates);
        assert!(
            refused
                .failure()
                .is_some_and(|(_, reason)| reason.contains("capacity exhausted"))
        );
        assert!(Instant::now() < deadline, "the stopped page released its slot");
        std::thread::sleep(Duration::from_millis(20));
    };
    contacts.recv_timeout(ANSWER).expect("the next page reaches its origin");
    assert!(browser.loading(next));
}
