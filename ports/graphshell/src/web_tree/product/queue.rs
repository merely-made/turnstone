// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A following view's storing actions that can be queued, and the writer's
//! admission of them. The follower sends a request through the extension page
//! (`web_follower_edit`) and shows it pending until the writer answers. The
//! writer applies it with `graphshell::follower_edit`, which calls the same
//! functions its own forms do, on the same completion path as a person's edit.
//! Everything else a follower could store stays view-only.
use super::*;
use crate::web_follower_edit as wire;
use graphshell::follower_edit::{
    Admissions, Edit, Reply, Request, Status, Verdict, answer, apply, base_of, normalized,
};

/// At most this many of a follower's edits wait for an answer at once.
const MAX_SENT: usize = 8;

struct Sent {
    id: String,
    what: String,
}

pub(super) struct Queue {
    /// Follower: edits sent and not yet answered.
    sent: Vec<Sent>,
    replies: Option<wire::Inbox<Reply>>,
    /// Writer: edits received, the ids already answered, and the one storing.
    requests: Option<wire::Inbox<Request>>,
    admissions: Admissions,
    admitting: Option<Request>,
    _listener: wire::Listener,
    wake: wire::Wake,
}

impl Queue {
    pub(super) fn new(following: bool) -> Result<Self, String> {
        let wake = wire::Wake::default();
        let (replies, requests, listener) = if following {
            let (inbox, listener) = wire::listen_for_replies(wake.clone())?;
            (Some(inbox), None, listener)
        } else {
            let (inbox, listener) = wire::listen_for_requests(wake.clone())?;
            (None, Some(inbox), listener)
        };
        Ok(Self {
            sent: Vec::new(),
            replies,
            requests,
            admissions: Admissions::new(),
            admitting: None,
            _listener: listener,
            wake,
        })
    }
}

fn refused(id: String, reason: String) -> Reply {
    Reply {
        id,
        view: String::new(),
        status: Status::Refused,
        reason,
        member: None,
    }
}

impl SavedProduct {
    pub(in crate::web_tree) fn bind_queue_wake(&mut self, wake: &cambium_rootstock::HostWake) {
        if let Some(queue) = &self.queue {
            *queue.wake.borrow_mut() = Some(wake.clone());
        }
    }

    // A completion can free the writer for the next queued edit. Signal the
    // host after publishing it, including when a normal save was already in
    // progress when the follower sent its request.
    pub(super) fn spawn_storage_task(&self, task: impl Future<Output = ()> + 'static) {
        let wake = self
            .queue
            .as_ref()
            .and_then(|queue| queue.wake.borrow().clone());
        wasm_bindgen_futures::spawn_local(async move {
            task.await;
            if let Some(wake) = wake {
                wake.wake();
            }
        });
    }

    /// Send `edit` to the view that edits and show it pending.
    fn send_edit(&mut self, edit: Edit, what: String) -> Result<(), String> {
        let app = self
            .app
            .as_ref()
            .ok_or("The graph is being read again · try again")?;
        let queue = self.queue.as_mut().ok_or(follow::VIEW_ONLY)?;
        if queue.sent.len() >= MAX_SENT {
            return Err("Edits are still waiting for the view that edits".into());
        }
        let request = Request {
            id: Uuid::new_v4().to_string(),
            view: wire::view_id(),
            base: base_of(app),
            edit,
        };
        wire::submit(&request)?;
        queue.sent.push(Sent {
            id: request.id,
            what: what.clone(),
        });
        self.status = format!("Sent to the view that edits · waiting · {what}");
        Ok(())
    }

    /// **Add an object** in a follower: the writer adds it.
    pub(super) fn queue_address(&mut self) {
        let address = self.intake_address.text().trim().to_owned();
        if address.is_empty() {
            self.status = "Enter an address before adding an object".into();
            return;
        }
        let title = self.intake_title.text().trim().to_owned();
        match self.send_edit(
            Edit::AddNode {
                address: address.clone(),
                title,
            },
            format!("add {address}"),
        ) {
            Ok(()) => {
                self.intake_address = TextInput::new("");
                self.intake_title = TextInput::new("");
            },
            Err(error) => self.status = error,
        }
    }

    /// **Send changes** in a follower: the writer saves the title and tags.
    pub(super) fn queue_metadata(&mut self) {
        let Some(member) = self.selected else {
            self.status = "Select an object before saving".into();
            return;
        };
        let held = match self.app.as_ref().map(|app| metadata(app, member)) {
            Some(Ok(held)) => held,
            Some(Err(error)) => {
                self.status = error;
                return;
            },
            None => {
                self.status = "The graph is being read again · try again".into();
                return;
            },
        };
        let title = self.title.text().trim().to_owned();
        let tags = normalized(
            &self
                .tags
                .text()
                .split(',')
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        );
        if let Err(error) = self.send_edit(
            Edit::EditNode {
                member,
                title: title.clone(),
                tags,
                was_title: held.title,
                was_tags: held.tags,
            },
            format!(
                "save {}",
                if title.is_empty() {
                    &held.address
                } else {
                    &title
                }
            ),
        ) {
            self.status = error;
        }
    }

