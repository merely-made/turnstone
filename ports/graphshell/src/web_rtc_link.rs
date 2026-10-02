// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The WebRTC link under a remote session, for whichever page holds it.
//!
//! The rules of the session — what each answer implies next, what the person
//! is told — are `graphshell_client::RemoteSession`'s. What is left here is
//! the browser's: joining a host through the C4 door (`BrowserJoin` →
//! `BrowserSession`), the two pumps that move lines between the channel and
//! the session, rejoining as the same subject, and the fixture's `/nudge`
//! hook. A page implements [`RemoteHost`] over wherever it keeps the session;
//! the old page keeps it in its `BrowserHost`, the tree page in its shared
//! state.

use std::cell::RefCell;
use std::rc::Rc;

use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures_util::StreamExt;
use graphshell::protocol::{CapabilityProfile, PresentationCapability};
use graphshell::webrtc_browser::{
    BrowserFrames, BrowserInitiatorConfig, BrowserJoin, BrowserSession, BrowserWriter,
    HandshakeLimits, InMemoryProvider, InviteV1, RetiredSession, SignedDelegationCertificate,
};
use graphshell_client::RemoteSession;
use js_sys::{Array, Function, Promise};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{Request, RequestInit, RequestMode, Response};

/// What the browser can present of a remote scene. `NativeGlyph` is what
/// the live fixture's cards require; the H3 canary offers portable cards.
pub(crate) fn remote_profile() -> CapabilityProfile {
    CapabilityProfile::new([
        PresentationCapability::PortableCard,
        PresentationCapability::Image,
        PresentationCapability::NativeGlyph,
    ])
}

/// How long to wait for ICE gathering before offering what has arrived.
const GATHER_TIMEOUT_MS: i32 = 3_000;

/// The channel a joined session speaks over.
pub(crate) struct Transport {
    outbox: UnboundedSender<String>,
    /// The channel's outbound half, kept so the page can close it.
    writer: BrowserWriter,
    pub(crate) subject: String,
    pub(crate) session_id: String,
    /// What a reconnect presents again: the same signaling server and the
    /// same invitation (the delegation and subject come back from the
    /// retired session).
    signal_url: String,
    /// The invitation as its fragment; `InviteV1` is deliberately not
    /// `Clone`, and a rejoin parses it again.
    invite_fragment: String,
}

/// A joined remote: the session's rules over the browser's channel.
pub(crate) struct LiveRemote {
    pub(crate) session: RemoteSession,
    pub(crate) transport: Transport,
}

impl LiveRemote {
    /// Put whatever the session queued onto the channel, in order.
    pub(crate) fn flush(&mut self) {
        for line in self.session.take_outgoing() {
            if self.transport.outbox.unbounded_send(line).is_err() {
                self.session.fail("the session writer is gone");
                break;
            }
        }
    }
}

/// Where a page keeps its remote session.
pub(crate) trait RemoteHost: 'static {
    /// The joined remote, if any.
    fn live(&mut self) -> Option<&mut LiveRemote>;
    /// A join completed: this is now the page's remote session.
    fn adopt(&mut self, live: LiveRemote);
    /// The link's status before a session exists; a join is in flight.
    fn pre_join(&mut self, status: &str);
    /// The first join failed before a session existed.
    fn join_failed(&mut self, error: String);
    /// Something changed: flush the session, mirror it, draw it.
    fn changed(&mut self);
}

/// Join a host over WebRTC and make it the page's remote session.
pub(crate) fn connect<H: RemoteHost>(
    state: Rc<RefCell<H>>,
    signal_url: String,
    invite: Option<String>,
) {
    {
        let mut host = state.borrow_mut();
        host.pre_join("joining");
        host.changed();
    }
    spawn_local(async move {
        if let Err(error) = join(state.clone(), signal_url, invite).await {
            let mut host = state.borrow_mut();
            host.join_failed(error);
            host.changed();
        }
    });
}

