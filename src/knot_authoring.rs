// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A retained Knot authoring document over Graphshell.
//!
//! The background hub owns the one endpoint process and all carrier traffic.
//! Each visible document owns a local Cambium editor. Keystrokes, selection,
//! undo, IME, highlighting, outline, folds, and preview therefore stay on the
//! UI thread; only Open, Save, and revision refresh cross the carrier.

use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cambium::{
    AnyView, CompositionEvent, DomHandle, GenetAppRunner, GenetCtx, GenetElement, Key, KeyEvent,
    NamedKey, PointerClick, PointerEvent, PointerPhase, StyleRange, button, el, lens,
    styled_textarea,
};
use chirograph::{
    AdvertisedAction, CapabilityProfile, DerivedTextV1, EDITABLE_TEXT_SAVE_INTENT, EditableTextV1,
    InsertKnotClipV1, InsertKnotClipV2, IntentResult, KNOT_BLOCK_RUN_INTENT,
    KNOT_CLIP_INSERT_INTENT, KNOT_CLIP_INSERT_SCHEMA_V2, KNOT_TRANSCLUSION_RESOLVE_INTENT,
    KnotClipArtifactRoleV1, KnotClipArtifactV1, KnotClipFidelityV1, KnotClipObservedEdgeV1,
    KnotClipSelectorV1, KnotEffectV1, PresentationCapability, ProjectionSession, SaveTextV1,
};
use genet_scripted_dom::ScriptedDom;
use graphshell_client::{ResolvedContent, ResolvedPresentation, RetainedEndpointSession};
use inker::{
    ContentReport, DocumentSession, OutlineEntry, SessionButtonState, SessionClick, SessionEffect,
    SessionEngine, SessionError, SessionIme, SessionKey, SessionLink, SessionModifiers,
    SessionScrollKey, SessionSpawnRequest,
};
use knot_editor_host::KnotEditor;
use netrender::Scene;
use sceno::InstanceId;
use scenograph::{
    ProjectionInputBinding, PublicSourceRevision, RevisionEvidence, RuntimeSourceBinding,
    SourceBinding,
};

/// An in-process freshness witness for an editable Knot resource.
///
/// This wraps the endpoint-minted save token only while the retained session is
/// alive. It is deliberately neither serializable nor inspectable: files and
/// other sources without a public, durable revision can participate in a
/// runtime-verified Scenograph binding, but cannot turn that token into saved
/// provenance.
#[derive(Clone, Eq, PartialEq)]
pub struct KnotRuntimeWitness {
    base_token: Vec<u8>,
}

struct OpenedDocument {
    registration: u64,
    binding: DocumentBinding,
    events: Receiver<HubEvent>,
}

