// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The old page's remote link: where the remote projection lives, and how
//! the page shows it.
//!
//! Two realizations. The **fixture** is the H3 page's in-process
//! `canary::FixtureEndpoint`, driven synchronously and mounted into the
//! app's own `ClientState`. The **WebRTC** link is a real host reached
//! through the C4 door; its rules are `graphshell_client::RemoteSession`'s
//! and its channel is `web_rtc_link`'s, both shared with the tree page.
//! `BrowserHost::remote_client` answers which of the two holds the scene,
//! and everything that reads it goes through it.
//!
//! Given `?signal=<url>` (and optionally `?invite=<fragment>`, else fetched
//! from the signaling server's `/invite`), `loader.js` calls
//! [`connect_remote`] once the host is ready. Without it the fixture stays,
//! as ruled: the choice is exposed, not made.

use graphshell::canary::FixtureEndpoint;
use graphshell::client::remote::{ActionForm, advertised_actions, card_labels};
use graphshell::client::{ClientState, MountedScene};
use graphshell::protocol::{AdvertisedAction, ProjectionSession};
use graphshell::remote_board::board_scene;
use mere::canvas::BoardFit;
use netrender::Scene;
use sceno::InstanceId;
use wasm_bindgen::prelude::*;
use web_sys::Document;

use super::web_rtc_link::{self, LiveRemote, RemoteHost, remote_profile};
use super::{BrowserHost, element, root, update_semantics, web_scenario};

/// The old page's chrome over the board: 50 px each side, the toolbar and
/// session row above, the detail strip below. It frames the scene's bounds,
/// as it always has, so its board sits where its chrome expects it.
const BOARD_FIT: BoardFit = BoardFit {
    left: 50.0,
    right: 50.0,
    top: 116.0,
    bottom: 64.0,
    frame_edges: false,
};

/// Where the remote projection lives.
pub(crate) enum RemoteLink {
    /// In-process, synchronous, mounted into the app's own client.
    Fixture(FixtureEndpoint),
    /// A real host over WebRTC.
    WebRtc(Box<LiveRemote>),
}

impl RemoteHost for BrowserHost {
    fn live(&mut self) -> Option<&mut LiveRemote> {
        match &mut self.remote {
            RemoteLink::WebRtc(live) => Some(live),
            RemoteLink::Fixture(_) => None,
        }
    }

    fn adopt(&mut self, mut live: LiveRemote) {
        // The fixture mount gives way: one remote session at a time. The
        // action surface carries across, so its count keeps counting.
        if let RemoteLink::Fixture(_) = self.remote {
            if let Some(old) = self.remote_session.take() {
                self.app.client.forget_session(&old);
            }
            live.session.form = std::mem::take(&mut self.actions);
        }
        self.remote = RemoteLink::WebRtc(Box::new(live));
        self.remote_joining = false;
    }

    fn pre_join(&mut self, status: &str) {
        self.remote_joining = true;
        self.remote_status = status.to_string();
    }

    fn join_failed(&mut self, error: String) {
        self.remote_joining = false;
        self.remote_status = format!("error: {error}");
        self.actions.status = format!("Failed · remote: {error}");
        self.probe_events.push(format!("remote-error {error}"));
    }

    fn changed(&mut self) {
        self.chrome_dirty = true;
        let _ = update_semantics(self);
    }
}

impl BrowserHost {
    /// Send what the session queued and fold its events into the page's.
    pub(crate) fn pump_remote(&mut self) {
        if let RemoteLink::WebRtc(live) = &mut self.remote {
            live.flush();
            let events = live.session.take_events();
            if !events.is_empty() {
                self.chrome_dirty = true;
            }
            self.probe_events.extend(events);
        }
    }

    /// The action surface in use: the session's once a WebRTC link is live,
    /// the page's own before.
    pub(crate) fn form(&self) -> &ActionForm {
        match &self.remote {
            RemoteLink::WebRtc(live) => &live.session.form,
            RemoteLink::Fixture(_) => &self.actions,
        }
    }

    pub(crate) fn form_mut(&mut self) -> &mut ActionForm {
        match &mut self.remote {
            RemoteLink::WebRtc(live) => &mut live.session.form,
            RemoteLink::Fixture(_) => &mut self.actions,
        }
    }

    /// The client holding the remote scene, whichever link is live.
    pub(crate) fn remote_client(&self) -> Option<&ClientState> {
        match &self.remote {
            RemoteLink::Fixture(_) => Some(&self.app.client),
            RemoteLink::WebRtc(live) => live.session.client(),
        }
    }

    /// The mounted remote projection's session, whichever link is live.
    pub(crate) fn remote_session_id(&self) -> Option<&ProjectionSession> {
        match &self.remote {
            RemoteLink::Fixture(_) => self.remote_session.as_ref(),
            RemoteLink::WebRtc(live) => live.session.session(),
        }
    }

    /// The mounted remote scene, if any.
    pub(crate) fn remote_mounted(&self) -> Option<&MountedScene> {
        self.remote_client()?.mounted(self.remote_session_id()?)
    }

