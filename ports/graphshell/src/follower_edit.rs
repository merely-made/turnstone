// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Edits a following view hands to the writer, and the writer's admission of
//! them.
//!
//! A follower holds a store that refuses every write. When a person edits
//! there, the follower sends the edit as a [`Request`] and the view holding
//! the writer lock admits it through the same functions its own forms call
//! ([`create_address`] and [`persist_intake`] for a new object,
//! [`save_metadata`] for a title or tags). Nothing here is a second write
//! path: the follower never stores, and the writer stores what it would have
//! stored had the person typed the edit there.
//!
//! Every request is answered once, by id. A request already answered replays
//! its answer and changes nothing, so a request delivered twice (a resend
//! after a handover, a duplicate broadcast) is admitted once. What is not in
//! the ledger, because the writer reloaded, is still safe to receive again:
//! an address already held is returned, and a title and tags already equal to
//! the request's are admitted as already applied.
use std::collections::{BTreeSet, VecDeque};

use muniment::Backend;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::GraphshellApp;
use crate::local_edit::{metadata, save_metadata};
use crate::local_intake::{create_address, persist_intake};

/// Answers the writer remembers, so a repeated request id is answered again
/// rather than applied again.
pub const REMEMBERED_REPLIES: usize = 256;
const MAX_FIELD: usize = 4096;
const MAX_TAGS: usize = 64;

/// What the follower last read: the session it observed and the projection
/// epoch of its store. The epoch advances each time a writer opens, so a
/// request made before a writer change or a new session is stale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Base {
    pub session: Uuid,
    pub epoch: u64,
}

/// The storing actions a follower queues. Everything else a writer's forms
/// do stays view-only in a follower.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    /// Add an object by address, as **Add an object** does.
    AddNode { address: String, title: String },
    /// Set an object's title and tags, as **Save changes** does. `was_*` are
    /// the values the follower read; the edit is refused if they have moved.
    EditNode {
        member: Uuid,
        title: String,
        tags: Vec<String>,
        was_title: String,
        was_tags: Vec<String>,
    },
}

/// One queued edit. `view` names the follower so replies can be told apart.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: String,
    pub view: String,
    pub base: Base,
    pub edit: Edit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Admitted,
    Refused,
}

/// The writer's answer to a request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub id: String,
    pub view: String,
    pub status: Status,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<Uuid>,
}

impl Request {
    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|error| format!("invalid edit request: {error}"))
    }
}

impl Reply {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a reply serializes")
    }

    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|error| format!("invalid edit reply: {error}"))
    }

    fn admitted(request: &Request, member: Option<Uuid>, note: &str) -> Self {
        Self {
            id: request.id.clone(),
            view: request.view.clone(),
            status: Status::Admitted,
            reason: note.into(),
            member,
        }
    }

    pub fn refused(request: &Request, reason: impl Into<String>) -> Self {
        Self {
            id: request.id.clone(),
            view: request.view.clone(),
            status: Status::Refused,
            reason: reason.into(),
            member: None,
        }
    }
}

/// The base a follower stamps on its requests.
pub fn base_of<B: Backend>(app: &GraphshellApp<B>) -> Base {
    Base {
        session: app.host.graph_session().id().0,
        epoch: app.host.projection_epoch().0,
    }
}

/// A node's tags in the order and form the stores compare them.
pub fn normalized(tags: &[String]) -> Vec<String> {
    tags.iter()
        .map(|tag| tag.trim().to_owned())
        .filter(|tag| !tag.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// What admitting a request will do, decided before any store is touched.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Answered before, or already true: the answer, with nothing to apply.
    Answered(Reply),
    /// Valid and current: apply it through the writer's funnel.
    Proceed,
}

/// The writer's record of answered request ids.
#[derive(Default)]
pub struct Admissions {
    replies: VecDeque<Reply>,
}

impl Admissions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `id` has been answered.
    pub fn answered(&self, id: &str) -> Option<&Reply> {
        self.replies.iter().find(|reply| reply.id == id)
    }

    /// Remember an answer. Returns it for the caller to send.
    pub fn record(&mut self, reply: Reply) -> Reply {
        if self.answered(&reply.id).is_none() {
            if self.replies.len() >= REMEMBERED_REPLIES {
                self.replies.pop_front();
            }
            self.replies.push_back(reply.clone());
        }
        reply
    }

    /// Decide `request` against the writer's graph. A refusal or an already
    /// true edit is returned as an answer and remembered; `Proceed` leaves
    /// the caller to apply it and [`record`](Self::record) the outcome.
    pub fn check<B: Backend>(&mut self, app: &GraphshellApp<B>, request: &Request) -> Verdict {
        if let Some(reply) = self.answered(&request.id) {
            return Verdict::Answered(reply.clone());
        }
        match judge(app, request) {
            Ok(None) => Verdict::Proceed,
            Ok(Some(reply)) | Err(reply) => Verdict::Answered(self.record(reply)),
        }
    }

    /// Check, apply and remember: the whole admission, for a writer that
    /// holds its app across the store write.
    pub async fn admit<B: pandect::graph_session::LegacyTransactionBackend>(
        &mut self,
        app: &mut GraphshellApp<B>,
        request: &Request,
        saved_at_secs: u64,
    ) -> Reply {
        if let Verdict::Answered(reply) = self.check(app, request) {
            return reply;
        }
        let reply = answer(request, apply(app, &request.edit, saved_at_secs).await);
        self.record(reply)
    }
}

