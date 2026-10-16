// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The "Graph tools" region's remote session section, and where the tree
//! keeps the session.
//!
//! The session's rules are `graphshell_client::RemoteSession`'s and its
//! channel is `web_rtc_link`'s, both shared with the old page. The tree keeps
//! them in [`TreeRemote`], beside its canvas in the shared state, because the
//! channel's pumps run outside the Cambium runner. The section holds the
//! session switch (two pressed-state buttons), the active-session line, one
//! button per advertised intent, the draft form for an intent with inputs,
//! the link's disconnect, reconnect and nudge, and the action status.
use super::*;
use cambium::{DisclosureState, SelectState, button, disclosure, lens, select};
use graphshell::client::remote::ActionForm;
use graphshell::client::{ActionDraftSemantics, MountedScene};
use graphshell::remote_board::RemoteBoard;
use mere::canvas::{BoardFit, BoardText};

use super::super::web_rtc_link::{self, LiveRemote, RemoteHost};

/// The board on the tree: no chrome floats over the leaf, so the margins are
/// even (the old page keeps 50/50/116/64).
pub(super) const BOARD_FIT: BoardFit = BoardFit {
    left: 24.0,
    right: 24.0,
    top: 24.0,
    bottom: 24.0,
    frame_edges: true,
};
/// The draft form's unset choice.
const CHOOSE: &str = "Choose…";

/// Which Graph tools sections are open. Arrangement and physics starts
/// open; Remote session starts closed and opens when a link is established or
/// remote is shown ("Remote starts collapsed"). The region scrolls.
pub(super) struct Sections {
    pub(super) physics: DisclosureState,
    pub(super) remote: DisclosureState,
}

impl Default for Sections {
    fn default() -> Self {
        Self {
            physics: DisclosureState::new("tools-physics", "Arrangement and physics")
                .expanded(true),
            remote: DisclosureState::new("tools-remote", "Remote session").expanded(false),
        }
    }
}

/// Which session the canvas leaf shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Session {
    Local,
    Remote,
}

/// The tree's remote session: absent until a join completes.
pub(super) struct TreeRemote {
    pub(super) reservoir: Option<mere_overmap::route_session::MereRouteSession>,
    pub(super) live: Option<LiveRemote>,
    /// A first join in flight, before any session exists.
    joining: bool,
    /// The link's status before a session exists.
    status: String,
    /// The action surface before a session exists.
    form: ActionForm,
    /// Receipt events, drained by the scenario lane.
    pub(super) events: Vec<String>,
    pub(super) board: RemoteBoard,
    /// The faces the board sets its card titles in: the page's own.
    pub(super) text: BoardText,
    /// Bumped on every change, so the frame hook knows to rebuild the view.
    pub(super) generation: u64,
}

impl TreeRemote {
    pub(super) fn new() -> Self {
        Self {
            reservoir: None,
            live: None,
            joining: false,
            status: "no link".to_string(),
            form: ActionForm::new("Ready"),
            events: Vec::new(),
            board: RemoteBoard::new(),
            text: {
                let mut text = BoardText::new();
                text.register_font(include_bytes!("../../web/IBMPlexSans-Regular.ttf").to_vec());
                text
            },
            generation: 0,
        }
    }

    /// Send what the session queued and collect its events.
    pub(super) fn pump(&mut self) {
        if let Some(live) = &mut self.live {
            if self.reservoir.is_none() && !live.session.in_flight() {
                if let Some(descriptor) = live
                    .session
                    .driver()
                    .core()
                    .map(|core| core.descriptor())
                    .filter(|descriptor| mere_overmap::route::requests(descriptor).is_some())
                {
                    match mere_overmap::route_session::MereRouteSession::from_descriptor(
                        descriptor.clone(),
                        live.session.profile().clone(),
                        "graphshell",
                    ) {
                        Ok(session) => self.reservoir = Some(session),
                        Err(reason) => live.session.fail(reason),
                    }
                }
            }
            if let Some(session) = &mut self.reservoir {
                if let Err(reason) = live.send_lines(session.take_outgoing()) {
                    session.fail(reason);
                }
            } else {
                live.flush();
            }
            self.events.extend(live.session.take_events());
        }
        self.generation = self.generation.wrapping_add(1);
    }

