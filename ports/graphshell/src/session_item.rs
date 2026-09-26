// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The session item a resident's mere route shows in its graph projection
//! (reservoir plan §7 item 31): the attached session itself, carrying the
//! intents that act on the session rather than on one node. An attached
//! application applies edits, undoes, redoes and changes its views through it.
//! Each is accepted by id at any revision of the graph (§7 item 32), though
//! not across an epoch, which names another session. Graphshell's own view
//! never shows it.
//!
//! The route's other projection, the mere's sessions, speaks the vocabulary
//! here too: the lifecycle intents, which name their session by id (§7 item
//! 33). djinn serves it and every application reads it from here (§7 item
//! 37).

use chirograph::{AdvertisedAction, CardValueV1, IntentEffect, IntentReference, PortableCardV1};
use mere::kernel::graph::CapturedDelta;
use muniment::Backend;
use pandect::{GraphSession, SessionId, ViewIntent};
use serde::{Deserialize, Serialize};

/// A mere route's projection of its sessions: the mere's card, then one card
/// per session, oldest first.
pub const MERE_SESSIONS: &str = "djinn.mere/v1/sessions";
/// A mere route's projection of the attached session's graph.
pub const MERE_GRAPH: &str = "djinn.mere/v1/graph";
/// The scene source kind of the sessions projection's cards.
pub const SESSIONS_SOURCE: &str = "mere.sessions";
/// Mint a new, empty session.
pub const MINT_SESSION_INTENT: &str = "mere.sessions.mint";
/// Attach this connection to another session.
pub const ATTACH_SESSION_INTENT: &str = "mere.sessions.attach";
/// Fork a session at its live cursor.
pub const FORK_SESSION_INTENT: &str = "mere.sessions.fork";
/// Put a session in the trash.
pub const TRASH_SESSION_INTENT: &str = "mere.sessions.trash";
/// Take a session back out of the trash.
pub const RESTORE_SESSION_INTENT: &str = "mere.sessions.restore";
/// Schema of [`SessionsActionV1`].
pub const SESSIONS_SCHEMA: &str = "mere.sessions/v1";

/// The scene source kind of the session item.
pub const SESSION_ITEM_SOURCE: &str = "mere.session";

/// Apply edits in stable-id form as one change.
pub const APPLY_EDITS_INTENT: &str = "mere.session.apply";
/// Schema of [`ApplyEditsV1`].
pub const APPLY_EDITS_SCHEMA: &str = "mere.session.apply/v1";
/// Undo the application's own latest change still in effect.
pub const UNDO_INTENT: &str = "mere.session.undo";
/// Redo the application's most recent undo.
pub const REDO_INTENT: &str = "mere.session.redo";
/// Schema of [`StepV1`], the undo and redo payload.
pub const STEP_SCHEMA: &str = "mere.session.step/v1";
/// Change one of the application's views.
pub const SET_VIEW_INTENT: &str = "mere.session.view";
/// Schema of [`SetViewV1`].
pub const SET_VIEW_SCHEMA: &str = "mere.session.view/v1";

/// Payload of [`APPLY_EDITS_INTENT`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyEditsV1 {
    pub schema: String,
    pub edits: Vec<CapturedDelta>,
}

impl ApplyEditsV1 {
    pub fn new(edits: Vec<CapturedDelta>) -> Self {
        Self {
            schema: APPLY_EDITS_SCHEMA.to_string(),
            edits,
        }
    }
}

/// Payload of [`UNDO_INTENT`] and [`REDO_INTENT`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepV1 {
    pub schema: String,
}

impl Default for StepV1 {
    fn default() -> Self {
        Self {
            schema: STEP_SCHEMA.to_string(),
        }
    }
}

/// Payload of [`SET_VIEW_INTENT`]: one of the application's views, by name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetViewV1 {
    pub schema: String,
    pub view: String,
    pub state: ViewIntent,
}

impl SetViewV1 {
    pub fn new(view: impl Into<String>, state: ViewIntent) -> Self {
        Self {
            schema: SET_VIEW_SCHEMA.to_string(),
            view: view.into(),
            state,
        }
    }
}

/// Payload of the sessions projection's intents. A step on a session names
/// it by id, so it lands however the list has moved since the application
/// looked (§7 item 33); minting names none, and only minting reads the name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionsActionV1 {
    pub schema: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

impl SessionsActionV1 {
    /// A step on session `id`: attach, fork, trash or restore.
    pub fn on(id: SessionId) -> Self {
        Self {
            schema: SESSIONS_SCHEMA.to_string(),
            session: Some(id),
            display_name: None,
        }
    }

    /// Mint a session, named or not.
    pub fn mint(display_name: Option<String>) -> Self {
        Self {
            schema: SESSIONS_SCHEMA.to_string(),
            session: None,
            display_name,
        }
    }
}

fn action(
    intent: &str,
    label: &str,
    explanation: &str,
    schema: &str,
    effect: IntentEffect,
) -> AdvertisedAction {
    AdvertisedAction {
        intent: IntentReference(intent.to_string()),
        label: label.to_string(),
        explanation: explanation.to_string(),
        payload_schema: schema.to_string(),
        input_form: None,
        effect,
    }
}

/// What the session item offers.
pub fn actions() -> Vec<AdvertisedAction> {
    vec![
        action(
            APPLY_EDITS_INTENT,
            "Apply edits",
            "Apply edits in stable-id form to this session as one change, under your name.",
            APPLY_EDITS_SCHEMA,
            IntentEffect::DomainTruth,
        ),
        action(
            UNDO_INTENT,
            "Undo",
            "Undo your latest change still in effect; what others changed since stays.",
            STEP_SCHEMA,
            IntentEffect::DomainTruth,
        ),
        action(
            REDO_INTENT,
            "Redo",
            "Redo your most recent undo.",
            STEP_SCHEMA,
            IntentEffect::DomainTruth,
        ),
        action(
            SET_VIEW_INTENT,
            "Change a view",
            "Keep one of your views of this session: its folds, camera and layout.",
            SET_VIEW_SCHEMA,
            IntentEffect::Curation,
        ),
    ]
}

/// The session item's card.
pub fn card<B: Backend>(session: &GraphSession<B>) -> PortableCardV1 {
    let manifest = session.manifest();
    PortableCardV1 {
        title: manifest
            .display_name
            .clone()
            .unwrap_or_else(|| "Session".to_string()),
        values: vec![
            CardValueV1 {
                label: "Session".to_string(),
                value: session.id().as_uuid().to_string(),
            },
            CardValueV1 {
                label: "Changes".to_string(),
                value: session.changes().len().to_string(),
            },
        ],
        badges: Vec::new(),
        media: Vec::new(),
    }
}
