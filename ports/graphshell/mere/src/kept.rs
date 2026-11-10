// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! "Find in what I kept": native retained page text for the mere viewer
//! (Eidetic S6).
//!
//! Three small things sit here, none of them a store: a **link** to the
//! resident's mere route that reads the bounded `RetainedPages` reply and
//! sends two owner-checked intents (keep a page, change the mere's "Keep page
//! text" setting); a **capture** that fetches the one address a person chose,
//! extracts its visible text and bounds it; and the **panel** that shows the
//! mere's keep state and runs the unchanged goal surface over the reply. The
//! reservoir owns the store; the resident decides whether text is written.
//!
//! A capture happens only on the person's explicit request for one address.
//! Nothing here fetches in the background, follows links, or keeps a page the
//! setting does not allow.

use super::{App, Child, connection::RouteTransport};
use cambium::{TextInput, button, el, lens, text_field_typed};
use chirograph::{
    CapabilityProfile, CarrierRequestBody, CarrierResponseBody, IntentInvocation, IntentResult,
    PresentationCapability, ProjectionSession, RetainedKeeping, RetainedPagesLimits,
    RetainedPagesRequest, RetainedPagesResponse, SessionOpen,
};
use graphshell::goal_surface::{self, GoalSurface, door};
use eidetic::kept_text::{
    DEFAULT_KEPT_BYTES, FetchRecordV1, KEEP_PAGES_INTENT, KEEP_SETTING_INTENT, KeepPagesV1,
    KeepSettingV1, KeptPage,
};
use graphshell_door::projection::AppBrokerClient;
use mere_overmap::wire::MERE_GRAPH;
use ostiary::app_admission::{AppId, AppRouteId};
use sceno::InstanceId;
use scenotime::{Revision, SceneEpoch};
use std::{rc::Rc, sync::mpsc};

/// Owner label for snapshots read over the door; citations are minted under it.
const OWNER: &str = "graphshell-mere";
/// What the viewer asks for: the endpoint clamps to its own ceiling.
const ASK: RetainedPagesLimits = RetainedPagesLimits {
    max_pages: 0,
    max_bytes: 0,
    max_versions: 0,
    max_links: 0,
};
/// Most HTML one capture reads, before extraction.
const MAX_HTML_BYTES: u64 = 1 << 20;

pub(super) const CSS: &str = ".kept-panel{padding:8px;border-top:1px solid #6d7783;overflow:auto;height:560px}.kept-panel h2,.kept-panel h3,.kept-panel p{margin:3px;overflow-wrap:anywhere}.kept-panel label{display:flex;align-items:center;gap:8px;margin:4px}.kept-input{min-width:420px;min-height:28px;padding:4px;border:1px solid #6d7783;background:#22262c;white-space:pre;overflow:hidden}.kept-panel .goal-input{background:#22262c;border-color:#6d7783;color:#e8e6e1}.kept-panel .goal-cite{background:#1d2127;border-color:#4a525a}.kept-panel .goal-meta{color:#abb4bd}.kept-panel .goal-notice{color:#f0b35a}.kept-panel mark{background:#ffd54a;color:#16191d}.kept-state-paused{color:#f0b35a}";

/// What the panel asks the worker to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Refresh,
    SetKeep(bool),
    Keep(String),
}

/// What the worker found out.
#[derive(Debug)]
pub(super) struct Reply {
    pub pages: Option<Result<RetainedPagesResponse, String>>,
    pub notice: Option<String>,
}