    pub(super) fn form(&self) -> &ActionForm {
        self.live
            .as_ref()
            .map_or(&self.form, |live| &live.session.form)
    }

    pub(super) fn form_mut(&mut self) -> &mut ActionForm {
        match &mut self.live {
            Some(live) => &mut live.session.form,
            None => &mut self.form,
        }
    }

    pub(super) fn status(&self) -> &str {
        if let Some(session) = &self.reservoir {
            if let Some(live) = &self.live
                && live.session.in_flight()
            {
                return live.session.status();
            }
            return session.status();
        }
        self.live
            .as_ref()
            .map_or(&self.status, |live| live.session.status())
    }

    pub(super) fn in_flight(&self) -> bool {
        if let Some(session) = &self.reservoir {
            return self.joining
                || self.live.as_ref().map_or_else(
                    || session.in_flight(),
                    |live| session.in_flight_with_transport(&live.session),
                );
        }
        self.joining
            || self
                .live
                .as_ref()
                .is_some_and(|live| live.session.in_flight())
    }

    pub(super) fn mounted(&self) -> Option<&MountedScene> {
        if let Some(session) = &self.reservoir {
            return session.primary();
        }
        self.live.as_ref()?.session.mounted()
    }

    pub(super) fn card_labels(&self) -> Vec<String> {
        if let Some(session) = &self.reservoir {
            return session
                .model()
                .graph
                .nodes
                .iter()
                .map(|node| node.label.clone())
                .collect();
        }
        self.live
            .as_ref()
            .map(|live| live.session.card_labels())
            .unwrap_or_default()
    }
    pub(super) fn revision(&self) -> Option<u64> {
        if let Some(session) = &self.reservoir {
            return session.primary().map(|mounted| mounted.scene.revision.0);
        }
        self.live.as_ref()?.session.revision()
    }

    pub(super) fn label(&self) -> String {
        if let Some(session) = &self.reservoir {
            return format!("Shared mere · {}", session.model().title);
        }
        match &self.live {
            Some(live) => live.session.label(),
            None => format!("Remote projection · {}", self.status),
        }
    }

    /// The labels and descriptions of the advertised intents, in order.
    pub(super) fn actions(&self) -> Vec<(String, String)> {
        if self.reservoir.is_some() {
            return Vec::new();
        }
        self.live.as_ref().map_or_else(Vec::new, |live| {
            live.session
                .actions()
                .into_iter()
                .map(|(_, action)| (action.label, action.explanation))
                .collect()
        })
    }

    /// Mirror the viewer's live stage and `speed` onto the board and reconcile
    /// it to the scene.
    pub(super) fn sync_board(
        &mut self,
        stage: Option<mere::canvas::dynamics_spec::DynamicsSpec>,
        speed: mere::canvas::Speed,
    ) {
        let revision = self.revision();
        let identity = self
            .reservoir
            .as_ref()
            .and_then(|session| session.model().sessions.iter().find(|entry| entry.attached))
            .map(|entry| {
                format!(
                    "{}:{}",
                    entry.id.as_uuid(),
                    self.reservoir.as_ref().unwrap().model().layout
                )
            });
        let mounted = match &self.reservoir {
            Some(session) => session.primary(),
            None => self.live.as_ref().and_then(|live| live.session.mounted()),
        };
        if self.reservoir.is_some() {
            self.board
                .sync_reading(mounted, revision, identity.as_deref());
        } else {
            self.board.sync_scoped(
                mounted,
                revision,
                identity.as_deref(),
                stage.as_ref(),
                speed,
            );
        }
        if let Some(session) = &self.reservoir {
            self.board.set_focus(session.focus_instance().as_deref());
        }
    }
}

