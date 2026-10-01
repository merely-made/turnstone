// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The lifted remote session against the real live-board endpoint.
//!
//! `graphshell_client::RemoteSession` is what both browser pages delegate
//! to. Here it drives the same `LiveEndpoint` the C4 host serves, through
//! `dispatch_common` with notices written after each answer, as the serve
//! loop writes them before its next read. The path is the headed receipts'
//! (`c4b1_live_board`, `c4b3_reconnect`) with the WebRTC link taken out.

use chirograph::{CapabilityProfile, CarrierOutput, CarrierRequest, PresentationCapability};
use graphshell::live_endpoint::{ADMITTED_INTENT, LiveEndpoint, REFUSED_INTENT};
use graphshell_client::RemoteSession;
use graphshell_endpoint::{ProjectionNoticeSource, ResumableProjectionSource, dispatch_common};

fn profile() -> CapabilityProfile {
    CapabilityProfile::new([
        PresentationCapability::PortableCard,
        PresentationCapability::Image,
        PresentationCapability::NativeGlyph,
    ])
}

/// The host end: answers, then bells, and a log of the verbs asked.
struct Host {
    endpoint: LiveEndpoint,
    verbs: Vec<String>,
}

impl Host {
    fn answer(&mut self, line: &str) -> Vec<String> {
        let request: CarrierRequest = serde_json::from_str(line).expect("a request");
        let verb = serde_json::to_value(&request.body).unwrap();
        self.verbs.push(match verb {
            serde_json::Value::String(name) => name,
            serde_json::Value::Object(map) => map.keys().next().cloned().unwrap_or_default(),
            other => other.to_string(),
        });
        let mut resume = |endpoint: &mut LiveEndpoint, request| {
            ResumableProjectionSource::resume(endpoint, request).map_err(|error| error.to_string())
        };
        let response = dispatch_common(&mut self.endpoint, request, &mut resume)
            .expect("only common verbs are asked");
        let mut lines =
            vec![serde_json::to_string(&CarrierOutput::Response(response)).unwrap()];
        while let Some(notice) = self.endpoint.poll_notice().unwrap() {
            lines.push(serde_json::to_string(&CarrierOutput::Notice(notice)).unwrap());
        }
        lines
    }

    fn count(&self, verb: &str) -> usize {
        self.verbs.iter().filter(|asked| *asked == verb).count()
    }
}

fn pump(remote: &mut RemoteSession, host: &mut Host) {
    for _ in 0..64 {
        let outgoing = remote.take_outgoing();
        if outgoing.is_empty() {
            return;
        }
        for line in outgoing {
            for answer in host.answer(&line) {
                remote.on_line(&answer);
            }
        }
    }
    panic!("the session never went quiet");
}

fn intent(remote: &RemoteSession, name: &str) -> usize {
    remote
        .actions()
        .iter()
        .position(|(_, action)| action.intent.0 == name)
        .unwrap_or_else(|| panic!("{name} is advertised"))
}

fn cards(remote: &RemoteSession) -> usize {
    remote
        .mounted()
        .map_or(0, |mounted| mounted.scene.tables.items.len())
}

#[test]
fn the_live_board_receipts_hold_without_a_browser() {
    let mut host = Host {
        endpoint: LiveEndpoint::new(),
        verbs: Vec::new(),
    };
    let mut remote = RemoteSession::new(profile());
    remote.joined();
    pump(&mut remote, &mut host);
    assert_eq!(remote.status(), "open");
    assert_eq!(remote.revision(), Some(1));
    assert_eq!(cards(&remote), 1);
    let labels: Vec<String> = remote
        .actions()
        .into_iter()
        .map(|(_, action)| action.label)
        .collect();
    assert!(labels.iter().any(|label| label == "Append a card"), "{labels:?}");
    assert!(labels.iter().any(|label| label == "Forbidden action"), "{labels:?}");

    // c4b1: the forbidden intent is refused and the revision stands still,
    // measured by a resnapshot.
    remote.invoke_action(intent(&remote, REFUSED_INTENT));
    pump(&mut remote, &mut host);
    assert!(remote.form.status.starts_with("Rejected"), "{}", remote.form.status);
    assert_eq!(remote.revision(), Some(1));
    assert_eq!(host.count("Snapshot"), 2);

    // The admitted one is accepted; its bell resumes by diff.
    remote.invoke_action(intent(&remote, ADMITTED_INTENT));
    pump(&mut remote, &mut host);
    assert!(remote.form.status.starts_with("Accepted"), "{}", remote.form.status);
    assert_eq!(remote.revision(), Some(2));
    assert_eq!(cards(&remote), 2);
    assert_eq!(remote.last_resume(), "diff · 1 → 2");
    let events = remote.take_events();
    assert!(events.contains(&"remote-bell revision 2".to_string()), "{events:?}");
    assert!(!remote.in_flight());

    // c4b3: down; the host moves natively; back as the same subject.
    remote.disconnect();
    remote.channel_closed();
    assert_eq!(remote.status(), "disconnected");
    host.endpoint.append();
    remote.reconnecting();
    remote.rejoined();
    pump(&mut remote, &mut host);
    assert_eq!(remote.status(), "open");
    assert_eq!(remote.rejoins(), 1);
    assert_eq!(remote.revision(), Some(3));
    assert_eq!(cards(&remote), 3);
    assert_eq!(remote.last_resume(), "diff · 2 → 3");
    assert_eq!(host.count("Snapshot"), 2, "the mount was kept: no re-snapshot");
    let events = remote.take_events();
    assert!(events.contains(&"remote-rejoined".to_string()), "{events:?}");
    assert!(events.contains(&"remote-bell revision 3".to_string()), "{events:?}");
    assert!(!remote.in_flight());
}