/// The bounded text of one page at `address`, fetched once because the person
/// asked. Declared HTML only; script and style are not text; the text is cut
/// at the per-page bound on a character boundary.
pub(super) fn capture(address: &str) -> Result<(KeptPage, FetchRecordV1), String> {
    let parsed = url::Url::parse(address).map_err(|_| "That is not a web address.".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Only http and https pages can be kept.".into());
    }
    let fetched_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default();
    let net = fetch::NetFetch::new(&fetch::Stores::in_memory())
        .map_err(|error| format!("Could not start the page fetch: {error}"))?;
    let body = <fetch::NetFetch as fetch::Fetch>::read_all(
        &net,
        address,
        Some("text/html"),
        MAX_HTML_BYTES,
    )
    .map_err(|error| format!("Could not read that page: {error}"))?;
    let html = String::from_utf8_lossy(&body.bytes);
    let (_, text) = fleece::page_text_from_html(body.facts.content_type.as_deref(), &html)
        .ok_or("That address did not return an HTML page.")?;
    let mut text = text.trim().to_owned();
    if text.is_empty() {
        return Err("The page had no text to keep.".into());
    }
    if text.len() > DEFAULT_KEPT_BYTES {
        let mut end = DEFAULT_KEPT_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    let record = FetchRecordV1 {
        requested: address.to_owned(),
        final_url: body.facts.final_url.clone(),
        fetched_at_ms,
        // Only a successful answer yields a body; anything else was refused above.
        status: 200,
        content_type: body.facts.content_type.clone(),
        html_bytes: body.bytes.len() as u64,
        text_bytes: text.len() as u64,
        text_version: eidetic::kept_text::text_version(&text),
    };
    let page = KeptPage {
        url: body.facts.final_url,
        text,
        at_ms: 0,
        private: false,
    };
    Ok((page, record))
}

/// The viewer's side of one attached mere route.
pub(super) struct Link<T> {
    transport: T,
    graph: ProjectionSession,
}

impl Link<AppBrokerClient> {
    pub(super) async fn open(domain: &str) -> Result<Self, String> {
        let mut transport = AppBrokerClient::open_declared_route(
            AppId::new(graphshell::declaration::APP),
            AppRouteId::new(format!("mere/{domain}")).map_err(|error| error.to_string())?,
            graphshell::declaration::declaration(),
        )
        .await
        .map_err(|error| error.to_string())?;
        let opened = transport
            .request(CarrierRequestBody::Open(Box::new(SessionOpen {
                version: chirograph::ProtocolVersion::V1,
                capabilities: CapabilityProfile::new([PresentationCapability::PortableCard]),
            })))
            .await?;
        let CarrierResponseBody::Opened(opened) = opened else {
            return Err("The resident did not open the mere route.".into());
        };
        Self::attach(transport, &opened.descriptor)
    }
}

impl<T: RouteTransport> Link<T> {
    pub(super) fn attach(
        transport: T,
        descriptor: &chirograph::EndpointDescriptor,
    ) -> Result<Self, String> {
        let graph = descriptor
            .projections
            .iter()
            .map(|offer| &offer.request.session)
            .find(|session| session.0 == MERE_GRAPH)
            .cloned()
            .ok_or("This mere offers no attached graph session.")?;
        Ok(Self { transport, graph })
    }

    async fn read(&mut self) -> Result<RetainedPagesResponse, String> {
        let body = self
            .transport
            .request(CarrierRequestBody::RetainedPages(RetainedPagesRequest {
                session: self.graph.clone(),
                limits: ASK,
            }))
            .await?;
        match body {
            CarrierResponseBody::RetainedPages(pages) => Ok(*pages),
            other => Err(format!("The resident answered with {other:?}.")),
        }
    }

    async fn send(&mut self, intent: &str, payload: Vec<u8>) -> Result<IntentResult, String> {
        let body = self
            .transport
            .request(CarrierRequestBody::Intent(IntentInvocation {
                session: self.graph.clone(),
                target: InstanceId(0),
                observed_epoch: SceneEpoch(0),
                observed_revision: Revision(0),
                intent: intent.into(),
                payload,
            }))
            .await?;
        match body {
            CarrierResponseBody::Intent(result) => Ok(result),
            other => Err(format!("The resident answered with {other:?}.")),
        }
    }

    /// Do one thing, then read what is kept now. The reply says what happened
    /// in the person's words; a failure to read is its own message.
    pub(super) async fn run(
        &mut self,
        command: Command,
        page: Option<(KeptPage, FetchRecordV1)>,
    ) -> Reply {
        let notice = match command {
            Command::Refresh => None,
            Command::SetKeep(enabled) => {
                let payload = serde_json::to_vec(&KeepSettingV1 {
                    enabled,
                    ..KeepSettingV1::default()
                })
                .expect("setting encodes");
                Some(match self.send(KEEP_SETTING_INTENT, payload).await {
                    Ok(IntentResult::Accepted) if enabled => {
                        "Keeping page text is on. Pages you ask to keep are stored.".to_string()
                    },
                    Ok(IntentResult::Accepted) => "Keeping page text is off.".to_string(),
                    Ok(IntentResult::Rejected { reason }) => reason,
                    Ok(IntentResult::Stale { .. }) => "The resident moved on; try again.".into(),
                    Err(reason) => reason,
                })
            },
            Command::Keep(_) => Some(match page {
                None => "Nothing was captured.".to_string(),
                Some((page, record)) => {
                    let address = page.url.clone();
                    let payload = serde_json::to_vec(&KeepPagesV1 {
                        pages: vec![page],
                        fetches: vec![record],
                    })
                        .expect("page encodes");
                    match self.send(KEEP_PAGES_INTENT, payload).await {
                        Ok(IntentResult::Accepted) => format!("Kept the text of {address}."),
                        Ok(IntentResult::Rejected { reason }) => reason,
                        Ok(IntentResult::Stale { .. }) => "The resident moved on; try again.".into(),
                        Err(reason) => reason,
                    }
                },
            }),
        };
        Reply {
            pages: Some(self.read().await),
            notice,
        }
    }
}

/// The worker thread: opens its own route lazily, then serves commands. The
/// one-shot capture runs on a plain thread, because the fetch handle owns a
/// runtime and must not start one inside this one.
pub(super) fn spawn(
    domain: String,
    wake: cambium_rootstock::HostWake,
) -> (mpsc::Sender<Command>, mpsc::Receiver<Reply>) {
    let (sender, commands) = mpsc::channel::<Command>();
    let (answers, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("kept-text worker runtime");
        runtime.block_on(async move {
            let deadline = std::time::Duration::from_secs(8);
            let mut link: Option<Link<AppBrokerClient>> = None;
            while let Ok(command) = commands.recv() {
                let failure = |reason: String| Reply {
                    pages: Some(Err(reason.clone())),
                    notice: Some(reason),
                };
                let reply = async {
                    if link.is_none() {
                        link = Some(match tokio::time::timeout(deadline, Link::open(&domain)).await
                        {
                            Ok(Ok(link)) => link,
                            Ok(Err(reason)) => return failure(reason),
                            Err(_) => return failure("The resident connection timed out.".into()),
                        });
                    }
                    let page = match &command {
                        Command::Keep(address) => {
                            let address = address.clone();
                            let (done, result) = tokio::sync::oneshot::channel();
                            std::thread::spawn(move || {
                                let _ = done.send(capture(&address));
                            });
                            match result.await {
                                Ok(Ok(page)) => Some(page),
                                Ok(Err(reason)) => {
                                    // A capture that failed still re-reads, so the
                                    // panel shows the truth beside the reason.
                                    let mut reply = link
                                        .as_mut()
                                        .expect("opened")
                                        .run(Command::Refresh, None)
                                        .await;
                                    reply.notice = Some(reason);
                                    return reply;
                                },
                                Err(_) => return failure("The page fetch stopped.".into()),
                            }
                        },
                        _ => None,
                    };
                    let link = link.as_mut().expect("opened");
                    match tokio::time::timeout(deadline, link.run(command, page)).await {
                        Ok(reply) => reply,
                        Err(_) => failure("The resident operation timed out.".into()),
                    }
                }
                .await;
                if reply.pages.as_ref().is_some_and(Result::is_err) {
                    // Reopen next time rather than reuse a failed route.
                    link = None;
                }
                if answers.send(reply).is_err() {
                    break;
                }
                wake.wake();
            }
        });
    });
    (sender, receiver)
}