impl RemoteHost for TreeRemote {
    fn received(&mut self, line: &str) {
        if let Some(session) = &mut self.reservoir {
            session.on_line(line);
        } else if let Some(live) = &mut self.live {
            live.session.on_line(line);
        }
    }
    fn stream_closed(&mut self, reason: Option<String>) {
        if let Some(session) = &mut self.reservoir {
            session.fail(
                reason
                    .clone()
                    .unwrap_or_else(|| "The resident route closed.".into()),
            );
        }
        if let Some(live) = &mut self.live {
            match reason {
                Some(reason) => live.session.link_failed(reason),
                None => live.session.channel_closed(),
            }
        }
    }
    fn rejoined(&mut self) {
        if self.reservoir.take().is_some() {
            if let Some(live) = &mut self.live {
                live.session =
                    graphshell_client::RemoteSession::new(web_rtc_link::remote_profile());
                live.session.joined();
            }
        } else if let Some(live) = &mut self.live {
            live.session.rejoined();
        }
    }
    fn live(&mut self) -> Option<&mut LiveRemote> {
        self.live.as_mut()
    }

    fn adopt(&mut self, mut live: LiveRemote) {
        self.reservoir = None;
        live.session.form = std::mem::take(&mut self.form);
        self.live = Some(live);
        self.joining = false;
    }

    fn pre_join(&mut self, status: &str) {
        self.joining = true;
        self.status = status.to_string();
    }

    fn join_failed(&mut self, error: String) {
        self.joining = false;
        self.status = format!("error: {error}");
        self.form.status = format!("Failed · remote: {error}");
        self.events.push(format!("remote-error {error}"));
    }

    fn changed(&mut self) {
        self.pump();
    }
}

/// Join a host over WebRTC as the tree's remote session.
pub(crate) fn connect(signal_url: String, invite: Option<String>) -> Result<(), String> {
    let remote = TREE
        .with(|tree| {
            tree.borrow()
                .as_ref()
                .map(|tree| tree.shared.remote.clone())
        })
        .ok_or("the tree has not booted")?;
    web_rtc_link::connect(remote, signal_url, invite);
    Ok(())
}

/// The draft form's controls: one select per field, the first option unset.
#[derive(Default)]
pub(super) struct DraftControls {
    /// The draft these were built for, by intent.
    intent: Option<String>,
    pub(super) fields: Vec<SelectState>,
}

impl TreePage {
    fn show(&mut self, session: Session) {
        if session == Session::Local && self.session == Session::Remote {
            // The local law did not step while the board was shown.
            self.shared.canvas.borrow_mut().reset_frame_time();
        }
        self.session = session;
        if session == Session::Remote {
            // Showing the board opens its section ("opens when remote is shown").
            self.sections.remote.expanded = true;
        }
        self.shared.remote_shown.set(session == Session::Remote);
        self.shared.dirty.set(true);
    }

    /// Run `f` on the session, then send and collect what it produced.
    fn with_remote(&mut self, f: impl FnOnce(&mut TreeRemote)) {
        let mut remote = self.shared.remote.borrow_mut();
        f(&mut remote);
        remote.pump();
        let draft = remote.form().draft.as_ref().map(|draft| draft.semantics());
        drop(remote);
        self.sync_draft(draft.as_ref());
    }

    /// Rebuild the draft's selects when a different draft opens.
    fn sync_draft(&mut self, draft: Option<&ActionDraftSemantics>) {
        let intent = draft.map(|draft| draft.label.clone());
        if intent == self.draft.intent {
            return;
        }
        self.draft.intent = intent;
        self.draft.fields = draft.map_or_else(Vec::new, |draft| {
            draft
                .fields
                .iter()
                .map(|field| SelectState::new(0).with_label(field.label.clone()))
                .collect()
        });
    }

    fn invoke(&mut self, index: usize) {
        if self.shared.remote.borrow().reservoir.is_some() {
            return;
        }
        self.with_remote(|remote| match &mut remote.live {
            Some(live) => live.session.invoke_action(index),
            None => remote.form.status = "Failed · no remote link".to_string(),
        });
    }

    /// Carry the selects into the draft, then submit it.
    fn submit(&mut self) {
        if self.shared.remote.borrow().reservoir.is_some() {
            return;
        }
        let chosen: Vec<usize> = self
            .draft
            .fields
            .iter()
            .map(|field| field.selected)
            .collect();
        self.with_remote(|remote| {
            let Some(live) = &mut remote.live else {
                return;
            };
            let form = &mut live.session.form;
            if let Some(draft) = form.draft.as_ref().map(|draft| draft.semantics()) {
                for (field, selected) in draft.fields.iter().zip(chosen) {
                    if let Some(choice) = selected.checked_sub(1).and_then(|i| field.choices.get(i))
                    {
                        form.choose(&field.name, &choice.value);
                    }
                }
            }
            live.session.submit_draft();
        });
    }