async fn join<H: RemoteHost>(
    state: Rc<RefCell<H>>,
    signal_url: String,
    invite: Option<String>,
) -> Result<(), String> {
    let status = |text: &str| {
        let mut host = state.borrow_mut();
        host.pre_join(text);
        host.changed();
    };
    let fragment = match invite {
        Some(fragment) if !fragment.trim().is_empty() => fragment,
        _ => {
            status("fetching the invite");
            http("GET", &format!("{signal_url}/invite"), None)
                .await
                .map_err(|error| format!("GET /invite: {error}"))?
        },
    };
    let invite_fragment = fragment.trim().to_string();
    let invite = InviteV1::parse_fragment(&invite_fragment)
        .map_err(|error| format!("invite fragment: {error}"))?;
    status("building the peer connection");
    let mut browser_join =
        BrowserJoin::new(BrowserInitiatorConfig::default()).map_err(|error| error.to_string())?;
    let answer = offer_and_signal(&mut browser_join, &signal_url, &status).await?;
    status("joining: challenge, redemption, admission");
    let session = browser_join
        .complete(&answer, &invite, &HandshakeLimits::default().clamped())
        .await
        .map_err(|error| format!("join refused: {error}"))?;
    let subject = session.subject_hex();
    let session_id = hex(&session.joined.session_id);
    let writer = session.writer();
    let (outbox, inbox) = unbounded::<String>();
    {
        let mut host = state.borrow_mut();
        host.adopt(LiveRemote {
            session: RemoteSession::new(remote_profile()),
            transport: Transport {
                outbox,
                writer: writer.clone(),
                subject,
                session_id,
                signal_url,
                invite_fragment,
            },
        });
        if let Some(live) = host.live() {
            live.session.joined();
        }
        host.changed();
    }
    spawn_pumps(state, session, writer, inbox);
    Ok(())
}

/// The two tasks a live session needs: outbound lines onto the channel, and
/// every inbound line into the session.
fn spawn_pumps<H: RemoteHost>(
    state: Rc<RefCell<H>>,
    mut session: BrowserSession,
    writer: BrowserWriter,
    mut inbox: UnboundedReceiver<String>,
) {
    spawn_local(async move {
        while let Some(line) = inbox.next().await {
            if let Err(error) = writer.send_line(&line).await {
                web_sys::console::error_1(&format!("remote send: {error}").into());
                break;
            }
        }
    });

    spawn_local(async move {
        loop {
            let read = session.next_line().await;
            let mut host = state.borrow_mut();
            let done = match (read, host.live()) {
                (Ok(Some(line)), Some(live)) => {
                    live.session.on_line(&line);
                    false
                },
                (Ok(None), Some(live)) => {
                    live.session.channel_closed();
                    true
                },
                (Err(error), Some(live)) => {
                    live.session.link_failed(format!("recv: {error}"));
                    true
                },
                (_, None) => true,
            };
            host.changed();
            if done {
                break;
            }
        }
        // Parked, never dropped: the channel's own close event still lands
        // in the initiator's closures (see `BrowserSession::retire`).
        let retired = session.retire();
        RETIRED.with(|parked| parked.borrow_mut().push(retired));
    });
}

/// Close the channel from this end. The session retires when the close
/// lands; the session and its mount are kept for a reconnect.
pub(crate) fn disconnect<H: RemoteHost>(host: &mut H) -> Result<(), String> {
    let live = host
        .live()
        .ok_or("Failed · no WebRTC link to disconnect")?;
    live.session.disconnect();
    live.transport.writer.close();
    Ok(())
}

