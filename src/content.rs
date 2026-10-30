// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The content lane's app truth (rung 4, born ahead of its port): per-node
//! document lifecycle only. Charter per the architecture plan's module map:
//! engine registrations, per-node document lifecycle, the verso-tile flip,
//! content frames, and input routing — where the registry itself is
//! Inker's, never a hand-wired lane ladder.
//!
//! What lives HERE is the lifecycle state machine and nothing else. Live
//! document sessions are retained, non-`Send` handles, so the shell's
//! content port owns them (ports are the only owners of handles; `App`
//! holds data) keyed by the same node ids. The shell registers Livery, reader,
//! and native smolweb session engines at composition time; a `Requested` node
//! never silently spins when routing or spawning fails.

use std::collections::HashMap;

use uuid::Uuid;

/// Host observations about engine construction, separate from a live page's
/// document capabilities. Registration does not imply a page is running.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineAvailability {
    Registered,
    OnDemand { detail: String },
    Unavailable { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineFamily {
    Document,
    Surface,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineDescriptor {
    pub id: String,
    pub label: String,
    pub family: EngineFamily,
    pub availability: EngineAvailability,
}

/// Typed extraction lineage projected on a node rendered through `genet.reader`.
pub const READER_LINEAGE_FACET: &str = "web.reader-lineage";

/// One node's content lifecycle. Absent from the map = no content activity
/// (the at-rest state for every node).
#[derive(Clone, Debug, PartialEq)]
pub enum NodeContent {
    /// A spawn effect is in flight through the content port.
    Requested,
    /// A live session exists shell-side; frames compose into the node.
    Live,
    /// The actor reached a protocol input response and the host is waiting for
    /// a human answer. There is no live document handle yet.
    AwaitingInput,
    /// A Gemini capsule asked for a client certificate and the host is waiting
    /// for explicit approval to derive and present one.
    AwaitingIdentity,
    /// A Gemini server certificate changed and the host is waiting for an
    /// explicit trust decision before replacing the durable pin.
    AwaitingTrust,
    /// The last spawn failed; the reason is surfaced, never swallowed.
    Failed(String),
}

/// App-owned facts about a live session, mirrored out of the content port at
/// spawn (the adapter converts the service's report type at the boundary, so
/// this module stays port-agnostic). Data, not a handle: the Inspector pane
/// and the observation snapshot read these without reaching into the shell.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentFacts {
    /// The engine id the route decision picked (e.g. `genet.livery`).
    pub engine: String,
    /// Which lane of the node's Pelt content presents it. A surface takes
    /// Back, Forward, Reload and Stop on its engine's own web plane; a
    /// document's belong to the App's load and graph history.
    pub lane: ContentLane,
    /// The structural read, when the lane has one. `None` is reported
    /// honestly (a lane without introspection, not an empty document).
    pub structure: Option<StructureFacts>,
    /// Fleece derivation when the live representation is a reader rendering.
    pub lineage: Option<ExtractionLineageFacts>,
    /// Document controls reported by the owning retained or hosted engine.
    /// Reasons and partial-support details remain intact across the port
    /// boundary so product surfaces never infer support from a method's
    /// existence.
    pub capabilities: DocumentCapabilityFacts,
}

/// The lane of a node's live content (Pelt's `PeltLane`, mirrored so the App
/// stays port-agnostic).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentLane {
    /// A retained document session.
    Document,
    /// A live web-engine surface (Weld, Scry or Servo).
    Surface,
}

/// App-owned, port-agnostic mirror of one document control's availability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityStatus {
    Supported,
    Unsupported { reason: String },
    Partial { detail: String },
}

/// The document controls Turnstone can present uniformly across engine kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentCapabilityFacts {
    pub find_in_page: CapabilityStatus,
    pub page_zoom: CapabilityStatus,
    pub page_capture: CapabilityStatus,
    pub navigation: CapabilityStatus,
}

/// What a live engine reports it is ACTUALLY presenting a document at, after
/// its own clamping and quantization. App-owned and port-agnostic, mirrored
/// out of the content port the same way the capability facts are.
///
/// This is runtime-only truth and never reaches the sidecar: the persisted
/// value is the node's REQUEST, and an engine's effective level belongs to the
/// live session that computed it. Lanes without a read-back (every hosted
/// surface today) simply have no entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageZoomFacts {
    /// The factor the host asked for, echoed back unchanged.
    pub requested: f32,
    /// The factor the engine is presenting at.
    pub applied: f32,
    /// The engine's own bounds, so a surface can say why a step was refused.
    pub min: f32,
    pub max: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractionLineageFacts {
    pub tool: String,
    pub version: String,
    pub selector: String,
    pub score: Option<i32>,
    pub block_count: usize,
}

/// One page body fetched by the shell actor and ready to hand to a document
/// engine: exact acquired bytes, the decoded body, the response address and
/// the admission time. A live session never issues a second network request
/// for bytes the host already owns. The type is Mere's `page-load` document.
pub type FetchedDocument = page_load::LoadedDocument;

/// Network progress, which remains app truth even after the first prefix has
/// become a live document session. The type is Mere's `page-load` phase.
pub type PageFetchPhase = page_load::LoadPhase;

/// The structural read, mirrored in app-owned terms (the report type itself
/// stays port-side; the app holds what its surfaces present — the Inspector's
/// counts and the a11y projection's outline).
#[derive(Clone, Debug, PartialEq)]
pub struct StructureFacts {
    /// The document's own `<title>`.
    pub title: Option<String>,
    pub headings: usize,
    pub links: usize,
    /// The element outline (painted elements, document order): the a11y
    /// projection's document subtree is built from exactly this.
    pub outline: Vec<OutlineFact>,
}

/// One outline element: nesting depth, a coarse semantic role, and the
/// element's accessible name.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineFact {
    pub depth: usize,
    pub role: &'static str,
    pub name: String,
}

/// The app-truth side of the content lane: node id -> lifecycle, plus the
/// spawn-time facts for live nodes.
#[derive(Debug)]
pub(crate) struct SurfaceTraversal {
    pub from: String,
    pub target: String,
    pub back: bool,
    pub request: Option<inker::webcue::RequestKey>,
}

#[derive(Debug, Default)]
pub struct ContentStates {
    states: HashMap<Uuid, NodeContent>,
    facts: HashMap<Uuid, ContentFacts>,
    cue_reports: HashMap<Uuid, inker::webcue::CapabilityReport>,
    surface_traversals: HashMap<Uuid, SurfaceTraversal>,
    /// The effective page zoom a live engine reported back, per node. Purely
    /// transient: it dies with the session, and the save path never sees it.
    page_zoom: HashMap<Uuid, PageZoomFacts>,
    /// Each node's load: its current request, transfer phase, streamed bytes
    /// and retained response, owned by Mere's sans-IO `PageLoad`.
    loads: HashMap<Uuid, page_load::PageLoad>,
    /// A retained response admission, independent of equal response bytes.
    source_admissions: HashMap<Uuid, (Uuid, crate::source_capture::SourceIdentity)>,
}

#[cfg(test)]
#[path = "content/tests.rs"]
mod tests;

#[path = "content/engine_availability_is_selectable.rs"]
mod engine_availability_is_selectable;
#[path = "content/engine_label.rs"]
mod engine_label;
pub use engine_label::engine_label;
#[path = "content/capability_status_is_available.rs"]
mod capability_status_is_available;
#[path = "content/content_states_get.rs"]
mod content_states_get;
#[path = "content/impl_default_for_document_capability_facts.rs"]
mod impl_default_for_document_capability_facts;