    fn cancel(&mut self) {
        self.with_remote(|remote| {
            let form = remote.form_mut();
            form.close();
            form.status = "Ready".to_string();
        });
    }

    fn disconnect(&mut self) {
        self.with_remote(|remote| {
            if let Err(status) = web_rtc_link::disconnect(remote) {
                remote.form_mut().status = status;
            }
        });
    }

    fn reconnect(&mut self) {
        let state = self.shared.remote.clone();
        self.with_remote(|remote| {
            if let Err(status) = web_rtc_link::reconnect(state, remote) {
                remote.form_mut().status = status;
            }
        });
    }

    fn nudge(&mut self) {
        let state = self.shared.remote.clone();
        self.with_remote(|remote| {
            if let Err(status) = web_rtc_link::nudge(state, remote) {
                remote.form_mut().status = status;
            }
        });
    }
}

/// The board's slot as an accessible list: one item per card, named by its
/// title, at the rectangle it is painted in.
pub(super) fn board_semantics(
    remote: &TreeRemote,
    width: u32,
    height: u32,
) -> cambium_rootstock::ProducerSemantics {
    use cambium_rootstock::{ProducerNode, ProducerRole, ProducerSemantics};
    let children = if remote.mounted().is_some() {
        remote
            .board
            .scene()
            .card_rects(remote.board.board(), width, height, BOARD_FIT)
            .into_iter()
            .enumerate()
            .map(|(ordinal, (id, title, rect))| ProducerNode {
                // A card's id is its scene instance.
                key: id.parse().unwrap_or(ordinal as u64),
                role: ProducerRole::ListItem,
                name: title,
                rect: [rect.x, rect.y, rect.width, rect.height],
                actions: Vec::new(),
            })
            .collect()
    } else {
        Vec::new()
    };
    ProducerSemantics {
        role: Some(ProducerRole::List),
        name: Some(match &remote.reservoir {
            Some(session) => format!(
                "Resident graph · {} · {} records",
                session.model().title,
                children.len(),
            ),
            None => format!("Remote board · {} cards", children.len()),
        }),
        children,
    }
}

/// The active-session line.
pub(super) fn active_line(page: &TreePage) -> String {
    match page.session {
        Session::Local => format!(
            "Local Mere · {} objects",
            page.shared.canvas.borrow().graph().node_count()
        ),
        Session::Remote => page.shared.remote.borrow().label(),
    }
}

fn switch(label: &'static str, session: Session, page: &TreePage) -> Child {
    Box::new(
        button(label, move |page: &mut TreePage, _| page.show(session)).attr(
            "aria-pressed",
            if page.session == session {
                "true"
            } else {
                "false"
            },
        ),
    )
}

fn command(label: &'static str, action: fn(&mut TreePage)) -> Child {
    Box::new(button(label, move |page: &mut TreePage, _| action(page)))
}

/// The draft form, when an intent with inputs is open.
fn draft_form(page: &TreePage, draft: &ActionDraftSemantics) -> Child {
    let mut children: Vec<Child> = vec![
        Box::new(el("h3", draft.label.clone())),
        Box::new(el("p", draft.explanation.clone()).attr("class", "tools-caption")),
    ];
    for (index, field) in draft.fields.iter().enumerate() {
        if index >= page.draft.fields.len() {
            break;
        }
        let options: Vec<String> = std::iter::once(CHOOSE.to_string())
            .chain(field.choices.iter().map(|choice| choice.label.clone()))
            .collect();
        children.push(Box::new(el(
            "label",
            (
                el("span", field.label.clone()).attr("class", "tools-caption"),
                lens(
                    move |state: &mut SelectState| {
                        let options: Vec<&str> = options.iter().map(String::as_str).collect();
                        select(state, &options)
                    },
                    move |page: &mut TreePage| &mut page.draft.fields[index],
                ),
            ),
        )));
    }
    if let Some(error) = &draft.error {
        children.push(Box::new(
            el("p", error.clone())
                .attr("class", "tools-status")
                .attr("role", "alert"),
        ));
    }
    // "Submit" rather than the action's own label, which its button in the
    // actions group already carries; the form is named by the action.
    children.push(Box::new(
        button("Submit", |page: &mut TreePage, _| page.submit())
            .attr("aria-description", draft.submit_label.clone()),
    ));
    children.push(command("Cancel", TreePage::cancel));
    Box::new(
        el("form", children)
            .attr("class", "tools-draft")
            .attr("aria-label", draft.label.clone()),
    )
}

