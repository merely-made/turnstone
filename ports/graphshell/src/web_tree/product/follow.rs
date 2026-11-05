// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A second view of the installed profile (P0): it observes the session the
//! writer commits through a store handle that refuses writes, reads the store
//! again after each of the writer's saves, and sends the edits `queue` can
//! carry to the writer; every other storing action is refused here.
use std::cell::Cell;

use super::*;
use crate::web_capture::{BatchListener, listen_for_commits};

/// What a follower answers when asked to store a change.
pub(in crate::web_tree) const VIEW_ONLY: &str =
    "Viewing only · edit in the view that edits, or choose Edit here";

pub(super) struct Following {
    requested: Rc<Cell<bool>>,
    refreshing: bool,
    /// Counts adopted refreshes, so panes keyed on the unchanged session id
    /// know to read the store again.
    generation: u64,
    completed: Rc<RefCell<Option<Result<App, String>>>>,
    _listener: BatchListener,
}

impl Following {
    pub(super) fn new() -> Result<Self, String> {
        let requested = Rc::new(Cell::new(false));
        Ok(Self {
            _listener: listen_for_commits(requested.clone())?,
            requested,
            refreshing: false,
            generation: 0,
            completed: Rc::new(RefCell::new(None)),
        })
    }
}

impl SavedProduct {
    pub(in crate::web_tree) fn following(&self) -> bool {
        self.following.is_some()
    }

    /// How many refreshes this follower has adopted (0 when it is the writer).
    pub(in crate::web_tree) fn refreshes(&self) -> u64 {
        self.following.as_ref().map_or(0, |follow| follow.generation)
    }

    /// A follower's refusal of anything that would store a change.
    pub(in crate::web_tree) fn view_only(&self) -> Result<(), String> {
        if self.following() {
            Err(VIEW_ONLY.into())
        } else {
            Ok(())
        }
    }

    /// Read the store again, as if the writer had just saved.
    pub(super) fn reread(&mut self) {
        if let Some(follow) = &self.following {
            follow.requested.set(true);
        }
    }

    pub(super) fn refresh_ready(&self) -> bool {
        self.following.as_ref().is_some_and(|follow| {
            follow.completed.borrow().is_some()
                || (follow.requested.get() && !follow.refreshing && self.app.is_some())
        })
    }

    /// Observe the store again; the current graph stays shown meanwhile.
    pub(super) fn start_refresh(&mut self) {
        let (Some(follow), Some(app), Some(profile)) = (
            self.following.as_mut(),
            self.app.as_ref(),
            self.local_profile.clone(),
        ) else {
            return;
        };
        if follow.refreshing || !follow.requested.replace(false) {
            return;
        }
        follow.refreshing = true;
        let backend = app.host.store();
        let completed = follow.completed.clone();
        self.spawn_storage_task(async move {
            let observed =
                GraphshellApp::observe_local(backend, &profile, js_sys::Date::now() as u64)
                    .await
                    .map_err(|error| error.to_string());
            *completed.borrow_mut() = Some(observed);
        });
    }

    pub(super) fn finish_refresh(&mut self, canvas: &mut Canvas) -> bool {
        let Some(follow) = self.following.as_mut() else {
            return false;
        };
        let Some(observed) = follow.completed.borrow_mut().take() else {
            return false;
        };
        follow.refreshing = false;
        match observed {
            Ok(app) => {
                follow.generation += 1;
                let owner = app.host.graph_session().id().0;
                graphshell::local_scene::adopt_host_graph(
                    canvas,
                    self.session.parse().ok(),
                    owner,
                    &app.host,
                );
                self.session = owner.to_string();
                if self
                    .selected
                    .is_some_and(|id| app.host.graph().get_node_by_id(id).is_none())
                {
                    self.selected = None;
                }
                self.app = Some(app);
                self.goal_stale();
            },
            Err(error) => self.status = format!("Could not read the other view's saves · {error}"),
        }
        true
    }
}