    /// Whether an answer or an edit waits for the frame loop: an answer to
    /// show, or an edit this writer can start now.
    pub(in crate::web_tree) fn queue_ready(&self) -> bool {
        let Some(queue) = &self.queue else {
            return false;
        };
        let answers = queue
            .replies
            .as_ref()
            .is_some_and(|inbox| !inbox.borrow().is_empty());
        let edits = queue
            .requests
            .as_ref()
            .is_some_and(|inbox| !inbox.borrow().is_empty());
        answers
            || (edits
                && queue.admitting.is_none()
                && !self.following()
                && !self.selection_locked()
                && self.app.is_some())
    }

    /// Show the writer's answers to this follower's edits.
    pub(super) fn collect_replies(&mut self) -> bool {
        let Some(inbox) = self.queue.as_ref().and_then(|queue| queue.replies.clone()) else {
            return false;
        };
        let mut changed = false;
        let mut reread = false;
        while let Some(next) = inbox.borrow_mut().pop_front() {
            let queue = self.queue.as_mut().expect("queue");
            match next {
                Ok(reply) => {
                    // An answer to an edit this view no longer awaits is ignored.
                    let Some(at) = queue.sent.iter().position(|sent| sent.id == reply.id) else {
                        continue;
                    };
                    let sent = queue.sent.remove(at);
                    self.status = match reply.status {
                        Status::Admitted => format!("Admitted · {}", sent.what),
                        Status::Refused => {
                            reread = true;
                            format!("Refused · {} · {}", sent.what, reply.reason)
                        },
                    };
                },
                Err((_, error)) => self.status = format!("Unreadable answer · {error}"),
            }
            changed = true;
        }
        if reread {
            self.reread();
        }
        changed
    }

    /// Admit the next edit a follower sent, once nothing else is saving.
    pub(super) fn admit_next(&mut self) -> bool {
        if self.following() || self.selection_locked() || self.app.is_none() {
            return false;
        }
        let Some(queue) = self.queue.as_mut() else {
            return false;
        };
        let Some(inbox) = queue.requests.clone() else {
            return false;
        };
        if queue.admitting.is_some() {
            return false;
        }
        let Some(next) = inbox.borrow_mut().pop_front() else {
            return false;
        };
        let announce = |reply: &Reply| {
            if let Err(error) = wire::admitted(reply) {
                web_sys::console::error_1(&error.into());
            }
        };
        let request = match next {
            Ok(request) => request,
            Err((id, error)) => {
                if !id.is_empty() {
                    announce(&refused(id, error));
                }
                return true;
            },
        };
        let app = self.app.as_ref().expect("checked above");
        match queue.admissions.check(app, &request) {
            Verdict::Answered(reply) => {
                announce(&reply);
                return true;
            },
            Verdict::Proceed => {},
        }
        let mut app = self.app.take().expect("checked above");
        let edit = request.edit.clone();
        queue.admitting = Some(request);
        self.saving = true;
        self.save_state = "saving";
        self.status = "Saving an edit from another view…".into();
        let completed = self.completed.clone();
        // The task owns the app. No RefCell borrow crosses the IndexedDB await.
        self.spawn_storage_task(async move {
            let result = match apply(&mut app, &edit, now_secs()).await {
                Ok(Some(member)) => metadata(&app, member),
                Ok(None) => Err("the edit named no object".into()),
                Err(error) => Err(error),
            };
            *completed.borrow_mut() = Some((app, CompletedEdit::Admitted, result));
        });
        true
    }

    /// The edit being admitted has been stored or has failed: answer it.
    /// A person's selection and form are left as they were.
    pub(super) fn finish_admission(&mut self, result: Result<NodeMetadata, String>) {
        let Some(queue) = self.queue.as_mut() else {
            return;
        };
        let Some(request) = queue.admitting.take() else {
            return;
        };
        let reply = answer(&request, result.map(|saved| Some(saved.member)));
        let reply = queue.admissions.record(reply);
        if let Err(error) = wire::admitted(&reply) {
            web_sys::console::error_1(&error.into());
        }
        let (status, state) = match reply.status {
            Status::Admitted => ("Saved an edit from another view".to_owned(), "saved"),
            Status::Refused => (
                format!(
                    "An edit from another view was not stored · {} · any change stays pending here and the next save stores it",
                    reply.reason
                ),
                "error",
            ),
        };
        self.status = status;
        self.save_state = state;
    }
}