/// The panel's state. Everything but the surface's typed fields is derived
/// from the last reply.
pub(super) struct Panel {
    pub open: bool,
    pub surface: GoalSurface,
    pub address: TextInput,
    pub keeping: Option<RetainedKeeping>,
    pub notice: String,
    pub omitted: String,
    pub kept_pages: usize,
    pub busy: bool,
    sender: mpsc::Sender<Command>,
    receiver: mpsc::Receiver<Reply>,
}

impl Panel {
    pub(super) fn new(sender: mpsc::Sender<Command>, receiver: mpsc::Receiver<Reply>) -> Self {
        let mut surface = GoalSurface::new(Rc::new(Default::default()));
        surface.heading = false;
        surface.intro = Some(
            "Type what you are looking for. This reads only the page text you chose to keep in this mere; it fetches nothing."
                .into(),
        );
        Self {
            open: false,
            surface,
            address: TextInput::new(""),
            keeping: None,
            notice: String::new(),
            omitted: String::new(),
            kept_pages: 0,
            busy: false,
            sender,
            receiver,
        }
    }

    /// A panel with no worker, for hosts and tests that apply replies by hand.
    #[cfg(test)]
    pub(super) fn detached() -> Self {
        let (sender, _) = mpsc::channel();
        let (_, receiver) = mpsc::channel();
        Self::new(sender, receiver)
    }

    pub(super) fn send(&mut self, command: Command) {
        if self.busy {
            self.notice = "Still working on the last request.".into();
            return;
        }
        if self.sender.send(command).is_ok() {
            self.busy = true;
        } else {
            self.notice = "The kept-text worker stopped.".into();
        }
    }

