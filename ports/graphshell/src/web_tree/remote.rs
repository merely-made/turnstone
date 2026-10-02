// Copyright 2026 Mark Alan Boykin
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

/// Which Graph tools sections are open. Both start open; the region scrolls.
pub(super) struct Sections {
    pub(super) physics: DisclosureState,
    pub(super) remote: DisclosureState,
}

impl Default for Sections {
    fn default() -> Self {
        Self {
            physics: DisclosureState::new("tools-physics", "Arrangement and physics")
                .expanded(true),
            remote: DisclosureState::new("tools-remote", "Remote session").expanded(true),
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
            live: None,
            joining: false,
            status: "no link".to_string(),
            form: ActionForm::new("Ready"),
            events: Vec::new(),
            board: RemoteBoard::new(),
            text: {
                let mut text = BoardText::new();
                text.register_font(include_bytes!("../../web/GraphshellSans.ttf").to_vec());
                text
            },
            generation: 0,
        }
    }

    /// Send what the session queued and collect its events.
    pub(super) fn pump(&mut self) {
        if let Some(live) = &mut self.live {
            live.flush();
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
        self.live
            .as_ref()
            .map_or(&self.status, |live| live.session.status())
    }

    pub(super) fn in_flight(&self) -> bool {
        self.joining || self.live.as_ref().is_some_and(|live| live.session.in_flight())
    }

    pub(super) fn mounted(&self) -> Option<&MountedScene> {
        self.live.as_ref()?.session.mounted()
    }

    pub(super) fn revision(&self) -> Option<u64> {
        self.live.as_ref()?.session.revision()
    }

    pub(super) fn label(&self) -> String {
        match &self.live {
            Some(live) => live.session.label(),
            None => format!("Remote projection · {}", self.status),
        }
    }

    /// The labels and descriptions of the advertised intents, in order.
    pub(super) fn actions(&self) -> Vec<(String, String)> {
        self.live.as_ref().map_or_else(Vec::new, |live| {
            live.session
                .actions()
                .into_iter()
                .map(|(_, action)| (action.label, action.explanation))
                .collect()
        })
    }

    /// Mirror `choice` onto the board and reconcile it to the scene.
    pub(super) fn sync_board(&mut self, choice: mere::canvas::PhysicsChoice) {
        let revision = self.revision();
        let mounted = self.live.as_ref().and_then(|live| live.session.mounted());
        self.board.sync(mounted, revision, choice);
    }
}

impl RemoteHost for TreeRemote {
    fn live(&mut self) -> Option<&mut LiveRemote> {
        self.live.as_mut()
    }

    fn adopt(&mut self, mut live: LiveRemote) {
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
        .with(|tree| tree.borrow().as_ref().map(|tree| tree.shared.remote.clone()))
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
        self.with_remote(|remote| match &mut remote.live {
            Some(live) => live.session.invoke_action(index),
            None => remote.form.status = "Failed · no remote link".to_string(),
        });
    }

    /// Carry the selects into the draft, then submit it.
    fn submit(&mut self) {
        let chosen: Vec<usize> = self.draft.fields.iter().map(|field| field.selected).collect();
        self.with_remote(|remote| {
            let Some(live) = &mut remote.live else {
                return;
            };
            let form = &mut live.session.form;
            if let Some(draft) = form.draft.as_ref().map(|draft| draft.semantics()) {
                for (field, selected) in draft.fields.iter().zip(chosen) {
                    if let Some(choice) = selected.checked_sub(1).and_then(|i| field.choices.get(i)) {
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
            if page.session == session { "true" } else { "false" },
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
    // The board's cards by title, in the scene's order.
    let titles: Vec<Child> = remote
        .live
        .as_ref()
        .map(|live| live.session.card_labels())
        .unwrap_or_default()
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
        children.push(Box::new(
            el(
                "div",
                vec![
                    command("Disconnect", TreePage::disconnect),
                    command("Reconnect", TreePage::reconnect),
                    command("Nudge host", TreePage::nudge),
                ],
            )
            .attr("class", "tools-link")
            .attr("role", "group")
            .attr("aria-label", "Link"),
        ));
    }
    children.push(Box::new(
        el("p", remote.form().status.clone())
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