/// The reply to a request once its edit has been applied or has failed.
pub fn answer(request: &Request, outcome: Result<Option<Uuid>, String>) -> Reply {
    match outcome {
        Ok(member) => Reply::admitted(request, member, ""),
        Err(error) => Reply::refused(
            request,
            format!("{error} · not stored; the editing view keeps it pending"),
        ),
    }
}

/// Apply an admitted edit through the funnel the writer's own forms use,
/// storing it before answering.
pub async fn apply<B: pandect::graph_session::LegacyTransactionBackend>(
    app: &mut GraphshellApp<B>,
    edit: &Edit,
    saved_at_secs: u64,
) -> Result<Option<Uuid>, String> {
    match edit {
        Edit::AddNode { address, title } => {
            let member = create_address(app, address, title)?;
            persist_intake(app, member, saved_at_secs).await?;
            Ok(Some(member))
        },
        Edit::EditNode {
            member,
            title,
            tags,
            ..
        } => {
            save_metadata(app, *member, title, &tags.join(","), saved_at_secs).await?;
            Ok(Some(*member))
        },
    }
}

fn invalid(request: &Request, reason: &str) -> Reply {
    Reply::refused(request, reason)
}

/// `Ok(None)`: proceed. `Ok(Some)`: already true. `Err`: refused.
fn judge<B: Backend>(
    app: &GraphshellApp<B>,
    request: &Request,
) -> Result<Option<Reply>, Reply> {
    if request.id.is_empty() || request.id.len() > 128 || request.view.is_empty() {
        return Err(invalid(request, "the request has no id or view"));
    }
    if app.host.is_observing() {
        return Err(invalid(request, "this view does not hold the writer"));
    }
    let current = base_of(app);
    if request.base.session != current.session {
        return Err(invalid(
            request,
            "stale: the graph was replaced since this view read it · reread and try again",
        ));
    }
    if request.base.epoch != current.epoch {
        return Err(invalid(
            request,
            "stale: the editing view changed since this view read it · reread and try again",
        ));
    }
    match &request.edit {
        Edit::AddNode { address, title } => {
            if address.trim().is_empty() {
                return Err(invalid(request, "Enter an address before adding an object"));
            }
            if address.len() > MAX_FIELD || title.len() > MAX_FIELD {
                return Err(invalid(request, "the address or title is too long"));
            }
            Ok(None)
        },
        Edit::EditNode {
            member,
            title,
            tags,
            was_title,
            was_tags,
        } => {
            if [title, was_title].iter().any(|text| text.len() > MAX_FIELD)
                || tags.len() > MAX_TAGS
                || tags.iter().chain(was_tags).any(|tag| tag.len() > MAX_FIELD)
            {
                return Err(invalid(request, "the title or tags are too long"));
            }
            if tags.iter().any(|tag| tag.contains(',')) {
                return Err(invalid(request, "a tag cannot contain a comma"));
            }
            let held = metadata(app, *member)
                .map_err(|_| invalid(request, "the object no longer exists"))?;
            let (wanted_title, wanted_tags) = (title.trim(), normalized(tags));
            if held.title == wanted_title && held.tags == wanted_tags {
                return Ok(Some(Reply::admitted(
                    request,
                    Some(*member),
                    "already applied",
                )));
            }
            if held.title != was_title.trim() || held.tags != normalized(was_tags) {
                return Err(invalid(
                    request,
                    "stale: the object changed since this view read it · reread and try again",
                ));
            }
            Ok(None)
        },
    }
}

#[cfg(test)]
mod tests;