/// The "Graph tools" region's remote session section.
pub(super) fn section(page: &TreePage) -> Child {
    let remote = page.shared.remote.borrow();
    let mut children: Vec<Child> = vec![
        Box::new(
            el(
                "div",
                vec![
                    switch("Local Mere", Session::Local, page),
                    switch("Remote mount", Session::Remote, page),
                ],
            )
            .attr("class", "tools-switch")
            .attr("role", "group")
            .attr("aria-label", "Session"),
        ),
        Box::new(
            el("p", active_line(page))
                .attr("class", "tools-active")
                .attr("role", "status"),
        ),
    ];
    // The board steps at the viewer's own dial (ruled 2026-10-04, "The
    // viewer's own dial").
    if remote.mounted().is_some() {
        children.push(Box::new(
            el("p", crate::web_speed::board_line(remote.board.speed()))
                .attr("class", "tools-status")
                .attr("aria-label", "Remote board speed"),
        ));
    }
    // The board's cards by title, in the scene's order.
    let titles: Vec<Child> = remote
        .card_labels()
        .into_iter()
        .map(|title| Box::new(el("li", title).attr("role", "listitem")) as Child)
        .collect();
    if !titles.is_empty() {
        children.push(Box::new(
            el("ul", titles)
                .attr("class", "tools-cards")
                .attr("role", "list")
                .attr("aria-label", "Cards"),
        ));
    }
    let actions: Vec<Child> = remote
        .actions()
        .into_iter()
        .enumerate()
        .map(|(index, (label, explanation))| {
            Box::new(
                button(label, move |page: &mut TreePage, _| page.invoke(index))
                    .attr("aria-description", explanation),
            ) as Child
        })
        .collect();
    if !actions.is_empty() {
        children.push(Box::new(
            el("div", actions)
                .attr("class", "tools-actions")
                .attr("role", "group")
                .attr("aria-label", "Remote actions"),
        ));
    }
    if let Some(draft) = remote.form().draft.as_ref().map(|draft| draft.semantics())
        && !draft.fields.is_empty()
    {
        children.push(draft_form(page, &draft));
    }
    if remote.live.is_some() {
        let mut link = vec![
            command("Disconnect", TreePage::disconnect),
            command("Reconnect", TreePage::reconnect),
        ];
        if remote.reservoir.is_none() {
            link.push(command("Nudge host", TreePage::nudge));
        }
        children.push(Box::new(
            el("div", link)
                .attr("class", "tools-link")
                .attr("role", "group")
                .attr("aria-label", "Link"),
        ));
    }
    children.push(Box::new(
        el(
            "p",
            if remote.reservoir.is_some() {
                remote.status().to_owned()
            } else {
                remote.form().status.clone()
            },
        )
        .attr("class", "tools-status")
        .attr("role", "status"),
    ));
    drop(remote);
    Box::new(
        el(
            "section",
            disclosure(&page.sections.remote, children, |page: &mut TreePage| {
                page.sections.remote.toggle()
            }),
        )
        .attr("class", "tools-section")
        .attr("aria-label", "Remote session"),
    )
}

/// Existing session operations shared by toolbar controls and command API.
pub(super) fn run_command(page: &mut TreePage, command: &str) -> bool {
    match command {
        "session-local" => page.show(Session::Local),
        "session-remote" => page.show(Session::Remote),
        "remote-disconnect" => page.disconnect(),
        "remote-reconnect" => page.reconnect(),
        "remote-nudge" => page.nudge(),
        "invoke-action" if page.session == Session::Remote => page.invoke(0),
        _ => return false,
    }
    true
}