    /// Apply every reply the worker has sent; whether any arrived.
    pub(super) fn drain(&mut self) -> bool {
        let mut changed = false;
        while let Ok(reply) = self.receiver.try_recv() {
            self.apply(reply);
            changed = true;
        }
        changed
    }

    pub(super) fn apply(&mut self, reply: Reply) {
        self.busy = false;
        if let Some(notice) = reply.notice {
            self.notice = notice;
        }
        match reply.pages {
            None => {},
            Some(Err(reason)) => {
                self.keeping = None;
                self.notice = reason;
            },
            Some(Ok(pages)) => match door::from_wire(OWNER, &pages) {
                Ok(snapshot) => {
                    self.kept_pages = pages.pages.len();
                    self.omitted = omitted(&pages);
                    self.keeping = Some(pages.keeping);
                    self.surface.replace_pages(Rc::new(snapshot));
                },
                Err(reason) => self.notice = format!("The resident's reply was refused: {reason}"),
            },
        }
    }

    /// The keep state in words a person reads.
    pub(super) fn state_text(&self) -> String {
        match &self.keeping {
            None => "Keep page text: not read yet.".into(),
            Some(RetainedKeeping::Off) => {
                "Keep page text: off. Nothing is being kept.".into()
            },
            Some(RetainedKeeping::On) => format!(
                "Keep page text: on. {} page{} kept.",
                self.kept_pages,
                if self.kept_pages == 1 { "" } else { "s" }
            ),
            Some(RetainedKeeping::Paused { reason }) => {
                format!("Keep page text: paused. {reason} Nothing is written until this clears.")
            },
        }
    }
}

fn omitted(pages: &RetainedPagesResponse) -> String {
    let t = pages.truncated;
    if t.is_complete() {
        return String::new();
    }
    format!(
        "Showing part of what is kept: {} page(s), {} older version(s) and {} link(s) left out by the read limit.",
        t.pages_omitted, t.versions_omitted, t.links_omitted
    )
}

pub(super) fn view(app: &App) -> Child {
    let panel = &app.kept;
    let paused = matches!(panel.keeping, Some(RetainedKeeping::Paused { .. }));
    let on = !matches!(panel.keeping, None | Some(RetainedKeeping::Off));
    let mut children: Vec<Child> = vec![
        Box::new(el("h2", "Find in what I kept")),
        Box::new(
            el("p", panel.state_text())
                .attr("id", "kept-state")
                .attr("class", if paused { "kept-state-paused" } else { "" }),
        ),
        Box::new(
            button(
                if on { "Stop keeping page text" } else { "Keep page text" },
                move |app: &mut App, _| app.kept.send(Command::SetKeep(!on)),
            )
            .attr("id", "kept-toggle"),
        ),
        Box::new(el(
            "label",
            (
                "Page to keep",
                lens(
                    |input: &mut TextInput| {
                        text_field_typed(input)
                            .attr("aria-label", "Page to keep")
                            .attr("class", "kept-input")
                            .attr("id", "kept-address")
                    },
                    |app: &mut App| &mut app.kept.address,
                ),
            ),
        )),
        Box::new(
            button("Keep this page's text", |app: &mut App, _| {
                let address = app.kept.address.text().trim().to_owned();
                if address.is_empty() {
                    app.kept.notice = "Type the address of the page to keep.".into();
                } else {
                    app.kept.send(Command::Keep(address));
                }
            })
            .attr("id", "kept-keep"),
        ),
        Box::new(
            button("Refresh what is kept", |app: &mut App, _| {
                app.kept.send(Command::Refresh)
            })
            .attr("id", "kept-refresh"),
        ),
        Box::new(
            el("p", panel.notice.clone())
                .attr("id", "kept-notice")
                .attr("role", "status"),
        ),
    ];
    if !panel.omitted.is_empty() {
        children.push(Box::new(
            el("p", panel.omitted.clone())
                .attr("id", "kept-omitted")
                .attr("class", "goal-notice"),
        ));
    }
    children.push(Box::new(el(
        "div",
        lens(
            |surface: &mut GoalSurface| goal_surface::view(surface),
            |app: &mut App| &mut app.kept.surface,
        ),
    )));
    Box::new(
        el("section", children)
            .attr("class", "kept-panel")
            .attr("aria-label", "Find in what I kept"),
    )
}

#[cfg(test)]
#[path = "kept_tests.rs"]
mod tests;
