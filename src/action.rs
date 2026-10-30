// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The one vocabulary: everything that acts on turnstone lowers to an
//! [`Action`]; everything slow or platform-shaped leaves [`crate::app`] as an
//! [`Effect`] the shell runs through a port; services answer with an
//! [`Update`] drained on wake. Settings, automation, scenarios, scripting,
//! and remote control all speak this vocabulary, so no lane grows a second
//! execution model (the architecture plan's doctrine 2 — the meerkat
//! `command_drain` lesson).
//!
//! Two deliberate boundaries:
//!
//! * **The gesture law.** Ephemeral interaction may bypass Action: the
//!   canvas's semantic input methods (`pointer_down`, `cursor_moved`,
//!   `wheel`, ...) are already a typed vocabulary at the right granularity,
//!   and the shell maps raw input onto them directly. Durable or externally
//!   observable semantic change may not bypass — a gesture that ends in one
//!   surfaces a semantic event at gesture end. `Action` is the app-intent
//!   tier (navigate, reseed, flip a view mode), the tier automation and
//!   commands speak.
//! * **Port-agnostic messages.** This module never imports a service crate:
//!   [`Update`] carries app-owned types, and each port's adapter
//!   ([`crate::browse`] for the fetch actor) converts the service's concrete
//!   types at the boundary. The universal vocabulary must not depend on one
//!   port implementation.

mod command_metadata;
pub(crate) use command_metadata::command_metadata;

use crate::panes::{PaneKindId, PaneSource};
use mere::canvas::controls::CanvasCommand;

/// Manifest identities written by one explicit source-document capture.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredSourceDocument {
    pub raw_manifest: String,
    pub annotation_manifest: String,
}

/// A successfully fetched page document, in app-owned terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchedPage {
    /// The response's Content-Type header, verbatim.
    pub content_type: Option<String>,
    /// The response's Content-Disposition header, when the protocol has one.
    pub content_disposition: Option<String>,
    /// Exact response bytes, before replacement-character text decoding.
    pub bytes: Vec<u8>,
    /// The decoded body text.
    pub body: String,
}

/// Host-written facts for one completed download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredDownload {
    pub content: mere::kernel::graph::ContentHash,
    pub destination: String,
    pub byte_size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SmolwebSubmissionReceipt {
    Success(FetchedPage),
    Redirect(String),
}

/// One recall hit from browsing memory, in app-owned terms. The trail port
/// converts its lexical/vector projection at the boundary, like the bin does
/// with `DeletedNode`.
#[derive(Clone, Debug, PartialEq)]
pub struct RecallHit {
    pub url: String,
    pub title: Option<String>,
    /// When the traversal this hit indexes happened, unix milliseconds.
    pub at_ms: u64,
}

/// A staged (deleted) node's record in the recycle bin, in app-owned terms
/// (the port adapter converts eidetic's `DeletedNode` at the boundary).
/// Carries the ORIGINAL member id, so recovery restores identity.
#[derive(Clone, Debug, PartialEq)]
pub struct RemovedRecord {
    pub node_id: uuid::Uuid,
    pub url: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    /// Deletion time, unix milliseconds (the bin's newest-first ordering).
    pub deleted_at_ms: u64,
    /// The world the node BORE at deletion (`Node.nested`, string form) —
    /// its file sits in the archive slot while this record stands; recovery
    /// re-bears it, purging the record purges the file.
    pub nested: Option<String>,
    /// The node's facet bundle at deletion (facet-id -> payload), restored
    /// whole on recovery: residency binding, arrangement, web state.
    pub facets: Option<serde_json::Value>,
}

#[path = "action/impl_std_fmt_debug_for_sensitive.rs"]
mod impl_std_fmt_debug_for_sensitive;
#[path = "action/palette_actions.rs"]
mod palette_actions;
#[path = "action/sensitive_string_new.rs"]
mod sensitive_string_new;
pub use palette_actions::palette_actions;
#[path = "action/action_canvas_verb.rs"]
mod action_canvas_verb;
#[path = "action/shared_command_id.rs"]
mod shared_command_id;
pub use shared_command_id::shared_command_id;
#[path = "action/shared_label.rs"]
mod shared_label;
use shared_label::shared_label;
#[path = "action/shared.rs"]
mod shared;
use shared::shared;
#[path = "action/fetched_page_text.rs"]
mod fetched_page_text;
#[path = "action/impl_std_fmt_debug_for_micron.rs"]
mod impl_std_fmt_debug_for_micron;
#[path = "action/sensitive_string.rs"]
mod sensitive_string;
pub use sensitive_string::SensitiveString;
#[path = "action/space_ref.rs"]
mod space_ref;
pub use space_ref::SpaceRef;
#[path = "action/document_find_direction.rs"]
mod document_find_direction;
pub use document_find_direction::DocumentFindDirection;
#[path = "action/document_find_match.rs"]
mod document_find_match;
pub use document_find_match::DocumentFindMatch;
#[path = "action/document_find_model.rs"]
mod document_find_model;
pub use document_find_model::DocumentFindModel;
#[path = "action/place_artifact_kind.rs"]
mod place_artifact_kind;
pub use place_artifact_kind::PlaceArtifactKind;
#[path = "action/action.rs"]
mod action;
pub use action::Action;
#[path = "action/wb_axis.rs"]
mod wb_axis;
pub use wb_axis::WbAxis;
#[path = "action/caret_move.rs"]
mod caret_move;
pub use caret_move::CaretMove;
#[path = "action/content_control.rs"]
mod content_control;
pub use content_control::ContentControl;
#[path = "action/effect.rs"]
mod effect;
pub use effect::Effect;
#[path = "action/micron_submission.rs"]
mod micron_submission;
pub use micron_submission::MicronSubmission;
#[path = "action/update.rs"]
mod update;
pub use update::Update;
