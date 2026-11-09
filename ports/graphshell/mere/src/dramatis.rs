// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Dramatis pane in the viewer: personae, quarantine review, vault lock
//! and passphrase change, served by Dramatis's projection surface in this
//! process and drawn as cards plus bounded action forms.
//!
//! Nothing connects to Djinn until the person opens the pane. Djinn keeps the
//! vault and shows the consent and passphrase prompts itself; this pane holds
//! no secret and takes no text input.

use super::{App, Child};
use cambium::{button, el};
use cambium_rootstock::HostWake;
use dramatis_surface::{Custody, Worker};
use graphshell_dramatis_pane::{Pane, view};

pub(super) type Mounted = Pane<Box<dyn Custody>>;

/// Where the pane lives and whether it is showing.
pub(super) struct Panel {
    pub open: bool,
    pane: Option<Mounted>,
    wake: Option<HostWake>,
}

impl Panel {
    pub(super) fn new(wake: Option<HostWake>) -> Self {
        Self {
            open: false,
            pane: None,
            wake,
        }
    }

    /// Mount a pane over `custody` (tests, and hosts that bring their own).
    #[cfg(test)]
    pub(super) fn mount(&mut self, custody: Box<dyn Custody>) {
        self.pane = Some(Pane::new(custody));
    }

    /// Open or close the pane. The first open starts the Djinn worker.
    pub(super) fn toggle(&mut self) {
        self.open = !self.open;
        if self.open && self.pane.is_none() {
            if let Some(wake) = &self.wake {
                let worker = Worker::spawn(dramatis_surface::endpoint(), wake.callback());
                self.pane = Some(Pane::new(Box::new(worker)));
            }
        }
    }

    /// Take in what the worker has answered, on the host's wake.
    pub(super) fn drain(&mut self) {
        if let Some(pane) = self.pane.as_mut() {
            pane.refresh();
        }
    }

    pub(super) fn pane_mut(&mut self) -> Option<&mut Mounted> {
        self.pane.as_mut()
    }
}

pub(super) const CSS: &str = ".dramatis-pane{height:560px;overflow:auto;border-top:1px solid #6d7783}.dramatis-pane .card{background:#1d2127;border-color:#4a525a}.dramatis-pane button{background:#22262c;color:#e8e6e1;border-color:#6d7783}.dramatis-pane button[aria-pressed=true]{border-color:#8cb4de}";

/// The pane's own stylesheet, then the viewer's colours over it.
pub(super) fn css() -> String {
    format!("{}\n{}", graphshell_dramatis_pane::SHEET, CSS)
}

pub(super) fn toggle(app: &App) -> Child {
    Box::new(
        button(
            if app.dramatis.open {
                "Back to the graph"
            } else {
                "Personae and imports"
            },
            |app: &mut App, _| {
                if !app.dramatis.open {
                    app.kept.open = false;
                }
                app.dramatis.toggle();
                app.relayout();
            },
        )
        .attr("id", "dramatis-open"),
    )
}

pub(super) fn pane_view(app: &App) -> Child {
    match &app.dramatis.pane {
        Some(pane) => view(pane, |app: &mut App| {
            app.dramatis
                .pane_mut()
                .expect("the pane is drawn only while it is mounted")
        }),
        None => Box::new(el("p", "Dramatis is not connected.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_tests::app;
    use castellan_client::quarantine::ReviewAction;
    use dramatis::roster::{Roster, RosterEntry};
    use dramatis::view::VaultLockView;
    use dramatis_surface::{Reply, Request, Snapshot};
    use genet_scripted_dom::{NodeId, ScriptedDom};
    use layout_dom_api::{LayoutDom, Namespace};
    use personae::ProfileId;
    use std::{cell::RefCell, rc::Rc};

    #[derive(Clone, Default)]
    struct Fake {
        sent: Rc<RefCell<Vec<Request>>>,
        replies: Rc<RefCell<Vec<(Reply, bool)>>>,
    }
    impl Custody for Fake {
        fn send(&mut self, request: Request) {
            self.sent.borrow_mut().push(request);
        }
        fn drain(&mut self) -> Vec<(Reply, bool)> {
            std::mem::take(&mut *self.replies.borrow_mut())
        }
    }

    fn snapshot() -> Snapshot {
        let entry = |id: &str, name: &str, chosen| RosterEntry {
            id: ProfileId(id.into()),
            display_name: name.into(),
            slot_count: 0,
            chosen,
        };
        Snapshot {
            lock: VaultLockView::Unlocked,
            roster: Roster {
                chosen: ProfileId("family".into()),
                description: String::new(),
                entries: vec![entry("family", "Family", true), entry("work", "Work", false)],
            },
            cards: vec![],
            unassigned: vec![],
        }
    }

    fn text(dom: &ScriptedDom, node: NodeId) -> String {
        let mut content = dom.text(node).unwrap_or("").to_owned();
        for child in dom.dom_children(node) {
            content.push_str(&text(dom, child));
        }
        content
    }
    fn button(dom: &ScriptedDom, label: &str) -> Option<NodeId> {
        let mut nodes = vec![dom.document()];
        while let Some(node) = nodes.pop() {
            if dom
                .element_name(node)
                .is_some_and(|name| name.local.to_string() == "button")
                && dom.attribute(node, &Namespace::from(""), &"disabled".into()).is_none()
                && text(dom, node) == label
            {
                return Some(node);
            }
            nodes.extend(dom.dom_children(node));
        }
        None
    }

    fn mounted() -> (App, Fake) {
        let fake = Fake::default();
        fake.replies
            .borrow_mut()
            .push((Reply { snapshot: Some(snapshot()), notice: String::new() }, true));
        let (mut app, ..) = app();
        app.dramatis.mount(Box::new(fake.clone()));
        app.dramatis.open = true;
        (app, fake)
    }

    #[test]
    fn nothing_connects_until_the_pane_is_opened() {
        let (app, ..) = app();
        assert!(!app.dramatis.open);
        assert!(app.dramatis.pane.is_none());
    }

    #[test]
    fn opening_the_pane_closes_the_kept_panel_and_the_graph_gives_it_the_room() {
        let (mut app, ..) = mounted();
        app.kept.open = true;
        app.dramatis.open = true;
        assert_eq!(app.reserved(), 640);
        app.dramatis.open = false;
        app.kept.open = false;
        assert_ne!(app.reserved(), 640);
    }

    #[test]
    fn a_real_click_on_a_persona_card_reaches_the_surface_as_a_choice() {
        let (app, fake) = mounted();
        let dom: cambium::DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let mut runner =
            cambium::GenetAppRunner::new(dom.clone(), pane_view as fn(&App) -> Child, app);
        let choose = button(&dom.borrow(), "Choose Work").expect("Work is offered");
        assert!(button(&dom.borrow(), "Choose Family").is_none(), "the chosen persona has no choice");
        let _ = runner.dispatch_click(choose, cambium::PointerClick::at((1., 1.)));
        assert!(matches!(
            fake.sent.borrow().as_slice(),
            [Request::Select(ProfileId(p))] if p == "work"
        ));
        let _ = ReviewAction::Accept;
    }
}
