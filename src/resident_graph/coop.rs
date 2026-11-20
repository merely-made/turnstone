// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cambium::button;
use mere_view::{MereViewModel, ViewStatus};
use pandect::SessionId;

use super::{GraphView, State};
use crate::graph_coop::{
    controls::{self, Context},
    worker::{Command, Panel},
};

pub(super) struct OwnerPanel {
    domain: String,
    panel: Option<Panel>,
    dirty: Arc<AtomicBool>,
    refresh: Option<SessionId>,
}

impl OwnerPanel {
    pub(super) fn new(domain: String) -> Self {
        Self {
            domain,
            panel: None,
            dirty: Arc::new(AtomicBool::new(false)),
            refresh: None,
        }
    }

    pub(super) fn close(&mut self) {
        self.panel = None;
        self.refresh = None;
    }

    fn toggle(&mut self, wake: armillary::Wake) {
        if self.is_open() {
            self.close();
        } else {
            let dirty = self.dirty.clone();
            self.panel = Some(Panel::new(
                self.domain.clone(),
                None,
                Arc::new(move || {
                    dirty.store(true, Ordering::Release);
                    wake();
                }),
            ));
        }
    }

    pub(super) fn is_open(&self) -> bool {
        self.panel.is_some()
    }

    pub(super) fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }

    pub(super) fn refresh(&mut self, graph: Option<SessionId>) {
        self.refresh = graph.filter(|graph| {
            self.panel.as_ref().is_some_and(|panel| {
                panel
                    .update
                    .data
                    .as_ref()
                    .is_some_and(|data| data.graph == *graph)
            })
        });
    }

    pub(super) fn sync(&mut self) {
        if let Some(panel) = &mut self.panel {
            panel.drain();
            if !panel.busy()
                && let Some(graph) = self.refresh.take()
                && panel
                    .update
                    .data
                    .as_ref()
                    .is_some_and(|data| data.graph == graph)
            {
                panel.send(Command::Read(graph));
            }
        }
    }
}

pub(super) fn attached(model: &MereViewModel) -> Option<SessionId> {
    model.sessions.iter().find(|s| s.attached).map(|s| s.id)
}

fn context(state: &State) -> Context {
    Context {
        attached: attached(&state.model),
        focused: state
            .focused
            .as_deref()
            .and_then(|id| uuid::Uuid::parse_str(id).ok()),
        busy: state.requested != state.completed
            || !matches!(state.model.status, ViewStatus::Ready),
    }
}

pub(super) fn toggle_button(state: &State) -> GraphView {
    Box::new(
        button("Co-op", |state: &mut State, _| {
            state.coop.toggle(state.wake.clone())
        })
        .attr("id", "coop-open")
        .attr(
            "aria-pressed",
            if state.coop.is_open() {
                "true"
            } else {
                "false"
            },
        ),
    )
}

pub(super) fn view(state: &State) -> GraphView {
    controls::view(
        state,
        |s| s.coop.panel.as_ref(),
        |s| s.coop.panel.as_mut(),
        context,
    )
}

pub(super) fn observation(state: &State) -> String {
    controls::observe(
        state.coop.panel.as_ref(),
        &state.model,
        state.focused.as_deref(),
        state.record.as_ref(),
    )
}