    pub(crate) fn remote_status(&self) -> &str {
        match &self.remote {
            RemoteLink::WebRtc(live) => live.session.status(),
            RemoteLink::Fixture(_) => &self.remote_status,
        }
    }

    pub(crate) fn remote_last_resume(&self) -> &str {
        match &self.remote {
            RemoteLink::WebRtc(live) => live.session.last_resume(),
            RemoteLink::Fixture(_) => "",
        }
    }

    /// Mirror the local canvas's physics choice and speed onto the remote
    /// board, and reconcile its bodies whenever the acknowledged revision moves.
    pub(crate) fn sync_remote_board(&mut self) {
        let choice = self.canvas.physics_choice();
        let speed = self.canvas.physics_speed();
        let revision = self.remote_revision();
        let mounted = match &self.remote {
            RemoteLink::Fixture(_) => self
                .remote_session
                .as_ref()
                .and_then(|session| self.app.client.mounted(session)),
            RemoteLink::WebRtc(live) => live.session.mounted(),
        };
        self.remote_board.sync(mounted, revision, choice, speed);
    }

    /// The board drawn from the bodies, fitted under the page's chrome.
    pub(crate) fn remote_scene(&self) -> Scene {
        let board = self.remote_board.board();
        if self.remote_mounted().is_some() {
            self.remote_board
                .scene()
                .paint(board, self.width, self.height, BOARD_FIT)
        } else {
            mere::canvas::BoardScene::default().paint(board, self.width, self.height, BOARD_FIT)
        }
    }

    pub(crate) fn remote_revision(&self) -> Option<u64> {
        self.remote_client()?
            .acknowledgement(self.remote_session_id()?)
            .map(|ack| ack.revision.0)
    }

    /// Authority-emitted generation carried by the mounted scene.
    pub(crate) fn remote_generation(&self) -> Option<u64> {
        self.remote_mounted()
            .map(|mounted| mounted.scene.tables.generation)
    }

    /// Whether a remote answer is still to come — what the scenario lane's
    /// `wait` holds on.
    pub(crate) fn remote_in_flight(&self) -> bool {
        self.remote_joining
            || match &self.remote {
                RemoteLink::Fixture(_) => false,
                RemoteLink::WebRtc(live) => live.session.in_flight(),
            }
    }

    pub(crate) fn remote_link_name(&self) -> &'static str {
        match &self.remote {
            RemoteLink::Fixture(_) => "fixture",
            RemoteLink::WebRtc(_) => "webrtc",
        }
    }

    /// The chrome's label for the remote session.
    pub(crate) fn remote_label(&self) -> String {
        match &self.remote {
            RemoteLink::Fixture(_) => super::REMOTE_LABEL.to_string(),
            RemoteLink::WebRtc(live) => live.session.label(),
        }
    }

    /// The selection line and address the chrome shows for the remote session.
    pub(crate) fn remote_selection(&self) -> (String, String) {
        match &self.remote {
            RemoteLink::Fixture(_) => (
                "Projection boundary card".to_string(),
                "fixture.graphshell/note:recent".to_string(),
            ),
            RemoteLink::WebRtc(live) => live.session.selection(),
        }
    }

    /// The actions the remote scene advertises, one per intent.
    pub(crate) fn remote_actions(&self) -> Vec<(InstanceId, AdvertisedAction)> {
        match (self.remote_client(), self.remote_session_id()) {
            (Some(client), Some(session)) => advertised_actions(client, session, &remote_profile()),
            _ => Vec::new(),
        }
    }

    /// Names advertised by the mounted endpoint's presentation semantics.
    pub(crate) fn remote_card_labels(&self) -> Vec<String> {
        match (self.remote_client(), self.remote_session_id()) {
            (Some(client), Some(session)) => card_labels(client, session, &remote_profile()),
            _ => Vec::new(),
        }
    }

    /// Overlapping mounted footprints at the positions drawn this frame.
    pub(crate) fn remote_card_overlaps(&self) -> usize {
        self.remote_mounted()
            .map(|mounted| board_scene(mounted).overlaps(self.remote_board.board()))
            .unwrap_or(0)
    }

    /// Invoke the `index`th advertised action. A bounded form opens as a
    /// draft for the person to fill; a plain action submits at once.
    pub(crate) fn invoke_remote_action(&mut self, index: usize) {
        self.detail_open = true;
        if let RemoteLink::WebRtc(live) = &mut self.remote {
            live.session.invoke_action(index);
            return;
        }
        let Some((target, action)) = self.remote_actions().into_iter().nth(index) else {
            self.actions.status = format!("Failed · no remote action #{index}");
            return;
        };
        let Some((session, ack)) = self.remote_session.clone().and_then(|session| {
            let ack = self.app.client.acknowledgement(&session)?;
            Some((session, ack))
        }) else {
            self.actions.status = "Failed · remote projection is not acknowledged".to_string();
            return;
        };
        let bounded = self
            .actions
            .open_action(session, target, action, (ack.epoch, ack.revision));
        if bounded {
            self.chrome_dirty = true;
        } else {
            self.submit_action_draft();
        }
    }

    /// Open the first bounded form the remote scene advertises.
    pub(crate) fn open_remote_action_draft(&mut self) {
        match &mut self.remote {
            RemoteLink::WebRtc(live) => live.session.open_first_bounded(),
            RemoteLink::Fixture(_) => match self.remote_session.clone() {
                Some(session) => {
                    self.actions
                        .open_first_bounded(&self.app.client, &session, &remote_profile())
                },
                None => {
                    self.actions.status = "Failed · remote projection is not mounted".to_string()
                },
            },
        }
    }

    pub(crate) fn remote_disconnect(&mut self) {
        if let Err(status) = web_rtc_link::disconnect(self) {
            self.form_mut().status = status;
        }
    }

    pub(crate) fn remote_reconnect(&mut self) {
        let Some(state) = web_scenario::host() else {
            return;
        };
        if let Err(status) = web_rtc_link::reconnect(state, self) {
            self.form_mut().status = status;
        }
    }

    pub(crate) fn remote_nudge(&mut self) {
        let Some(state) = web_scenario::host() else {
            return;
        };
        if let Err(status) = web_rtc_link::nudge(state, self) {
            self.form_mut().status = status;
        }
    }
}

