// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The observation surface: one snapshot + one event stream over app truth
//! (the architecture plan's recorded snapshot/events pair, landed at its
//! trigger — the scenario lane is the first automation consumer, and its
//! asserts read THIS surface instead of poking app fields one by one). The
//! same surface is what the a11y projection, diagnostics, and the
//! session-engines plan's automation story consume later: observation is
//! the vocabulary's other half, so it lives beside `action`, app-owned and
//! port-agnostic.
//!
//! Scope note: events are emitted where Actions and Updates fold — the
//! semantic tier. Continuous gestures bypass Action by the gesture law, so
//! a gesture-end semantic change (click-selection, drag-placement) does not
//! yet emit; that arrives with the gesture-end events the law already
//! promises, not by teaching this module about pointers.

use uuid::Uuid;

use crate::app::App;
use crate::content::NodeContent;
use crate::ui::Suggestion;

/// The place receipt: what this profile is, which place it is in, and one
/// digest each over the two shared projections. Two converged peers differ in
/// `personae_root` and agree on every id and digest.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct PlaceFacts {
    pub personae_root: String,
    pub moot: String,
    pub root: String,
    pub chat: String,
    pub default_channel: String,
    /// This bind's own dialable tickets at the last refresh.
    pub local_rendezvous: Vec<String>,
    /// Effective Gemot membership at the last refresh.
    pub members: usize,
    pub graph_nodes: usize,
    pub chat_messages: usize,
    /// Group-key frames not yet applicable, and refused since this open.
    pub group_frames_pending: usize,
    pub group_frames_refused: usize,
    /// Records held for an epoch not yet installed, admitted once it arrived,
    /// and dropped for good at the parking bound.
    pub parked_records: u64,
    pub readmitted_records: u64,
    pub evicted_records: u64,
    pub graph_digest: String,
    pub chat_digest: String,
    pub lanes: Vec<PlaceLaneFacts>,
}

/// One lane's local observation. Not peer reachability or delivery.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct PlaceLaneFacts {
    pub name: String,
    pub syncing: bool,
    pub sync_rounds: u64,
    pub ops_received: u64,
}

/// One Knot document this session has open, and where it is held.
///
/// The comparison two windows are made on: the same `address` and the same
/// `derived_digest` on both sides is the whole claim that a place-held
/// document is one document and not two copies.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct KnotDocumentFacts {
    pub address: String,
    /// The holder's Personae root, for a place-held address. `None` for a
    /// document this profile holds under its own local address.
    pub holder: Option<String>,
    /// `local`, `visiting`, `unavailable`, or `refused`.
    pub status: String,
    /// Blake3 hex over the derived source text the surface currently holds.
    /// Absent for a refusal: there is no surface, so there is no text.
    pub derived_digest: Option<String>,
    /// Exact source/selection witness for an explicit retained-content action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_selection: Option<crate::producer_retention::KnotSelection>,
}

#[cfg(test)]
#[path = "observe/tests.rs"]
mod tests;

#[path = "observe/app_event_describe.rs"]
mod app_event_describe;
#[path = "observe/place_facts.rs"]
mod place_facts;
pub use place_facts::place_facts;
#[path = "observe/place_record.rs"]
mod place_record;
pub use place_record::place_record;
#[path = "observe/snapshot.rs"]
mod snapshot;
pub use snapshot::snapshot;
#[path = "observe/workbench_cells.rs"]
mod workbench_cells;
pub use workbench_cells::workbench_cells;
#[path = "observe/suggestion_line.rs"]
mod suggestion_line;
pub use suggestion_line::suggestion_line;
#[path = "observe/snapshot_2.rs"]
mod snapshot_2;
pub use snapshot_2::Snapshot;
#[path = "observe/source_capture_view.rs"]
mod source_capture_view;
pub use source_capture_view::SourceCaptureView;
#[path = "observe/focused_node.rs"]
mod focused_node;
pub use focused_node::FocusedNode;
#[path = "observe/omnibar_view.rs"]
mod omnibar_view;
pub use omnibar_view::OmnibarView;
#[path = "observe/document_find_view.rs"]
mod document_find_view;
pub use document_find_view::DocumentFindView;
#[path = "observe/document_capabilities_view.rs"]
mod document_capabilities_view;
pub use document_capabilities_view::DocumentCapabilitiesView;
#[path = "observe/user_agent_decision_view.rs"]
mod user_agent_decision_view;
pub use user_agent_decision_view::UserAgentDecisionView;
#[path = "observe/app_event.rs"]
mod app_event;
pub use app_event::AppEvent;