/// A new link to the same host as the same subject: the retired session's
/// delegation is presented again, the invitation is not spent twice, and
/// whatever moved while the link was down comes back by diff.
pub(crate) fn reconnect<H: RemoteHost>(state: Rc<RefCell<H>>, host: &mut H) -> Result<(), String> {
    let live = host.live().ok_or("Failed · no WebRTC link to reconnect")?;
    let signal_url = live.transport.signal_url.clone();
    let invite = live.transport.invite_fragment.clone();
    let retired = RETIRED
        .with(|parked| parked.borrow_mut().pop())
        .ok_or("Failed · no retired session to rejoin as")?;
    // The retired channel stays parked; only the subject and the delegation
    // travel to the new link.
    PARKED.with(|parked| parked.borrow_mut().push(retired.frames));
    live.session.reconnecting();
    spawn_local(async move {
        let result = rejoin(
            state.clone(),
            signal_url,
            invite,
            retired.ephemeral,
            retired.joined.delegation,
        )
        .await;
        if let Err(error) = result {
            let mut host = state.borrow_mut();
            if let Some(live) = host.live() {
                live.session.link_failed(error);
            }
            host.changed();
        }
    });
    Ok(())
}

/// Ask the signaling server to move the board natively (`POST /nudge`), as
/// a host would while a peer is away. A receipt hook: the fixture is the
/// only server that answers it.
pub(crate) fn nudge<H: RemoteHost>(state: Rc<RefCell<H>>, host: &mut H) -> Result<(), String> {
    let live = host.live().ok_or("Failed · no WebRTC link to nudge")?;
    let signal_url = live.transport.signal_url.clone();
    live.session.nudging();
    spawn_local(async move {
        let result = http("POST", &format!("{signal_url}/nudge"), Some("")).await;
        let mut host = state.borrow_mut();
        if let Some(live) = host.live() {
            live.session.nudged(result);
        }
        host.changed();
    });
    Ok(())
}

async fn rejoin<H: RemoteHost>(
    state: Rc<RefCell<H>>,
    signal_url: String,
    invite_fragment: String,
    ephemeral: InMemoryProvider,
    delegation: SignedDelegationCertificate,
) -> Result<(), String> {
    let status = |text: &str| {
        let mut host = state.borrow_mut();
        if let Some(live) = host.live() {
            live.session.set_status(text);
        }
        host.changed();
    };
    let invite = InviteV1::parse_fragment(&invite_fragment)
        .map_err(|error| format!("invite fragment: {error}"))?;
    status("building the peer connection");
    let mut browser_join =
        BrowserJoin::new(BrowserInitiatorConfig::default()).map_err(|error| error.to_string())?;
    let answer = offer_and_signal(&mut browser_join, &signal_url, &status).await?;
    status("rejoining: challenge, admission");
    let session = browser_join
        .complete_rejoin(
            &answer,
            &invite,
            ephemeral,
            delegation,
            &HandshakeLimits::default().clamped(),
        )
        .await
        .map_err(|error| format!("rejoin refused: {error}"))?;
    let session_id = hex(&session.joined.session_id);
    let writer = session.writer();
    let (outbox, inbox) = unbounded::<String>();
    {
        let mut host = state.borrow_mut();
        let live = host
            .live()
            .ok_or("the link went away during the rejoin")?;
        live.transport.outbox = outbox;
        live.transport.writer = writer.clone();
        live.transport.session_id = session_id;
        live.session.rejoined();
        host.changed();
    }
    spawn_pumps(state, session, writer, inbox);
    Ok(())
}

thread_local! {
    static RETIRED: RefCell<Vec<RetiredSession>> = const { RefCell::new(Vec::new()) };
    /// Channels of sessions that were rejoined as: their frames, kept alive.
    static PARKED: RefCell<Vec<BrowserFrames>> = const { RefCell::new(Vec::new()) };
}