/// Mirror the remote link into the DOM: tokens on `<body>` and the
/// advertised actions as buttons, so the accessibility tree carries what
/// the endpoint offers and a scenario can press it.
pub(super) fn update_remote_semantics(
    host: &BrowserHost,
    document: &Document,
) -> Result<(), String> {
    let body = root()?;
    let set = |name: &str, value: &str| {
        body.set_attribute(name, value)
            .map_err(|_| format!("could not expose {name}"))
    };
    set("data-remote-link", host.remote_link_name())?;
    set("data-remote-state", host.remote_status())?;
    set(
        "data-remote-revision",
        &host
            .remote_revision()
            .map(|revision| revision.to_string())
            .unwrap_or_default(),
    )?;
    set("data-remote-resume", host.remote_last_resume())?;
    // The board's physics, for the P3 receipt: the law it runs, its energy,
    // and the distance between the first two cards in the score's units.
    set("data-remote-physics-law", host.canvas.physics_law().id())?;
    set(
        "data-remote-physics-speed",
        &crate::web_speed::field(host.remote_board.speed()),
    )?;
    set(
        "data-remote-energy",
        &format!("{:.1}", host.remote_board.energy()),
    )?;
    set(
        "data-remote-gap",
        &host
            .remote_board
            .gap("0", "1")
            .map(|gap| format!("{gap:.0}"))
            .unwrap_or_default(),
    )?;
    set(
        "data-remote-cards",
        &host
            .remote_mounted()
            .map(|mounted| mounted.scene.tables.items.len().to_string())
            .unwrap_or_default(),
    )?;
    set(
        "data-remote-generation",
        &host
            .remote_generation()
            .map(|generation| generation.to_string())
            .unwrap_or_default(),
    )?;
    set(
        "data-remote-card-labels",
        &host.remote_card_labels().join("\n"),
    )?;
    set(
        "data-remote-overlaps",
        &host.remote_card_overlaps().to_string(),
    )?;
    if let RemoteLink::WebRtc(live) = &host.remote {
        set("data-remote-subject", &live.transport.subject)?;
        set("data-remote-session", &live.transport.session_id)?;
        set("data-remote-rejoins", &live.session.rejoins().to_string())?;
    }
    let group = element("remote-actions")?;
    let actions = host.remote_actions();
    let rendered = group.get_attribute("data-rendered").unwrap_or_default();
    let signature = actions
        .iter()
        .map(|(_, action)| action.intent.0.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if rendered == signature {
        return Ok(());
    }
    group.set_text_content(None);
    for (index, (_, action)) in actions.iter().enumerate() {
        let button = document
            .create_element("button")
            .map_err(|_| "could not create a remote action button")?;
        button
            .set_attribute("type", "button")
            .and_then(|_| button.set_attribute("data-command", &format!("remote-action-{index}")))
            .and_then(|_| button.set_attribute("data-intent", &action.intent.0))
            .and_then(|_| {
                button.set_attribute(
                    "aria-description",
                    if action.input_form.is_some() {
                        "opens a bounded form"
                    } else {
                        "invokes at once"
                    },
                )
            })
            .map_err(|_| "could not describe a remote action button")?;
        button.set_text_content(Some(&action.label));
        group
            .append_child(&button)
            .map_err(|_| "could not append a remote action button")?;
    }
    group
        .set_attribute("data-rendered", &signature)
        .map_err(|_| "could not mark the remote actions rendered")?;
    Ok(())
}

/// Join a host over WebRTC and make it the remote link. Called by
/// `loader.js` from `?signal=` (and `?invite=`) once the host is ready.
#[wasm_bindgen]
pub fn connect_remote(signal_url: String, invite: Option<String>) -> Result<(), JsValue> {
    if super::web_tree::mounted() {
        return super::web_tree::connect_remote(signal_url, invite)
            .map_err(|error| JsValue::from_str(&error));
    }
    let state = web_scenario::host().ok_or_else(|| JsValue::from_str("the host has not booted"))?;
    web_rtc_link::connect(state, signal_url, invite);
    Ok(())
}