enum HubCommand {
    Open {
        registration: u64,
        address: String,
        events: Sender<HubEvent>,
        reply: SyncSender<Result<(u64, DocumentBinding), String>>,
    },
    Save {
        registration: u64,
        base_token: Vec<u8>,
        source: String,
    },
    InsertClip {
        expected_base: Option<Vec<u8>>,
        address: String,
        clip: PendingClip,
        status: Arc<Mutex<KnotClipStatus>>,
    },
    Effect {
        registration: u64,
        kind: KnotEffectKind,
        confirmed: bool,
    },
    Reload {
        registration: u64,
    },
    Unregister {
        registration: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KnotEffectKind {
    Resolve,
    Run,
}

#[derive(Clone)]
enum HubEvent {
    Remote(DocumentBinding),
    Intent(ProjectionIntentReceipt),
    Saved {
        source: String,
        binding: DocumentBinding,
    },
    Reloaded(DocumentBinding),
    Stale,
    Rejected(String),
    Revoked(String),
    /// The holder of a visited document is gone. Distinct from `Revoked`: the
    /// document still exists, this profile just cannot reach the mere that
    /// holds it, so the last scene stays and nothing is offered to save.
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectionIntentReceipt {
    target: InstanceId,
    intent: String,
    result: IntentResult,
}

struct Subscriber {
    address: String,
    events: Sender<HubEvent>,
}

pub(crate) struct KnotHub {
    directory: Option<PathBuf>,
    commands: HubSender,
    next_registration: AtomicU64,
}

#[derive(Clone)]
struct PendingClip {
    source_url: String,
    title: Option<String>,
    selector: Option<String>,
    knot_body: String,
    selectors: Vec<KnotClipSelectorV1>,
    artifacts: Vec<KnotClipArtifactV1>,
    fidelity: Vec<KnotClipFidelityV1>,
    discovered_edges: Vec<KnotClipObservedEdgeV1>,
}

/// Last known result of the configured Inspector clip destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnotClipStatus {
    Ready,
    Sending,
    Saved,
    Stale,
    Rejected(String),
}

/// UI-thread handle for the endpoint-owned clip action.
#[derive(Clone)]
pub struct KnotClipHandle {
    expected_base: Option<Vec<u8>>,
    hub: Arc<KnotHub>,
    target: String,
    status: Arc<Mutex<KnotClipStatus>>,
}

/// The effect grant a hosted endpoint carries, already parsed.
///
/// The spawned path passes these as CLI strings the endpoint binary reparses.
/// Hosting skips the round trip: the values are typed on both sides of a call
/// that no longer crosses a process.
struct HostedEffects {
    policy: knot::KnotEffectPolicy,
}

struct HostedEvidence {
    root: PathBuf,
    max_artifact_bytes: u64,
}

/// Which source truth a hosted endpoint serves.
///
/// The same two the spawned path names as mode strings, minus the stringly
/// typed hop: the process boundary was the only reason they were ever text.
enum HostedKnot {
    Directory { root: PathBuf },
}

/// Which mere holds what, for the addresses this engine is asked to open.
///
/// Shared with the shell rather than owned, because the engine is boxed into
/// the content registry when the window opens and a place is joined long
/// afterwards. A mutex rather than a cell because a registered content engine
/// must be `Send + Sync`, however single-threaded its use in fact is.
pub(crate) type KnotVisits = Arc<Mutex<KnotVisitRegistry>>;

#[derive(Default)]
pub(crate) struct KnotVisitRegistry {
    /// This profile's own Personae root while a place is open. A place-held
    /// address naming it is this mere's own document and opens locally.
    local_root: Option<[u8; 32]>,
    /// One hub per holder being visited, each owning one dialed session.
    holders: std::collections::HashMap<[u8; 32], Arc<KnotHub>>,
}

/// One configured endpoint shared by every open Knot document.
pub struct KnotAuthoringEngine {
    /// This profile's own vault, when it has one. `None` is a visit-only
    /// engine: a member with no vault of its own still has to be able to open
    /// a document someone else holds, so the engine registers either way and
    /// only the local half is missing.
    hub: Option<Arc<KnotHub>>,
    visits: KnotVisits,
    /// A separate read handle for an in-process source, when that source can
    /// safely grant one. Resident and spawned routes retain their boundary.
    publish_source: Option<crate::publish_service::PublishSource>,
    clip_target: Option<String>,
    auto_resolve: bool,
    auto_run: bool,
}

/// One open document on a hub, driven the way the surface drives it, with no
/// editor and no frame.
///
/// The render-free half of [`KnotDocumentSession`]: the same `Open` and
/// `Save` commands on the same channel, and the same event receiver. Tests
/// that care about who holds a document and whether it can be reached use
/// this; tests that care about typing and painting use the session.
#[cfg(test)]
pub(crate) struct HubProbe {
    hub: Arc<KnotHub>,
    opened: OpenedDocument,
}

/// The liveness watch a visiting hub keeps on its holder.
///
/// It exists because the hub thread cannot watch itself: the round trip it is
/// waiting on is a blocking carrier read with no deadline of its own, so the
/// thread that would notice the silence is the thread that is inside it. This
/// one holds nothing but clones of the subscribers' event senders and the
/// instant of the hub's last completed round trip.
///
/// It reports once and stops. The hub thread it left behind is still blocked
/// in the carrier, and stays there until the transport gives up; that costs one
/// parked thread and no correctness, because a surface already told its holder
/// is unavailable offers nothing further to save.
struct LivenessWatch {
    heard: Arc<Mutex<Instant>>,
    listeners: Arc<Mutex<Vec<Sender<HubEvent>>>>,
    stop: Arc<AtomicBool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthoringStatus {
    Current,
    Saving,
    Resolving,
    Running,
    Reloading,
    Stale,
    Rejected,
    Revoked,
    /// A visited document whose holder is gone. The scene is the last one the
    /// holder disclosed and no save is offered, because there is nobody to
    /// save to; a person keeps reading what they had.
    Unavailable,
}

/// One thing the toolbar asks the pane to do, once.
///
/// Queued rather than flagged: a flag per verb coalesced repeats, fixed the
/// order in the drain site rather than the user's, and had to be cleared
/// through `runner.update` — a full view rebuild per flag, just to reset a
/// bool.
#[derive(Clone, Debug, PartialEq, Eq)]
enum AuthoringRequest {
    OpenSavedCitation {
        address: String,
        expected_source: blake3::Hash,
    },
    Save,
    Resolve,
    Run,
    Reload,
}

struct AuthoringState {
    editor: KnotEditor,
    projection_target: InstanceId,
    last_intent: Option<ProjectionIntentReceipt>,
    status: AuthoringStatus,
    detail: String,
    /// Toolbar commands in the order they were asked for, drained after
    /// dispatch.
    requests: Vec<AuthoringRequest>,
    resolve_available: bool,
    run_available: bool,
    derived: Option<DerivedTextV1>,
    width: u32,
    height: u32,
}

type AuthoringView = Box<dyn AnyView<AuthoringState, (), GenetCtx, GenetElement>>;
type AuthoringRunner =
    GenetAppRunner<AuthoringState, fn(&AuthoringState) -> AuthoringView, AuthoringView, ()>;

/// The visible editor retained in Turnstone's ordinary content-session map.
pub struct KnotDocumentSession {
    clip_available: bool,
    hub: Arc<KnotHub>,
    registration: u64,
    address: String,
    /// Whether this document is held by another mere. Only observation and
    /// the unavailable path read it; editing does not care.
    visiting: bool,
    base_token: Vec<u8>,
    projection_input: Option<ProjectionInputBinding>,
    events: Receiver<HubEvent>,
    revision_refreshes: u64,
    dom: DomHandle,
    runner: AuthoringRunner,
    /// Kept across frames. Rebuilding a layout per paint re-cascaded and
    /// re-shaped the whole document to draw an unchanged screen; see
    /// [`crate::ui::RetainedLayout`] for the measurement.
    layout: crate::ui::RetainedLayout,
    text_drag: Option<genet_scripted_dom::NodeId>,
}

#[cfg(test)]
#[path = "knot_authoring/tests.rs"]
mod tests;

#[path = "knot_authoring/knot_projection_input_binding.rs"]
mod knot_projection_input_binding;
#[path = "knot_authoring/knot_runtime_witness_matches_save_token.rs"]
mod knot_runtime_witness_matches_save_token;
use knot_projection_input_binding::knot_projection_input_binding;
#[path = "knot_authoring/knot_runtime_source_binding.rs"]
mod knot_runtime_source_binding;
use knot_runtime_source_binding::knot_runtime_source_binding;
#[path = "knot_authoring/clip_knot_body.rs"]
mod clip_knot_body;
#[path = "knot_authoring/knot_clip_handle_same_admission.rs"]
mod knot_clip_handle_same_admission;
#[path = "knot_authoring/knot_clip_status_label.rs"]
mod knot_clip_status_label;
#[path = "knot_authoring/projection_intent_receipt_result_word.rs"]
mod projection_intent_receipt_result_word;
use clip_knot_body::clip_knot_body;
#[path = "knot_authoring/clip_selectors.rs"]
mod clip_selectors;
use clip_selectors::clip_selectors;
#[path = "knot_authoring/json_path.rs"]
mod json_path;
use json_path::json_path;
#[path = "knot_authoring/effect_mode.rs"]
mod effect_mode;
#[path = "knot_authoring/impl_session_engine_scene_for.rs"]
mod impl_session_engine_scene_for;
#[path = "knot_authoring/knot_authoring_engine_from_env.rs"]
mod knot_authoring_engine_from_env;
#[path = "knot_authoring/knot_hub_host.rs"]
mod knot_hub_host;
#[path = "knot_authoring/knot_visit_registry_set_local_root.rs"]
mod knot_visit_registry_set_local_root;
use effect_mode::effect_mode;
#[path = "knot_authoring/reject_nested_evidence_root.rs"]
mod reject_nested_evidence_root;
use reject_nested_evidence_root::reject_nested_evidence_root;
#[path = "knot_authoring/normalized_absolute.rs"]
mod normalized_absolute;
use normalized_absolute::normalized_absolute;
#[path = "knot_authoring/env_path.rs"]
mod env_path;
pub(crate) use env_path::env_path;
#[path = "knot_authoring/env_integer.rs"]
mod env_integer;
use env_integer::env_integer;
#[path = "knot_authoring/effect_mode_value.rs"]
mod effect_mode_value;
use effect_mode_value::effect_mode_value;
#[path = "knot_authoring/split_list.rs"]
mod split_list;
use split_list::split_list;
#[path = "knot_authoring/visit_holder.rs"]
mod visit_holder;
pub(crate) use visit_holder::visit_holder;
#[path = "knot_authoring/hub_probe_open.rs"]
#[cfg(test)]
mod hub_probe_open;
#[path = "knot_authoring/is_knot_address.rs"]
mod is_knot_address;
pub use is_knot_address::is_knot_address;
#[path = "knot_authoring/place_held_address.rs"]
mod place_held_address;
pub use place_held_address::place_held_address;
#[path = "knot_authoring/parse_place_held_address.rs"]
mod parse_place_held_address;
pub use parse_place_held_address::parse_place_held_address;
#[path = "knot_authoring/place_held_rewrite.rs"]
mod place_held_rewrite;
pub use place_held_rewrite::place_held_rewrite;
#[path = "knot_authoring/vault_relative.rs"]
mod vault_relative;
use vault_relative::vault_relative;
#[path = "knot_authoring/address_matches.rs"]
mod address_matches;
use address_matches::address_matches;
#[path = "knot_authoring/run_hub.rs"]
mod run_hub;
use run_hub::run_hub;
#[path = "knot_authoring/visit_liveness_budget.rs"]
mod visit_liveness_budget;
use visit_liveness_budget::visit_liveness_budget;
#[path = "knot_authoring/ensure_binding.rs"]
mod ensure_binding;
#[path = "knot_authoring/impl_drop_for_liveness_watch.rs"]
mod impl_drop_for_liveness_watch;
#[path = "knot_authoring/liveness_watch_start.rs"]
mod liveness_watch_start;
use ensure_binding::ensure_binding;
#[path = "knot_authoring/resolve_binding.rs"]
mod resolve_binding;
use resolve_binding::resolve_binding;
#[path = "knot_authoring/binding_from_presentation.rs"]
mod binding_from_presentation;
use binding_from_presentation::binding_from_presentation;
#[path = "knot_authoring/save_from_subscriber.rs"]
mod save_from_subscriber;
use save_from_subscriber::save_from_subscriber;
#[path = "knot_authoring/insert_clip.rs"]
mod insert_clip;
use insert_clip::insert_clip;
#[path = "knot_authoring/invoke_effect.rs"]
mod invoke_effect;
use invoke_effect::invoke_effect;
#[path = "knot_authoring/reload_subscriber.rs"]
mod reload_subscriber;
use reload_subscriber::reload_subscriber;
#[path = "knot_authoring/refresh_subscribers.rs"]
mod refresh_subscribers;
use refresh_subscribers::refresh_subscribers;
#[path = "knot_authoring/broadcast.rs"]
mod broadcast;
use broadcast::broadcast;
#[path = "knot_authoring/lost.rs"]
mod lost;
use lost::lost;
#[path = "knot_authoring/send_event.rs"]
mod send_event;
use send_event::send_event;
#[path = "knot_authoring/authoring_view.rs"]
mod authoring_view;
use authoring_view::authoring_view;
#[path = "knot_authoring/derived_status.rs"]
mod derived_status;
use derived_status::derived_status;
#[path = "knot_authoring/derived_status_at.rs"]
mod derived_status_at;
use derived_status_at::derived_status_at;
#[path = "knot_authoring/format_cache_age.rs"]
mod format_cache_age;
use format_cache_age::format_cache_age;
#[path = "knot_authoring/cambium_key.rs"]
mod cambium_key;
#[path = "knot_authoring/impl_drop_for_knot_document_session.rs"]
mod impl_drop_for_knot_document_session;
#[path = "knot_authoring/knot_document_session_invoke_effect.rs"]
mod knot_document_session_invoke_effect;
#[path = "knot_authoring/knot_document_session_pointer_event.rs"]
mod knot_document_session_pointer_event;
use cambium_key::cambium_key;
#[path = "knot_authoring/engine_id.rs"]
mod engine_id;
#[path = "knot_authoring/impl_document_session_scene_for.rs"]
mod impl_document_session_scene_for;
pub use engine_id::ENGINE_ID;
#[path = "knot_authoring/default_max_source_bytes.rs"]
mod default_max_source_bytes;
use default_max_source_bytes::DEFAULT_MAX_SOURCE_BYTES;
#[path = "knot_authoring/default_effect_max_depth.rs"]
mod default_effect_max_depth;
use default_effect_max_depth::DEFAULT_EFFECT_MAX_DEPTH;
#[path = "knot_authoring/default_effect_max_ops.rs"]
mod default_effect_max_ops;
use default_effect_max_ops::DEFAULT_EFFECT_MAX_OPS;
#[path = "knot_authoring/poll_interval.rs"]
mod poll_interval;
use poll_interval::POLL_INTERVAL;
#[path = "knot_authoring/visit_liveness_limit.rs"]
mod visit_liveness_limit;
pub(crate) use visit_liveness_limit::VISIT_LIVENESS_BUDGET;
#[path = "knot_authoring/liveness_tick.rs"]
mod liveness_tick;
use liveness_tick::LIVENESS_TICK;
#[path = "knot_authoring/knot_projection_authority.rs"]
mod knot_projection_authority;
use knot_projection_authority::KNOT_PROJECTION_AUTHORITY;
#[path = "knot_authoring/knot_projection_domain.rs"]
mod knot_projection_domain;
use knot_projection_domain::KNOT_PROJECTION_DOMAIN;
#[path = "knot_authoring/knot_sheet.rs"]
mod knot_sheet;
pub(crate) use knot_sheet::KNOT_SHEET;
#[path = "knot_authoring/wake.rs"]
mod wake;
use wake::Wake;
#[path = "knot_authoring/document_binding.rs"]
mod document_binding;
use document_binding::DocumentBinding;

#[path = "knot_authoring/lazy_hub.rs"]
mod lazy_hub;
use lazy_hub::HubSender;