/// Offer, gather, splice, post, and hand back the answer — the C4a page's
/// signaling, unchanged: one `POST /offer` that C5 replaces with `mer3ly.net`.
async fn offer_and_signal(
    join: &mut BrowserJoin,
    signal_url: &str,
    status: &dyn Fn(&str),
) -> Result<String, String> {
    let candidates: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let (gathered, finish) = resolver();
    {
        let candidates = candidates.clone();
        join.initiator()
            .on_local_ice_candidate(move |candidate| match candidate {
                Some(candidate) => {
                    if candidate_is_usable(&candidate.candidate) {
                        candidates.borrow_mut().push(candidate.candidate);
                    }
                },
                None => {
                    let _ = finish.call0(&JsValue::NULL);
                },
            });
    }
    status("creating the offer");
    let offer = join
        .create_offer()
        .await
        .map_err(|error| format!("create_offer: {error}"))?;
    status("gathering ICE candidates");
    let race = Array::new();
    race.push(&gathered);
    race.push(&JsValue::from(timeout(GATHER_TIMEOUT_MS)?));
    JsFuture::from(Promise::race(&JsValue::from(race)))
        .await
        .map_err(|error| describe(&error))?;
    if candidates.borrow().is_empty() {
        return Err(
            "offer has no usable ICE candidates (mDNS .local names only) — \
                    disable chrome://flags/#enable-webrtc-hide-local-ips-with-mdns"
                .to_string(),
        );
    }
    let full_offer = offer_with_candidates(&offer, &candidates.borrow());
    status("posting the offer");
    http("POST", &format!("{signal_url}/offer"), Some(&full_offer))
        .await
        .map_err(|error| format!("POST /offer: {error}"))
}

/// Drop what the host's ICE agent cannot pair with: mDNS `.local` names
/// and non-UDP.
fn candidate_is_usable(candidate: &str) -> bool {
    let fields: Vec<&str> = candidate.split_whitespace().collect();
    if fields.len() < 6 || !fields[2].eq_ignore_ascii_case("udp") {
        return false;
    }
    let address = fields[4];
    !address.to_ascii_lowercase().ends_with(".local") && address.parse::<std::net::IpAddr>().is_ok()
}

fn offer_with_candidates(offer: &str, candidates: &[String]) -> String {
    let mut sdp = offer.to_owned();
    if !sdp.ends_with('\n') {
        sdp.push_str("\r\n");
    }
    for candidate in candidates {
        sdp.push_str("a=");
        sdp.push_str(candidate);
        sdp.push_str("\r\n");
    }
    sdp
}

fn resolver() -> (Promise, Function) {
    let mut slot: Option<Function> = None;
    let promise = Promise::new(&mut |resolve, _reject| slot = Some(resolve));
    (
        promise,
        slot.expect("a Promise executor runs synchronously"),
    )
}

fn timeout(ms: i32) -> Result<Promise, String> {
    let window = web_sys::window().ok_or("no window")?;
    Ok(Promise::new(&mut |resolve, _reject| {
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
    }))
}

async fn http(method: &str, url: &str, body: Option<&str>) -> Result<String, String> {
    let init = RequestInit::new();
    init.set_method(method);
    init.set_mode(RequestMode::Cors);
    if let Some(body) = body {
        init.set_body(&JsValue::from_str(body));
    }
    let request = Request::new_with_str_and_init(url, &init).map_err(|error| describe(&error))?;
    if body.is_some() {
        // A CORS *simple* request: no preflight for the fixture to answer.
        request
            .headers()
            .set("content-type", "text/plain")
            .map_err(|error| describe(&error))?;
    }
    let window = web_sys::window().ok_or("no window")?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|error| describe(&error))?
        .dyn_into()
        .map_err(|_| "fetch returned no Response".to_string())?;
    let text = JsFuture::from(response.text().map_err(|error| describe(&error))?)
        .await
        .map_err(|error| describe(&error))?
        .as_string()
        .unwrap_or_default();
    if !response.ok() {
        return Err(format!(
            "{} {}: {text}",
            response.status(),
            response.status_text()
        ));
    }
    Ok(text)
}

fn describe(value: &JsValue) -> String {
    value
        .as_string()
        .or_else(|| {
            value
                .dyn_ref::<js_sys::Error>()
                .map(|error| String::from(error.message()))
        })
        .unwrap_or_else(|| format!("{value:?}"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
