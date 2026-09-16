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
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, Key, KeyEvent, PointerClick,
    StyleRange, button, el, lens, styled_textarea,
};
use genet_scripted_dom::ScriptedDom;
use graphshell::client::{ResolvedContent, ResolvedPresentation, RetainedEndpointSession};
use graphshell::projection_editor::{
    ProjectionInputBinding, PublicSourceRevision, RevisionEvidence, RuntimeSourceBinding,
    SourceBinding,
};
use graphshell::protocol::{
    AdvertisedAction, CapabilityProfile, DerivedTextV1, EDITABLE_TEXT_SAVE_INTENT, EditableTextV1,
    InsertKnotClipV1, InsertKnotClipV2, IntentResult, KNOT_BLOCK_RUN_INTENT,
    KNOT_CLIP_INSERT_INTENT, KNOT_CLIP_INSERT_SCHEMA_V2, KNOT_TRANSCLUSION_RESOLVE_INTENT,
    KnotClipArtifactRoleV1, KnotClipArtifactV1, KnotClipFidelityV1, KnotClipObservedEdgeV1,
    KnotClipSelectorV1, KnotEffectV1, PresentationCapability, ProjectionSession, SaveTextV1,
};
use inker::{
    ContentReport, DocumentSession, OutlineEntry, SessionClick, SessionEngine, SessionError,
    SessionLink, SessionScrollKey, SessionSpawnRequest,
};
use knot_editor_host::KnotEditor;
use netrender::Scene;
use sceno::InstanceId;

/// The app-side route id. Knot remains the authority; this names only the
/// retained Turnstone presentation.
pub const ENGINE_ID: &str = "knot.authoring";

const DEFAULT_MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
const DEFAULT_EFFECT_MAX_DEPTH: u8 = 1;
const DEFAULT_EFFECT_MAX_OPS: u64 = 100_000;
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// How long a VISITED document may go without an answer from its holder before
/// the surface is told the holder is out of reach.
///
/// A holder that closes says so, and the carrier read fails at once. A holder
/// whose process dies says nothing: the carrier read stays blocked until QUIC
/// abandons the connection on its own, which is the transport's idle timeout
/// (tens of seconds) and not a bound this product chose. This is that bound,
/// measured from the hub's last completed round trip.
pub(crate) const VISIT_LIVENESS_BUDGET: Duration = Duration::from_secs(6);
/// How often the liveness watch checks the budget. It is a separate thread
/// because the one it watches is blocked inside the carrier read.
const LIVENESS_TICK: Duration = Duration::from_millis(250);
const KNOT_PROJECTION_AUTHORITY: &str = "knot";
const KNOT_PROJECTION_DOMAIN: &str = "knot.document.v1";

pub(crate) const KNOT_SHEET: &str = "\
    .knot-root { background-color: rgb(22, 27, 40); color: rgb(205, 212, 226); } \
    .knot-toolbar { display: flex; background-color: rgb(18, 22, 33); \
                    padding: 6px 10px; } \
    .knot-title { color: rgb(205, 212, 226); font-size: 12px; width: 35%; \
                  white-space: nowrap; overflow: hidden; } \
    .knot-status { color: rgb(140, 153, 176); font-size: 12px; width: 20%; \
                   white-space: nowrap; } \
    .knot-save { color: rgb(232, 150, 40); background-color: rgb(28, 34, 50); \
                 border: 1px solid rgb(52, 62, 86); padding: 3px 10px; } \
    .knot-effect { color: rgb(103, 184, 235); background-color: rgb(28, 34, 50); \
                   border: 1px solid rgb(52, 62, 86); padding: 3px 10px; } \
    .knot-body { display: flex; } \
    .knot-editor-wrap { width: 70%; padding: 10px; } \
    .knot-editor-wrap textarea { color: rgb(218, 224, 236); \
                                 background-color: rgb(25, 30, 44); \
                                 font-size: 13px; white-space: pre-wrap; \
                                 padding: 10px; border: 1px solid rgb(52, 62, 86); } \
    .knot-readout { width: 30%; padding: 10px; color: rgb(172, 183, 204); \
                    font-size: 12px; white-space: pre-wrap; overflow: hidden; } \
    .syntax-heading { color: rgb(240, 179, 94); } \
    .syntax-link { color: rgb(103, 184, 235); } \
    .syntax-strong { color: rgb(235, 239, 247); } \
    .syntax-emphasis { color: rgb(194, 205, 224); } \
    .syntax-codeblock, .syntax-verbatim { color: rgb(141, 207, 160); }";

type Wake = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
struct DocumentBinding {
    target: InstanceId,
    save_action: AdvertisedAction,
    clip_action: AdvertisedAction,
    resolve_action: Option<AdvertisedAction>,
    run_action: Option<AdvertisedAction>,
    editable: EditableTextV1,
}

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

impl KnotRuntimeWitness {
    /// Check that the endpoint still recognizes the observed save token.
    ///
    /// The caller receives only the comparison result, never the token.
    pub fn matches_save_token(&self, candidate: &[u8]) -> bool {
        self.base_token == candidate
    }
}

fn knot_projection_input_binding(editable: &EditableTextV1) -> ProjectionInputBinding {
    ProjectionInputBinding {
        source: SourceBinding {
            authority: KNOT_PROJECTION_AUTHORITY.to_owned(),
            domain: KNOT_PROJECTION_DOMAIN.to_owned(),
            resource: editable.address.clone(),
        },
        expects_generation: editable
            .public_revision
            .as_ref()
            .map(|revision| PublicSourceRevision::new(knot::hex32(revision))),
        revision_evidence: if editable.public_revision.is_some() {
            RevisionEvidence::PublicGeneration
        } else {
            RevisionEvidence::RuntimeVerified
        },
    }
}

fn knot_runtime_source_binding(
    source: SourceBinding,
    base_token: Vec<u8>,
) -> RuntimeSourceBinding<KnotRuntimeWitness> {
    RuntimeSourceBinding::new(source, KnotRuntimeWitness { base_token })
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

impl ProjectionIntentReceipt {
    fn result_word(&self) -> &'static str {
        match &self.result {
            IntentResult::Accepted => "accepted",
            IntentResult::Rejected { .. } => "rejected",
            IntentResult::Stale { .. } => "stale",
        }
    }

    fn line(&self) -> String {
        format!(
            "Projection intent {}: {} {}",
            self.target.0,
            self.intent,
            self.result_word()
        )
    }
}

struct Subscriber {
    address: String,
    events: Sender<HubEvent>,
}

pub(crate) struct KnotHub {
    commands: Sender<HubCommand>,
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

impl KnotClipStatus {
    pub fn label(&self) -> String {
        match self {
            Self::Ready => "ready".into(),
            Self::Sending => "clipping".into(),
            Self::Saved => "clip saved".into(),
            Self::Stale => "clip target changed; retry".into(),
            Self::Rejected(reason) => reason.clone(),
        }
    }
}

/// UI-thread handle for the endpoint-owned clip action.
#[derive(Clone)]
pub struct KnotClipHandle {
    hub: Arc<KnotHub>,
    target: String,
    status: Arc<Mutex<KnotClipStatus>>,
}

impl KnotClipHandle {
    pub fn insert(&self, clip: inker::DocumentClip) -> Result<(), String> {
        let artifacts = clip
            .artifacts
            .into_iter()
            .map(|artifact| KnotClipArtifactV1 {
                role: match artifact.role {
                    inker::DocumentClipArtifactRole::SourceResponse => {
                        KnotClipArtifactRoleV1::SourceResponse
                    }
                    inker::DocumentClipArtifactRole::ObservedRepresentation => {
                        KnotClipArtifactRoleV1::ObservedRepresentation
                    }
                },
                media_type: artifact.media_type,
                canonical_uri: artifact.canonical_uri,
                bytes: artifact.bytes,
            })
            .collect::<Vec<_>>();
        let selectors = clip_selectors(clip.selector.as_deref(), &clip.text, &artifacts);
        let had_dom_range = clip.selector.as_deref().is_some_and(|selector| {
            serde_json::from_str::<serde_json::Value>(selector)
                .ok()
                .and_then(|value| {
                    value
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
                .as_deref()
                == Some("dom-range")
        });
        let retained_dom_range = selectors
            .iter()
            .any(|selector| matches!(selector, KnotClipSelectorV1::DomRange { .. }));
        let mut fidelity = (!artifacts.is_empty())
            .then_some(KnotClipFidelityV1 {
                class: "arrangement-unchecked".into(),
                detail: "The semantic lowering did not compare computed layout.".into(),
                selector: None,
            })
            .into_iter()
            .collect::<Vec<_>>();
        if had_dom_range && !retained_dom_range {
            fidelity.push(KnotClipFidelityV1 {
                class: if selectors.is_empty() {
                    "selector-unanchored".into()
                } else {
                    "selector-demoted".into()
                },
                detail: if selectors.is_empty() {
                    "The DOM-range selector had no retained observed representation or exact source quote."
                        .into()
                } else {
                    "The DOM-range selector had no retained observed representation; a text quote into the source response is retained instead."
                        .into()
                },
                selector: selectors.first().cloned(),
            });
        }
        let discovered_edges = clip
            .links
            .iter()
            .cloned()
            .map(|target| KnotClipObservedEdgeV1 {
                target,
                relation: "link".into(),
            })
            .collect();
        let fragment = mere_import::web_clip::fragment_from_text(
            clip.source_url.clone(),
            clip.title.clone(),
            clip.text,
            clip.selector.clone(),
            clip.links,
        );
        let pending = PendingClip {
            source_url: clip.source_url,
            title: clip.title,
            selector: clip.selector,
            knot_body: mere_import::web_clip::fragment_to_knot_body(&fragment),
            selectors,
            artifacts,
            fidelity,
            discovered_edges,
        };
        *self.status.lock().map_err(|_| "clip status poisoned")? = KnotClipStatus::Sending;
        self.hub
            .commands
            .send(HubCommand::InsertClip {
                address: self.target.clone(),
                clip: pending,
                status: self.status.clone(),
            })
            .map_err(|_| "Knot authoring worker is unavailable".to_string())
    }

    pub fn status(&self) -> KnotClipStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_else(|_| KnotClipStatus::Rejected("clip status unavailable".into()))
    }

    pub fn target(&self) -> &str {
        &self.target
    }
}

fn clip_selectors(
    selector: Option<&str>,
    text: &str,
    artifacts: &[KnotClipArtifactV1],
) -> Vec<KnotClipSelectorV1> {
    let has_observed_representation = artifacts
        .iter()
        .any(|artifact| artifact.role == KnotClipArtifactRoleV1::ObservedRepresentation);
    let dom_range = has_observed_representation
        .then_some(selector)
        .flatten()
        .and_then(|selector| serde_json::from_str::<serde_json::Value>(selector).ok())
        .and_then(|selector| {
            if selector.get("type")?.as_str()? != "dom-range" {
                return None;
            }
            Some(KnotClipSelectorV1::DomRange {
                artifact_role: KnotClipArtifactRoleV1::ObservedRepresentation,
                anchor_path: json_path(selector.get("anchor")?.get("path")?)?,
                anchor_offset: selector.get("anchor")?.get("offset")?.as_u64()?,
                focus_path: json_path(selector.get("focus")?.get("path")?)?,
                focus_offset: selector.get("focus")?.get("offset")?.as_u64()?,
                quote: selector
                    .get("quote")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(text)
                    .to_string(),
            })
        });
    dom_range
        .or_else(|| {
            let artifact_role = artifacts.iter().find_map(|artifact| {
                std::str::from_utf8(&artifact.bytes)
                    .ok()
                    .filter(|source| !text.is_empty() && source.contains(text))
                    .map(|_| artifact.role)
            })?;
            Some(KnotClipSelectorV1::TextQuote {
                artifact_role,
                exact: text.to_string(),
                prefix: None,
                suffix: None,
            })
        })
        .into_iter()
        .collect()
}

fn json_path(value: &serde_json::Value) -> Option<Vec<u32>> {
    value
        .as_array()?
        .iter()
        .map(|component| {
            component
                .as_u64()
                .and_then(|component| u32::try_from(component).ok())
        })
        .collect()
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

impl KnotHub {
    /// Host the Knot endpoint in this process, over the in-memory carrier.
    ///
    /// The default path. A spawned endpoint cannot exist in a browser tab or
    /// an iOS app, so hosting is what lets the editor reach the targets the
    /// product is aimed at; `connect` remains for the case where an endpoint
    /// really is a separate program.
    ///
    /// Everything past construction is identical, because the carrier seam is
    /// where the two differ and `run_hub` never sees it.
    fn host(
        source: HostedKnot,
        effects: Option<HostedEffects>,
        evidence: Option<HostedEvidence>,
        max_source_bytes: u64,
        wake: Wake,
    ) -> Result<(Arc<Self>, Option<knot::KnotPublishSource>), String> {
        let (commands, receiver) = mpsc::channel();
        let (ready_send, ready_receive) =
            mpsc::sync_channel::<Result<Option<knot::KnotPublishSource>, String>>(1);
        std::thread::Builder::new()
            .name("turnstone-knot-authoring".into())
            .spawn(move || {
                let profile = CapabilityProfile::new([
                    PresentationCapability::EditableText,
                    PresentationCapability::PortableCard,
                ]);
                let grant = knot::KnotWriteGrant::new(max_source_bytes);
                let opened = match source {
                    HostedKnot::Directory { root } => {
                        knot::KnotEndpoint::open_writable(&root, grant)
                            .map_err(|error| format!("could not open the Knot directory: {error}"))
                            .map(|endpoint| (endpoint, None::<knot::KnotPublishSource>))
                    }
                };
                let retained = opened
                    .and_then(|(mut endpoint, publish_source)| {
                        if let Some(hosted) = effects {
                            endpoint.grant_effects(knot::KnotEffectAuthority::new(hosted.policy));
                        }
                        if let Some(hosted) = evidence {
                            endpoint.grant_clip_evidence(knot::BlobClipEvidenceStore::open(
                                hosted.root,
                                hosted.max_artifact_bytes,
                            )?);
                        }
                        Ok((endpoint, publish_source))
                    })
                    .and_then(|(endpoint, publish_source)| {
                        // Resume is the endpoint's own answer; the carrier has
                        // no business inventing one.
                        let carrier = graphshell_local::LocalCarrier::new(
                            endpoint,
                            |endpoint: &mut knot::KnotEndpoint, request| {
                                graphshell_endpoint::ResumableProjectionSource::resume(
                                    endpoint, request,
                                )
                                .map_err(|error| error.to_string())
                            },
                        );
                        RetainedEndpointSession::over(Box::new(carrier), profile)
                            .map(|retained| (retained, publish_source))
                    });
                match retained {
                    Ok((retained, publish_source)) => {
                        let _ = ready_send.send(Ok(publish_source));
                        run_hub(retained, receiver, wake, false);
                    }
                    Err(error) => {
                        let _ = ready_send.send(Err(error));
                    }
                }
            })
            .map_err(|error| format!("could not start Knot authoring worker: {error}"))?;
        let publish_source = ready_receive
            .recv()
            .map_err(|_| "Knot authoring worker stopped during startup".to_string())??;
        Ok((
            Arc::new(Self {
                commands,
                next_registration: AtomicU64::new(1),
            }),
            publish_source,
        ))
    }

    /// Visit a document another mere holds, over an admitted place session.
    ///
    /// The same shape as [`Self::host`], with the carrier at the far end of a
    /// dialed stream instead of an in-process endpoint, and the same
    /// [`run_hub`] behind it: the document surface edits a visited document
    /// with no knowledge that it is visiting.
    ///
    /// The dial runs ON this thread, not before it. The retained session and
    /// its carrier never cross a thread boundary, so the thread that opens
    /// the stream is the thread that reads it until the visit ends.
    fn visit(dial: crate::place::lanes::HolderDial, wake: Wake) -> Result<Arc<Self>, String> {
        let (commands, receiver) = mpsc::channel();
        let (ready_send, ready_receive) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("turnstone-knot-visiting".into())
            .spawn(move || {
                let profile = CapabilityProfile::new([
                    PresentationCapability::EditableText,
                    PresentationCapability::PortableCard,
                ]);
                let retained = dial.open().and_then(|(carrier, holder)| {
                    tracing::info!(holder = %crate::place::hex32(&holder.to_bytes()), "visiting a place-held document");
                    RetainedEndpointSession::over(Box::new(carrier), profile)
                });
                match retained {
                    Ok(retained) => {
                        let _ = ready_send.send(Ok(()));
                        run_hub(retained, receiver, wake, true);
                    },
                    Err(error) => {
                        let _ = ready_send.send(Err(error));
                    },
                }
            })
            .map_err(|error| format!("could not start the Knot visiting worker: {error}"))?;
        ready_receive
            .recv()
            .map_err(|_| "the Knot visiting worker stopped while dialing".to_string())??;
        Ok(Arc::new(Self {
            commands,
            next_registration: AtomicU64::new(1),
        }))
    }

    fn connect(program: PathBuf, args: Vec<OsString>, wake: Wake) -> Result<Arc<Self>, String> {
        let (commands, receiver) = mpsc::channel();
        let (ready_send, ready_receive) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("turnstone-knot-authoring".into())
            .spawn(move || {
                let profile = CapabilityProfile::new([
                    PresentationCapability::EditableText,
                    PresentationCapability::PortableCard,
                ]);
                let retained = graphshell::sessions::spawn_endpoint_session(
                    program.as_os_str(),
                    &args,
                    profile,
                );
                match retained {
                    Ok(retained) => {
                        let _ = ready_send.send(Ok(()));
                        run_hub(retained, receiver, wake, false);
                    }
                    Err(error) => {
                        let _ = ready_send.send(Err(error));
                    }
                }
            })
            .map_err(|error| format!("could not start Knot authoring worker: {error}"))?;
        ready_receive
            .recv()
            .map_err(|_| "Knot authoring worker stopped during startup".to_string())??;
        Ok(Arc::new(Self {
            commands,
            next_registration: AtomicU64::new(1),
        }))
    }

    /// Connect Turnstone to the resident Knot route. The resident process is
    /// the only owner of persona files, evidence custody, and p2p sync.
    fn resident(wake: Wake) -> Result<Arc<Self>, String> {
        let (commands, receiver) = mpsc::channel();
        let (ready_send, ready_receive) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("turnstone-knot-authoring".into())
            .spawn(move || {
                let profile = CapabilityProfile::new([
                    PresentationCapability::EditableText,
                    PresentationCapability::PortableCard,
                ]);
                let retained = (|| {
                    let route = graphshell::native::app_admission::AppRouteId::new("knot")
                        .map_err(|error| format!("invalid resident Knot route: {error}"))?;
                    let carrier = graphshell::native::app_client::AppRouteCarrier::open(
                        graphshell::native::app_admission::AppId::new("turnstone"),
                        route,
                    )
                    .map_err(|error| format!("could not open resident Knot route: {error}"))?;
                    RetainedEndpointSession::over(Box::new(carrier), profile)
                        .map_err(|error| error.to_string())
                })();
                match retained {
                    Ok(retained) => {
                        let _ = ready_send.send(Ok(()));
                        run_hub(retained, receiver, wake, false);
                    }
                    Err(error) => {
                        let _ = ready_send.send(Err(error));
                    }
                }
            })
            .map_err(|error| format!("could not start Knot authoring worker: {error}"))?;
        ready_receive
            .recv()
            .map_err(|_| "Knot authoring worker stopped during startup".to_string())??;
        Ok(Arc::new(Self {
            commands,
            next_registration: AtomicU64::new(1),
        }))
    }

    fn open(&self, address: &str) -> Result<OpenedDocument, String> {
        let registration = self.next_registration.fetch_add(1, Ordering::Relaxed);
        let (events_send, events) = mpsc::channel();
        let (reply_send, reply) = mpsc::sync_channel(1);
        self.commands
            .send(HubCommand::Open {
                registration,
                address: address.to_string(),
                events: events_send,
                reply: reply_send,
            })
            .map_err(|_| "Knot authoring worker is unavailable".to_string())?;
        let (confirmed_registration, binding) = reply
            .recv()
            .map_err(|_| "Knot authoring worker stopped while opening".to_string())??;
        debug_assert_eq!(registration, confirmed_registration);
        Ok(OpenedDocument {
            registration,
            binding,
            events,
        })
    }
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

impl KnotVisitRegistry {
    pub(crate) fn set_local_root(&mut self, root: Option<[u8; 32]>) {
        self.local_root = root;
    }

    pub(crate) fn is_local(&self, root: &[u8; 32]) -> bool {
        self.local_root.as_ref() == Some(root)
    }

    pub(crate) fn holder(&self, root: &[u8; 32]) -> Option<Arc<KnotHub>> {
        self.holders.get(root).cloned()
    }

    pub(crate) fn insert_holder(&mut self, root: [u8; 32], hub: Arc<KnotHub>) {
        self.holders.insert(root, hub);
    }

    /// Let go of every visit. Called when the place closes: a visiting thread
    /// drives its carrier on the lane runtime, so it must stop before that
    /// runtime does.
    pub(crate) fn clear(&mut self) {
        self.holders.clear();
        self.local_root = None;
    }
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
    publish_source: Option<knot::KnotPublishSource>,
    clip_target: Option<String>,
    auto_resolve: bool,
    auto_run: bool,
}

impl KnotAuthoringEngine {
    pub fn from_env(wake: Wake, visits: KnotVisits) -> Result<Option<Self>, String> {
        let mode =
            std::env::var("TURNSTONE_KNOT_MODE").unwrap_or_else(|_| "directory-write".into());
        if !matches!(mode.as_str(), "directory-write" | "persona-vault") {
            return Err(format!(
                "unsupported TURNSTONE_KNOT_MODE {mode}; expected directory-write or persona-vault"
            ));
        }
        let max_source_bytes = std::env::var("TURNSTONE_KNOT_MAX_BYTES")
            .ok()
            .map(|value| {
                value.parse::<u64>().map_err(|error| {
                    format!("TURNSTONE_KNOT_MAX_BYTES must be an integer: {error}")
                })
            })
            .transpose()?
            .unwrap_or(DEFAULT_MAX_SOURCE_BYTES);
        let evidence_root = env_path("TURNSTONE_KNOT_EVIDENCE_ROOT");
        let max_evidence_bytes = env_integer(
            "TURNSTONE_KNOT_EVIDENCE_MAX_BYTES",
            DEFAULT_MAX_SOURCE_BYTES,
        )?;
        let resolve_mode = effect_mode("TURNSTONE_KNOT_RESOLVE_MODE")?;
        let run_mode = effect_mode("TURNSTONE_KNOT_RUN_MODE")?;
        let schemes = std::env::var("TURNSTONE_KNOT_RESOLVE_SCHEMES").unwrap_or_else(|_| {
            if resolve_mode == "never" {
                String::new()
            } else {
                "file".into()
            }
        });
        let languages = std::env::var("TURNSTONE_KNOT_RUN_LANGUAGES").unwrap_or_else(|_| {
            if run_mode == "never" {
                String::new()
            } else {
                "rhai".into()
            }
        });
        let max_depth = env_integer("TURNSTONE_KNOT_RESOLVE_MAX_DEPTH", DEFAULT_EFFECT_MAX_DEPTH)?;
        let max_ops = env_integer("TURNSTONE_KNOT_RUN_MAX_OPS", DEFAULT_EFFECT_MAX_OPS)?;
        let effects_enabled = resolve_mode != "never" || run_mode != "never";
        let clip_target = std::env::var("TURNSTONE_KNOT_CLIP_TARGET")
            .ok()
            .filter(|target| !target.trim().is_empty());

        if mode == "persona-vault" {
            if env_path("TURNSTONE_KNOT_ENDPOINT").is_some() {
                return Err(
                    "persona-vault mode uses Graphshell's resident Knot route; TURNSTONE_KNOT_ENDPOINT is only valid for isolated directory fixtures"
                        .into(),
                );
            }
            if std::env::var_os("TURNSTONE_KNOT_PERSONA").is_some() {
                return Err(
                    "Graphshell owner settings select the resident Knot persona; remove TURNSTONE_KNOT_PERSONA from Turnstone"
                        .into(),
                );
            }
            if evidence_root.is_some() {
                return Err(
                    "the resident owns Knot evidence custody; TURNSTONE_KNOT_EVIDENCE_ROOT is only valid for directory mode"
                        .into(),
                );
            }
            if effects_enabled {
                return Err(
                    "persona-vault effects require a resident grant; Turnstone cannot grant effects across the app route"
                        .into(),
                );
            }
            return Ok(Some(Self {
                hub: Some(KnotHub::resident(wake)?),
                visits,
                publish_source: None,
                clip_target,
                auto_resolve: false,
                auto_run: false,
            }));
        }

        let Some(root) = env_path("TURNSTONE_KNOT_ROOT") else {
            // No vault of this profile's own. The engine still registers, so a
            // place-held `knot://<holder>/<path>` address routes to the hub the
            // shell dialed for that holder instead of falling off the end of
            // the content registry into the external-protocol route. Only a
            // LOCAL address has nowhere to go, and that is refused by name.
            return Ok(Some(Self::visit_only(visits, clip_target)));
        };

        // Host in-process where the endpoint's constructor is a plain
        // directory open. A spawned endpoint cannot exist in a browser tab or
        // an iOS app, so hosting is the path that reaches the targets this
        // product is aimed at, and spawning is the exception rather than the
        // rule.
        //
        // `TURNSTONE_KNOT_ENDPOINT` is an explicit opt-out, for the case where
        // the endpoint genuinely is a separate program. Everything else hosts.
        let explicit_program = env_path("TURNSTONE_KNOT_ENDPOINT").is_some();
        if !explicit_program {
            let effects = effects_enabled.then(|| HostedEffects {
                policy: knot::KnotEffectPolicy {
                    resolve: effect_mode_value(&resolve_mode),
                    run: effect_mode_value(&run_mode),
                    allowed_schemes: split_list(&schemes),
                    allowed_languages: split_list(&languages),
                    max_depth: max_depth as u8,
                    max_ops,
                },
            });
            let evidence = evidence_root.clone().map(|root| HostedEvidence {
                root,
                max_artifact_bytes: max_evidence_bytes,
            });
            reject_nested_evidence_root(&root, evidence_root.as_deref())?;
            let (hub, publish_source) = match KnotHub::host(
                HostedKnot::Directory { root: root.clone() },
                effects,
                evidence,
                max_source_bytes,
                wake,
            ) {
                Ok(hosted) => hosted,
                // A configured vault that will not open is a misconfiguration
                // worth saying out loud, but it is not a reason to strand the
                // place-held addresses this process can still reach. Say it,
                // then carry on as a visitor.
                Err(error) => {
                    tracing::warn!(%error, root = %root.display(), "the configured Knot vault could not be opened; visiting only");
                    return Ok(Some(Self::visit_only(visits, clip_target)));
                },
            };
            return Ok(Some(Self {
                hub: Some(hub),
                visits,
                publish_source,
                clip_target,
                auto_resolve: resolve_mode == "auto",
                auto_run: run_mode == "auto",
            }));
        }

        reject_nested_evidence_root(&root, evidence_root.as_deref())?;
        let endpoint = env_path("TURNSTONE_KNOT_ENDPOINT")
            .ok_or_else(|| {
                "TURNSTONE_KNOT_ENDPOINT disappeared while opening directory mode".to_string()
            })?;
        let endpoint_mode = match (effects_enabled, evidence_root.is_some()) {
            (false, false) => "directory-write",
            (false, true) => "directory-write-evidence",
            (true, false) => "directory-write-effects",
            (true, true) => "directory-write-effects-evidence",
        };
        let mut args = vec![
            endpoint_mode.into(),
            root.into_os_string(),
            max_source_bytes.to_string().into(),
        ];
        if effects_enabled {
            args.extend([
                resolve_mode.clone().into(),
                run_mode.clone().into(),
                schemes.clone().into(),
                languages.clone().into(),
                max_depth.to_string().into(),
                max_ops.to_string().into(),
            ]);
        }
        if let Some(evidence_root) = &evidence_root {
            args.extend([
                evidence_root.clone().into_os_string(),
                max_evidence_bytes.to_string().into(),
            ]);
        }
        let hub = KnotHub::connect(endpoint, args, wake)?;
        Ok(Some(Self {
            hub: Some(hub),
            visits,
            publish_source: None,
            clip_target,
            auto_resolve: resolve_mode == "auto",
            auto_run: run_mode == "auto",
        }))
    }

    /// An engine with no vault of its own: visits only.
    fn visit_only(visits: KnotVisits, clip_target: Option<String>) -> Self {
        Self {
            hub: None,
            visits,
            publish_source: None,
            clip_target,
            auto_resolve: false,
            auto_run: false,
        }
    }

    /// The refusal a local Knot address earns on a visit-only engine. One
    /// sentence, naming the missing thing rather than the failed route.
    fn no_local_vault(address: &str) -> String {
        format!("no Knot vault is configured, so {address} cannot be opened here")
    }

    pub fn clip_handle(&self) -> Option<KnotClipHandle> {
        // A clip lands in the local vault. Without one there is nothing to
        // clip into, whatever `TURNSTONE_KNOT_CLIP_TARGET` names.
        let hub = self.hub.clone()?;
        self.clip_target.as_ref().map(|target| KnotClipHandle {
            hub,
            target: target.clone(),
            status: Arc::new(Mutex::new(KnotClipStatus::Ready)),
        })
    }

    /// Gives the shell the independent read authority needed to host shares.
    /// Directory and spawned-endpoint modes have none: their file or process
    /// boundary cannot safely be widened by this UI.
    pub fn take_publish_source(&mut self) -> Option<knot::KnotPublishSource> {
        self.publish_source.take()
    }

    #[cfg(test)]
    fn connect_directory(
        program: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        max_source_bytes: u64,
    ) -> Result<Self, String> {
        Ok(Self {
            visits: KnotVisits::default(),
            hub: Some(KnotHub::connect(
                program.into(),
                vec![
                    "directory-write".into(),
                    root.into().into_os_string(),
                    max_source_bytes.to_string().into(),
                ],
                Arc::new(|| {}),
            )?),
            publish_source: None,
            clip_target: None,
            auto_resolve: false,
            auto_run: false,
        })
    }

    #[cfg(test)]
    fn connect_directory_evidence(
        program: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        max_source_bytes: u64,
        evidence_root: impl Into<PathBuf>,
        max_evidence_bytes: u64,
    ) -> Result<Self, String> {
        Ok(Self {
            visits: KnotVisits::default(),
            hub: Some(KnotHub::connect(
                program.into(),
                vec![
                    "directory-write-evidence".into(),
                    root.into().into_os_string(),
                    max_source_bytes.to_string().into(),
                    evidence_root.into().into_os_string(),
                    max_evidence_bytes.to_string().into(),
                ],
                Arc::new(|| {}),
            )?),
            publish_source: None,
            clip_target: None,
            auto_resolve: false,
            auto_run: false,
        })
    }

    #[cfg(test)]
    fn connect_directory_effects(
        program: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        max_source_bytes: u64,
        resolve: &str,
        run: &str,
    ) -> Result<Self, String> {
        Ok(Self {
            visits: KnotVisits::default(),
            hub: Some(KnotHub::connect(
                program.into(),
                vec![
                    "directory-write-effects".into(),
                    root.into().into_os_string(),
                    max_source_bytes.to_string().into(),
                    resolve.into(),
                    run.into(),
                    "file".into(),
                    "rhai".into(),
                    "1".into(),
                    "10000".into(),
                ],
                Arc::new(|| {}),
            )?),
            publish_source: None,
            clip_target: None,
            auto_resolve: resolve == "auto",
            auto_run: run == "auto",
        })
    }

    #[cfg(test)]
    fn connect_communal_fixture_effects(
        program: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        max_source_bytes: u64,
        run: &str,
    ) -> Result<Self, String> {
        Ok(Self {
            visits: KnotVisits::default(),
            hub: Some(KnotHub::connect(
                program.into(),
                vec![
                    "communal-fixture-effects".into(),
                    root.into().into_os_string(),
                    max_source_bytes.to_string().into(),
                    "never".into(),
                    run.into(),
                    "".into(),
                    "rhai".into(),
                    "1".into(),
                    "10000".into(),
                ],
                Arc::new(|| {}),
            )?),
            publish_source: None,
            clip_target: None,
            auto_resolve: false,
            auto_run: run == "auto",
        })
    }
}

impl SessionEngine<Scene> for KnotAuthoringEngine {
    fn engine_id(&self) -> &str {
        ENGINE_ID
    }

    fn spawn(
        &self,
        request: &SessionSpawnRequest,
    ) -> Result<Box<dyn DocumentSession<Scene>>, SessionError> {
        if !is_knot_address(&request.address) {
            return Err(SessionError::Unsupported(format!(
                "{} is not a Knot document",
                request.address
            )));
        }
        // A place-held address decides whose endpoint answers it. This
        // profile's own root is the local vault, exactly as before; anyone
        // else's is the hub the shell dialed for that holder, and no hub
        // means the visit was never opened rather than an empty document.
        let (hub, visiting) = match parse_place_held_address(&request.address) {
            Some((root, _)) => {
                let visits = self.visits.lock().map_err(|_| {
                    SessionError::SpawnFailed("the Knot visit registry is poisoned".into())
                })?;
                if visits.is_local(&root) {
                    let hub = self.hub.clone().ok_or_else(|| {
                        SessionError::SpawnFailed(Self::no_local_vault(&request.address))
                    })?;
                    (hub, false)
                } else {
                    let hub = visits.holder(&root).ok_or_else(|| {
                        SessionError::SpawnFailed(format!(
                            "no visit is open to the mere holding {}",
                            request.address
                        ))
                    })?;
                    (hub, true)
                }
            },
            // A plain `file://` or `knot://vault/` address is this profile's
            // own document, and without a vault there is no such thing.
            None => {
                let hub = self.hub.clone().ok_or_else(|| {
                    SessionError::SpawnFailed(Self::no_local_vault(&request.address))
                })?;
                (hub, false)
            },
        };
        let opened = hub.open(&request.address).map_err(SessionError::SpawnFailed)?;
        Ok(Box::new(KnotDocumentSession::new(
            hub,
            opened,
            request.viewport,
            self.auto_resolve,
            self.auto_run,
            visiting,
        )))
    }
}

fn effect_mode(name: &str) -> Result<String, String> {
    let value = std::env::var(name).unwrap_or_else(|_| "never".into());
    match value.as_str() {
        "auto" | "ask" | "never" => Ok(value),
        _ => Err(format!("{name} must be auto, ask, or never")),
    }
}

fn reject_nested_evidence_root(
    source: &PathBuf,
    evidence: Option<&std::path::Path>,
) -> Result<(), String> {
    let Some(evidence) = evidence else {
        return Ok(());
    };
    let source = normalized_absolute(source)?;
    let evidence = normalized_absolute(evidence)?;
    if evidence.starts_with(&source) {
        return Err(format!(
            "TURNSTONE_KNOT_EVIDENCE_ROOT must be outside the served Knot directory {}",
            source.display()
        ));
    }
    Ok(())
}

fn normalized_absolute(path: &std::path::Path) -> Result<PathBuf, String> {
    let absolute = std::path::absolute(path)
        .map_err(|error| format!("could not resolve {}: {error}", path.display()))?;
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
}

/// A path setting, with an empty or blank value read as unset.
///
/// A child process inherits every name its parent exported, including the ones
/// the parent left blank, and `var_os` answers `Some("")` for those. Treating
/// that as a path sends the vault open at the current directory and fails, so a
/// profile that was never given a vault is told it has none.
pub(crate) fn env_path(name: &str) -> Option<PathBuf> {
    let value = std::env::var_os(name)?;
    if value.to_string_lossy().trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

fn env_integer<T>(name: &str, default: T) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    std::env::var(name)
        .ok()
        .map(|value| {
            value
                .parse()
                .map_err(|error| format!("{name} must be an integer: {error}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

/// The already-validated mode string as Knot's own enum.
///
/// `effect_mode` rejected anything else on the way in, so an unknown value
/// here would be a bug upstream rather than user input; the safe reading is
/// the one that grants nothing.
fn effect_mode_value(mode: &str) -> knot::KnotEffectMode {
    match mode {
        "auto" => knot::KnotEffectMode::Auto,
        "ask" => knot::KnotEffectMode::Ask,
        _ => knot::KnotEffectMode::Never,
    }
}

/// Split a comma-separated setting, dropping blanks so a trailing comma or an
/// empty setting yields no entry rather than an empty-string allowance.
fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

/// Open one visiting hub over a prepared dial.
///
/// The shell's entry point to [`KnotHub::visit`]: the hub itself stays
/// private, and what the shell holds is a handle the engine can be given.
pub(crate) fn visit_holder(
    dial: crate::place::lanes::HolderDial,
    wake: Wake,
) -> Result<Arc<KnotHub>, String> {
    KnotHub::visit(dial, wake)
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

#[cfg(test)]
impl HubProbe {
    pub(crate) fn open(hub: Arc<KnotHub>, address: &str) -> Result<Self, String> {
        let opened = hub.open(address)?;
        Ok(Self { hub, opened })
    }

    /// The source the endpoint last disclosed for this document.
    pub(crate) fn source(&self) -> &str {
        &self.opened.binding.editable.source
    }

    /// The address the endpoint discloses, which for a visited document is
    /// the holder's own and not the one that was asked for.
    pub(crate) fn disclosed_address(&self) -> &str {
        &self.opened.binding.editable.address
    }

    /// Author one revision through the hub's ordinary save path.
    pub(crate) fn save(&mut self, source: &str) -> Result<String, String> {
        self.hub
            .commands
            .send(HubCommand::Save {
                registration: self.opened.registration,
                base_token: self.opened.binding.editable.base_token.clone(),
                source: source.to_string(),
            })
            .map_err(|_| "the hub is gone".to_string())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match self.opened.events.recv_timeout(left) {
                Ok(HubEvent::Saved { source, binding }) => {
                    self.opened.binding = binding;
                    return Ok(source);
                },
                Ok(HubEvent::Stale) => return Err("stale".into()),
                Ok(HubEvent::Rejected(reason) | HubEvent::Revoked(reason)) => return Err(reason),
                Ok(HubEvent::Unavailable(reason)) => {
                    return Err(format!("unavailable: {reason}"));
                },
                Ok(_) => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
    }

    /// Wait for the hub to report that the holder is gone.
    pub(crate) fn wait_for_unavailable(&self, budget: Duration) -> Option<String> {
        let deadline = std::time::Instant::now() + budget;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return None;
            }
            match self.opened.events.recv_timeout(left) {
                Ok(HubEvent::Unavailable(reason)) => return Some(reason),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    }
}

pub fn is_knot_address(address: &str) -> bool {
    address.split(['?', '#']).next().is_some_and(|base| {
        let base = base.to_ascii_lowercase();
        base.ends_with(".djot") || base.ends_with(".knot")
    }) || address.to_ascii_lowercase().starts_with("knot:")
}

/// A place-held document's address: `knot://<holder root hex>/<path>`.
///
/// The host slot is what makes the form decidable without a lookup. A local
/// vault discloses `knot://vault/<name>` and directory mode discloses
/// `file:///<absolute path>`; both name a document only this profile can
/// reach, so neither can be shared as-is. Sixty-four hex characters in the
/// host say WHO holds it, and the rest is the path inside the vault that
/// holder serves. `is_knot_address` matches it already, so the ordinary open
/// route carries it without a second scheme.
pub fn place_held_address(holder_root: &[u8; 32], path: &str) -> String {
    format!(
        "knot://{}/{}",
        crate::place::hex32(holder_root),
        path.trim_start_matches('/')
    )
}

/// Split a place-held address into its holder and the path it names.
///
/// `None` for every other Knot address, including `knot://vault/...`: a host
/// that is not 64 hex characters is not a Personae root and must not be
/// guessed at.
pub fn parse_place_held_address(address: &str) -> Option<([u8; 32], String)> {
    let rest = address
        .get(..7)
        .filter(|scheme| scheme.eq_ignore_ascii_case("knot://"))
        .and_then(|_| address.get(7..))?;
    let (host, path) = rest.split_once('/')?;
    if host.len() != 64 || !host.bytes().all(|byte| byte.is_ascii_hexdigit()) || path.is_empty() {
        return None;
    }
    let mut root = [0u8; 32];
    for (slot, pair) in root.iter_mut().zip(host.as_bytes().chunks(2)) {
        *slot = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some((root, path.to_string()))
}

/// Rewrite a locally held Knot address into the place-held form, or `None`
/// when this address names no document inside `vault_root`.
///
/// Called where the sharer's Personae root and its vault root are both known,
/// which is the place worker. An address already in the place-held form is
/// left alone: re-sharing someone else's document must not re-attribute it.
pub fn place_held_rewrite(
    address: &str,
    vault_root: &std::path::Path,
    holder_root: &[u8; 32],
) -> Option<String> {
    if parse_place_held_address(address).is_some() {
        return None;
    }
    if let Some(name) = address
        .get(..13)
        .filter(|prefix| prefix.eq_ignore_ascii_case("knot://vault/"))
        .and(address.get(13..))
        .map(|rest| rest.trim_start_matches('/'))
        .filter(|rest| !rest.is_empty())
    {
        return Some(place_held_address(holder_root, name));
    }
    let path = url::Url::parse(address).ok()?.to_file_path().ok()?;
    let relative = vault_relative(&path, vault_root)?;
    Some(place_held_address(holder_root, &relative))
}

/// The document's path inside the vault, with `/` separators.
///
/// Both the plain root and its canonical form are tried: the disclosed
/// address comes from a directory walk and the configured root comes from the
/// environment, and on Windows those differ by a verbatim prefix often enough
/// that one comparison is not enough.
fn vault_relative(path: &std::path::Path, vault_root: &std::path::Path) -> Option<String> {
    let canonical_path = std::fs::canonicalize(path);
    let canonical_root = std::fs::canonicalize(vault_root);
    let candidates = [
        (Some(path), Some(vault_root)),
        (canonical_path.as_deref().ok(), canonical_root.as_deref().ok()),
        (canonical_path.as_deref().ok(), Some(vault_root)),
        (Some(path), canonical_root.as_deref().ok()),
    ];
    candidates.into_iter().find_map(|(path, root)| {
        let relative = path?.strip_prefix(root?).ok()?;
        let relative = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        (!relative.is_empty()).then_some(relative)
    })
}

/// Whether a presentation the endpoint disclosed answers the requested
/// address.
///
/// A place-held address is the one case where the two strings differ on
/// purpose: the holder discloses its own local address, which a visitor has
/// no way to know and no business knowing. The path inside the vault is the
/// part both ends agree on, so that is what is compared. Which hub the
/// request reached has already decided whose vault this is.
fn address_matches(requested: &str, disclosed: &str) -> bool {
    if requested == disclosed {
        return true;
    }
    let Some((_, path)) = parse_place_held_address(requested) else {
        return false;
    };
    let tail = format!("/{path}");
    disclosed.len() > tail.len() && disclosed.ends_with(&tail)
}

/// Drive one endpoint session for as long as anyone is reading it.
///
/// `visiting` changes exactly one thing: what a lost endpoint means. A local
/// endpoint that stops is revoked and its document is gone; a holder that
/// stops is unavailable and the last scene stays on screen.
fn run_hub(
    mut retained: RetainedEndpointSession,
    commands: Receiver<HubCommand>,
    wake: Wake,
    visiting: bool,
) {
    let mut mounted: Option<ProjectionSession> = None;
    let mut bindings = BTreeMap::<String, DocumentBinding>::new();
    let mut subscribers = BTreeMap::<u64, Subscriber>::new();
    // Only a visited hub has a holder that can vanish. A local endpoint that
    // stops has already answered the last request it will ever answer.
    let liveness = visiting.then(|| LivenessWatch::start(wake.clone()));
    loop {
        if let Some(watch) = &liveness {
            watch.heard();
        }
        match commands.recv_timeout(POLL_INTERVAL) {
            Ok(HubCommand::Open {
                registration,
                address,
                events,
                reply,
            }) => {
                let result = ensure_binding(&mut retained, &mut mounted, &mut bindings, &address)
                    .map(|binding| {
                        if let Some(watch) = &liveness {
                            watch.listen(events.clone());
                        }
                        subscribers.insert(registration, Subscriber { address, events });
                        (registration, binding)
                    });
                let _ = reply.send(result);
            }
            Ok(HubCommand::Save {
                registration,
                base_token,
                source,
            }) => {
                save_from_subscriber(
                    &mut retained,
                    mounted.as_ref(),
                    &mut bindings,
                    &subscribers,
                    registration,
                    base_token,
                    source,
                    &wake,
                    visiting,
                );
            }
            Ok(HubCommand::InsertClip {
                address,
                clip,
                status,
            }) => {
                insert_clip(
                    &mut retained,
                    &mut mounted,
                    &mut bindings,
                    &subscribers,
                    &address,
                    clip,
                    &status,
                    &wake,
                    visiting,
                );
            }
            Ok(HubCommand::Effect {
                registration,
                kind,
                confirmed,
            }) => {
                invoke_effect(
                    &mut retained,
                    mounted.as_ref(),
                    &mut bindings,
                    &subscribers,
                    registration,
                    kind,
                    confirmed,
                    &wake,
                    visiting,
                );
            }
            Ok(HubCommand::Reload { registration }) => {
                reload_subscriber(
                    &mut retained,
                    mounted.as_ref(),
                    &mut bindings,
                    &subscribers,
                    registration,
                    &wake,
                );
            }
            Ok(HubCommand::Unregister { registration }) => {
                subscribers.remove(&registration);
                if subscribers.is_empty() {
                    if let Some(session) = mounted.take() {
                        retained.forget(&session);
                    }
                    bindings.clear();
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if mounted.is_some() {
                    match retained.poll_for_change() {
                        Ok(true) => refresh_subscribers(
                            &mut retained,
                            mounted.as_ref().expect("checked"),
                            &mut bindings,
                            &subscribers,
                            &wake,
                            visiting,
                        ),
                        Ok(false) => {}
                        Err(error) => {
                            broadcast(&subscribers, lost(visiting, error), &wake);
                            break;
                        }
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(liveness);
    let _ = retained.close();
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

/// The visited-document liveness budget in force, default or overridden.
///
/// The right number depends on the link, not on this code: a LAN holder answers
/// in milliseconds, one behind a relay on a bad connection may not. Exposed so
/// a deployment that knows its link can say so.
fn visit_liveness_budget() -> Duration {
    env_integer("TURNSTONE_KNOT_VISIT_LIVENESS_MS", 0u64)
        .ok()
        .filter(|ms| *ms > 0)
        .map(Duration::from_millis)
        .unwrap_or(VISIT_LIVENESS_BUDGET)
}

impl LivenessWatch {
    fn start(wake: Wake) -> Self {
        let watch = Self {
            heard: Arc::new(Mutex::new(Instant::now())),
            listeners: Arc::new(Mutex::new(Vec::new())),
            stop: Arc::new(AtomicBool::new(false)),
        };
        let heard = Arc::clone(&watch.heard);
        let listeners = Arc::clone(&watch.listeners);
        let stop = Arc::clone(&watch.stop);
        let budget = visit_liveness_budget();
        std::thread::Builder::new()
            .name("knot-visit-liveness".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(LIVENESS_TICK);
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    let silent = heard
                        .lock()
                        .map(|last| last.elapsed())
                        .unwrap_or_default();
                    if silent < budget {
                        continue;
                    }
                    let sent = listeners
                        .lock()
                        .map(|listeners| {
                            let mut sent = 0usize;
                            for events in listeners.iter() {
                                if events
                                    .send(HubEvent::Unavailable(format!(
                                        "the holder has not answered for {} s",
                                        silent.as_secs()
                                    )))
                                    .is_ok()
                                {
                                    sent += 1;
                                }
                            }
                            sent
                        })
                        .unwrap_or(0);
                    // Nobody is reading this document yet, so there is nothing
                    // to tell; keep watching rather than declaring to no one.
                    if sent == 0 {
                        continue;
                    }
                    tracing::info!(
                        silent_ms = silent.as_millis() as u64,
                        "a visited document's holder went silent; reporting unavailable"
                    );
                    wake();
                    return;
                }
            })
            .expect("spawn the visited-document liveness watch");
        watch
    }

    /// The hub completed a round trip, or is about to start one.
    fn heard(&self) {
        if let Ok(mut last) = self.heard.lock() {
            *last = Instant::now();
        }
    }

    fn listen(&self, events: Sender<HubEvent>) {
        if let Ok(mut listeners) = self.listeners.lock() {
            listeners.push(events);
        }
    }
}

impl Drop for LivenessWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn ensure_binding(
    retained: &mut RetainedEndpointSession,
    mounted: &mut Option<ProjectionSession>,
    bindings: &mut BTreeMap<String, DocumentBinding>,
    address: &str,
) -> Result<DocumentBinding, String> {
    if let Some(binding) = bindings.get(address) {
        return Ok(binding.clone());
    }
    let session = match mounted {
        Some(session) => session.clone(),
        None => {
            let session = retained.mount(0)?;
            *mounted = Some(session.clone());
            session
        }
    };
    let binding = resolve_binding(retained, &session, address)?;
    bindings.insert(address.to_string(), binding.clone());
    Ok(binding)
}

fn resolve_binding(
    retained: &mut RetainedEndpointSession,
    session: &ProjectionSession,
    address: &str,
) -> Result<DocumentBinding, String> {
    retained
        .resolve_all(session)?
        .into_iter()
        .find_map(|(target, presentation)| binding_from_presentation(target, presentation, address))
        .ok_or_else(|| format!("Knot did not disclose writable text for {address}"))
}

fn binding_from_presentation(
    target: InstanceId,
    presentation: ResolvedPresentation,
    address: &str,
) -> Option<DocumentBinding> {
    let ResolvedContent::EditableText(editable) = presentation.content else {
        return None;
    };
    if !address_matches(address, &editable.address) {
        return None;
    }
    let save_action = presentation
        .semantics
        .actions
        .iter()
        .find(|action| action.intent.0 == EDITABLE_TEXT_SAVE_INTENT)?
        .clone();
    let clip_action = presentation
        .semantics
        .actions
        .iter()
        .find(|action| action.intent.0 == KNOT_CLIP_INSERT_INTENT)?
        .clone();
    let resolve_action = presentation
        .semantics
        .actions
        .iter()
        .find(|action| action.intent.0 == KNOT_TRANSCLUSION_RESOLVE_INTENT)
        .cloned();
    let run_action = presentation
        .semantics
        .actions
        .iter()
        .find(|action| action.intent.0 == KNOT_BLOCK_RUN_INTENT)
        .cloned();
    Some(DocumentBinding {
        target,
        save_action,
        clip_action,
        resolve_action,
        run_action,
        editable,
    })
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn save_from_subscriber(
    retained: &mut RetainedEndpointSession,
    session: Option<&ProjectionSession>,
    bindings: &mut BTreeMap<String, DocumentBinding>,
    subscribers: &BTreeMap<u64, Subscriber>,
    registration: u64,
    base_token: Vec<u8>,
    source: String,
    wake: &Wake,
    visiting: bool,
) {
    let Some(subscriber) = subscribers.get(&registration) else {
        return;
    };
    let Some(session) = session else {
        send_event(
            subscriber,
            HubEvent::Rejected("Knot projection is not mounted".into()),
            wake,
        );
        return;
    };
    let Some(binding) = bindings.get(&subscriber.address).cloned() else {
        send_event(
            subscriber,
            HubEvent::Rejected("Knot document is no longer writable".into()),
            wake,
        );
        return;
    };
    let result = retained.invoke(
        session,
        binding.target,
        &binding.save_action,
        &SaveTextV1 {
            base_token,
            source: source.clone(),
        },
    );
    match result {
        Ok(IntentResult::Accepted) => {
            let refresh = retained
                .wait_for_change()
                .and_then(|_| resolve_binding(retained, session, &subscriber.address));
            match refresh {
                Ok(binding) => {
                    bindings.insert(subscriber.address.clone(), binding.clone());
                    send_event(subscriber, HubEvent::Saved { source, binding }, wake);
                    refresh_subscribers(retained, session, bindings, subscribers, wake, visiting);
                }
                Err(error) => send_event(subscriber, HubEvent::Rejected(error), wake),
            }
        }
        Ok(IntentResult::Stale { .. }) => send_event(subscriber, HubEvent::Stale, wake),
        Ok(IntentResult::Rejected { reason }) => {
            send_event(subscriber, HubEvent::Rejected(reason), wake)
        }
        Err(error) => send_event(subscriber, HubEvent::Rejected(error), wake),
    }
}

#[allow(clippy::too_many_arguments)]
fn insert_clip(
    retained: &mut RetainedEndpointSession,
    mounted: &mut Option<ProjectionSession>,
    bindings: &mut BTreeMap<String, DocumentBinding>,
    subscribers: &BTreeMap<u64, Subscriber>,
    address: &str,
    clip: PendingClip,
    status: &Arc<Mutex<KnotClipStatus>>,
    wake: &Wake,
    visiting: bool,
) {
    let result = ensure_binding(retained, mounted, bindings, address).and_then(|binding| {
        let session = mounted
            .as_ref()
            .expect("ensure_binding mounted the session");
        if binding.clip_action.payload_schema == KNOT_CLIP_INSERT_SCHEMA_V2 {
            if clip.artifacts.is_empty() {
                return Err(
                    "this clip lane supplied no source artifact for Knot evidence retention".into(),
                );
            }
            retained.invoke(
                session,
                binding.target,
                &binding.clip_action,
                &InsertKnotClipV2 {
                    base_token: binding.editable.base_token.clone(),
                    source_url: clip.source_url,
                    title: clip.title,
                    selectors: clip.selectors,
                    knot_body: clip.knot_body,
                    artifacts: clip.artifacts,
                    fidelity: clip.fidelity,
                    discovered_edges: clip.discovered_edges,
                },
            )
        } else {
            retained.invoke(
                session,
                binding.target,
                &binding.clip_action,
                &InsertKnotClipV1 {
                    base_token: binding.editable.base_token.clone(),
                    source_url: clip.source_url,
                    title: clip.title,
                    selector: clip.selector,
                    knot_body: clip.knot_body,
                },
            )
        }
    });
    let next = match result {
        Ok(IntentResult::Accepted) => retained
            .wait_for_change()
            .and_then(|_| {
                resolve_binding(
                    retained,
                    mounted.as_ref().expect("session remains mounted"),
                    address,
                )
            })
            .map(|current| {
                bindings.insert(address.to_string(), current);
                KnotClipStatus::Saved
            })
            .unwrap_or_else(KnotClipStatus::Rejected),
        Ok(IntentResult::Stale { .. }) => KnotClipStatus::Stale,
        Ok(IntentResult::Rejected { reason }) => KnotClipStatus::Rejected(reason),
        Err(error) => KnotClipStatus::Rejected(error),
    };
    if let Ok(mut current) = status.lock() {
        *current = next;
    }
    if matches!(status.lock().as_deref(), Ok(KnotClipStatus::Saved))
        && let Some(session) = mounted.as_ref()
    {
        refresh_subscribers(retained, session, bindings, subscribers, wake, visiting);
    }
    wake();
}

#[allow(clippy::too_many_arguments)]
fn invoke_effect(
    retained: &mut RetainedEndpointSession,
    session: Option<&ProjectionSession>,
    bindings: &mut BTreeMap<String, DocumentBinding>,
    subscribers: &BTreeMap<u64, Subscriber>,
    registration: u64,
    kind: KnotEffectKind,
    confirmed: bool,
    wake: &Wake,
    visiting: bool,
) {
    let Some(subscriber) = subscribers.get(&registration) else {
        return;
    };
    let Some(session) = session else {
        send_event(
            subscriber,
            HubEvent::Rejected("Knot projection is not mounted".into()),
            wake,
        );
        return;
    };
    let Some(binding) = bindings.get(&subscriber.address).cloned() else {
        send_event(
            subscriber,
            HubEvent::Rejected("Knot document is no longer effect-capable".into()),
            wake,
        );
        return;
    };
    let action = match kind {
        KnotEffectKind::Resolve => binding.resolve_action.as_ref(),
        KnotEffectKind::Run => binding.run_action.as_ref(),
    };
    let Some(action) = action else {
        send_event(
            subscriber,
            HubEvent::Rejected(format!(
                "{} is disabled for this document",
                match kind {
                    KnotEffectKind::Resolve => "Resolve",
                    KnotEffectKind::Run => "Run",
                }
            )),
            wake,
        );
        return;
    };
    let result = retained.invoke(
        session,
        binding.target,
        action,
        &KnotEffectV1 {
            base_token: binding.editable.base_token,
            confirmed,
        },
    );
    match result {
        Ok(result) => {
            send_event(
                subscriber,
                HubEvent::Intent(ProjectionIntentReceipt {
                    target: binding.target,
                    intent: action.intent.0.clone(),
                    result: result.clone(),
                }),
                wake,
            );
            match result {
                IntentResult::Accepted => match retained.wait_for_change() {
                    Ok(_) => {
                        refresh_subscribers(retained, session, bindings, subscribers, wake, visiting)
                    },
                    Err(error) => send_event(subscriber, HubEvent::Rejected(error), wake),
                },
                IntentResult::Stale { .. } => send_event(subscriber, HubEvent::Stale, wake),
                IntentResult::Rejected { reason } => {
                    send_event(subscriber, HubEvent::Rejected(reason), wake)
                }
            }
        }
        Err(error) => send_event(subscriber, HubEvent::Rejected(error), wake),
    }
}

fn reload_subscriber(
    retained: &mut RetainedEndpointSession,
    session: Option<&ProjectionSession>,
    bindings: &mut BTreeMap<String, DocumentBinding>,
    subscribers: &BTreeMap<u64, Subscriber>,
    registration: u64,
    wake: &Wake,
) {
    let Some(subscriber) = subscribers.get(&registration) else {
        return;
    };
    let Some(session) = session else {
        send_event(
            subscriber,
            HubEvent::Rejected("Knot projection is not mounted".into()),
            wake,
        );
        return;
    };
    match resolve_binding(retained, session, &subscriber.address) {
        Ok(binding) => {
            bindings.insert(subscriber.address.clone(), binding.clone());
            send_event(subscriber, HubEvent::Reloaded(binding), wake);
        }
        Err(error) => send_event(subscriber, HubEvent::Revoked(error), wake),
    }
}

fn refresh_subscribers(
    retained: &mut RetainedEndpointSession,
    session: &ProjectionSession,
    bindings: &mut BTreeMap<String, DocumentBinding>,
    subscribers: &BTreeMap<u64, Subscriber>,
    wake: &Wake,
    visiting: bool,
) {
    let addresses = subscribers
        .values()
        .map(|subscriber| subscriber.address.clone())
        .collect::<BTreeSet<_>>();
    for address in addresses {
        match resolve_binding(retained, session, &address) {
            Ok(binding) => {
                bindings.insert(address.clone(), binding.clone());
                for subscriber in subscribers
                    .values()
                    .filter(|subscriber| subscriber.address == address)
                {
                    send_event(subscriber, HubEvent::Remote(binding.clone()), wake);
                }
            }
            Err(error) => {
                bindings.remove(&address);
                for subscriber in subscribers
                    .values()
                    .filter(|subscriber| subscriber.address == address)
                {
                    send_event(subscriber, lost(visiting, error.clone()), wake);
                }
            }
        }
    }
}

fn broadcast(subscribers: &BTreeMap<u64, Subscriber>, event: HubEvent, wake: &Wake) {
    for subscriber in subscribers.values() {
        send_event(subscriber, event.clone(), wake);
    }
}

/// What a lost endpoint is called, which depends on whose it was.
fn lost(visiting: bool, reason: String) -> HubEvent {
    if visiting {
        HubEvent::Unavailable(reason)
    } else {
        HubEvent::Revoked(reason)
    }
}

fn send_event(subscriber: &Subscriber, event: HubEvent, wake: &Wake) {
    if subscriber.events.send(event).is_ok() {
        wake();
    }
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthoringRequest {
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

fn authoring_view(state: &AuthoringState) -> AuthoringView {
    let styles = state
        .editor
        .highlights()
        .into_iter()
        .map(|span| StyleRange {
            range: span.range,
            class: format!("syntax-{:?}", span.kind).to_ascii_lowercase(),
        })
        .collect::<Vec<_>>();
    let field = lens(
        move |input: &mut cambium::TextInput| styled_textarea(input, &styles),
        |state: &mut AuthoringState| state.editor.input_mut(),
    );
    let status = match state.status {
        AuthoringStatus::Unavailable => "unavailable".to_string(),
        AuthoringStatus::Revoked => "closed".to_string(),
        AuthoringStatus::Stale => "stale; reload or resolve".to_string(),
        AuthoringStatus::Saving => "saving".to_string(),
        AuthoringStatus::Resolving => "resolving".to_string(),
        AuthoringStatus::Running => "running".to_string(),
        AuthoringStatus::Reloading => "reloading".to_string(),
        AuthoringStatus::Rejected => state.detail.clone(),
        AuthoringStatus::Current if state.editor.is_dirty() => "unsaved".to_string(),
        AuthoringStatus::Current => "saved".to_string(),
    };
    let outline = state
        .editor
        .outline()
        .into_iter()
        .map(|item| {
            format!(
                "{}{}",
                "  ".repeat(item.level.saturating_sub(1) as usize),
                item.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let fold_count = state.editor.folds().len();
    let title = state
        .editor
        .source()
        .lines()
        .next()
        .unwrap_or("Knot document")
        .to_string();
    let preview = match &state.derived {
        Some(derived) => KnotEditor::scratch("knot:derived", derived.source.clone())
            .preview()
            .map(|document| format!("{}\n\n{}", derived_status(derived), document.to_markdown()))
            .unwrap_or_else(|error| format!("Derived preview unavailable: {error}")),
        None => state
            .editor
            .preview()
            .map(|document| document.to_markdown())
            .unwrap_or_else(|error| format!("Preview unavailable: {error}")),
    };
    let toolbar = el::<_, AuthoringState, ()>(
        "div",
        (
            el("div", title).attr("class", "knot-title"),
            el("div", status).attr("class", "knot-status"),
            button(
                "Save",
                |state: &mut AuthoringState, _click: PointerClick| {
                    state.requests.push(AuthoringRequest::Save);
                },
            )
            .attr("class", "knot-save")
            .attr(
                "data-projection-instance",
                state.projection_target.0.to_string(),
            )
            .attr("data-projection-intent", EDITABLE_TEXT_SAVE_INTENT)
            // Not merely disabled: an unreachable holder has no save to
            // offer, and a button that cannot work is worse than none.
            .attr(
                "style",
                if matches!(state.status, AuthoringStatus::Unavailable) {
                    "display: none;"
                } else {
                    ""
                },
            ),
            button(
                "Resolve",
                |state: &mut AuthoringState, _click: PointerClick| {
                    if state.resolve_available {
                        state.requests.push(AuthoringRequest::Resolve);
                    }
                },
            )
            .attr("class", "knot-effect knot-resolve")
            .attr(
                "data-projection-instance",
                state.projection_target.0.to_string(),
            )
            .attr("data-projection-intent", KNOT_TRANSCLUSION_RESOLVE_INTENT)
            .attr(
                "style",
                if state.resolve_available {
                    ""
                } else {
                    "display: none;"
                },
            ),
            button("Run", |state: &mut AuthoringState, _click: PointerClick| {
                if state.run_available {
                    state.requests.push(AuthoringRequest::Run);
                }
            })
            .attr("class", "knot-effect knot-run")
            .attr(
                "data-projection-instance",
                state.projection_target.0.to_string(),
            )
            .attr("data-projection-intent", KNOT_BLOCK_RUN_INTENT)
            .attr(
                "style",
                if state.run_available {
                    ""
                } else {
                    "display: none;"
                },
            ),
            button(
                "Reload",
                |state: &mut AuthoringState, _click: PointerClick| {
                    state.requests.push(AuthoringRequest::Reload);
                },
            )
            .attr("class", "knot-reload"),
        ),
    )
    .attr("class", "knot-toolbar");
    let intent_receipt = state.last_intent.as_ref().map(|receipt| {
        el::<_, AuthoringState, ()>("div", receipt.line())
            .attr("class", "knot-intent-receipt")
            .attr("data-projection-instance", receipt.target.0.to_string())
            .attr("data-projection-intent", receipt.intent.clone())
            .attr("data-intent-result", receipt.result_word())
    });
    let editor = el::<_, AuthoringState, ()>("div", field).attr("class", "knot-editor-wrap");
    let readout = el::<_, AuthoringState, ()>(
        "div",
        format!("Outline\n{outline}\n\nFolds: {fold_count}\n\nPreview\n{preview}"),
    )
    .attr("class", "knot-readout");
    Box::new(
        el::<_, AuthoringState, ()>(
            "div",
            (
                toolbar,
                intent_receipt,
                el("div", (editor, readout)).attr("class", "knot-body"),
            ),
        )
        .attr("class", "knot-root")
        .attr(
            "style",
            format!("width: {}px; height: {}px;", state.width, state.height),
        ),
    )
}

fn derived_status(derived: &DerivedTextV1) -> String {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    derived_status_at(derived, now_ms)
}

fn derived_status_at(derived: &DerivedTextV1, now_ms: u64) -> String {
    let Some(cache) = &derived.cache else {
        return derived.summary.clone();
    };
    let age = format_cache_age(now_ms.saturating_sub(cache.fetched_at_unix_ms));
    let source_label = if cache.sources.len() == 1 {
        "source"
    } else {
        "sources"
    };
    format!(
        "{}\nFetched: {age} ago; {} {source_label}",
        derived.summary,
        cache.sources.len()
    )
}

fn format_cache_age(age_ms: u64) -> String {
    let seconds = age_ms / 1_000;
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 60 * 60 {
        format!("{}m", seconds / 60)
    } else if seconds < 24 * 60 * 60 {
        format!("{}h", seconds / (60 * 60))
    } else {
        format!("{}d", seconds / (24 * 60 * 60))
    }
}

/// The visible editor retained in Turnstone's ordinary content-session map.
pub struct KnotDocumentSession {
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
}

impl KnotDocumentSession {
    fn new(
        hub: Arc<KnotHub>,
        opened: OpenedDocument,
        viewport: (u32, u32),
        auto_resolve: bool,
        auto_run: bool,
        visiting: bool,
    ) -> Self {
        let address = opened.binding.editable.address.clone();
        let base_token = opened.binding.editable.base_token.clone();
        let projection_input = knot_projection_input_binding(&opened.binding.editable);
        let resolve_available = opened.binding.resolve_action.is_some();
        let run_available = opened.binding.run_action.is_some();
        let dom: DomHandle = Rc::new(std::cell::RefCell::new(ScriptedDom::new()));
        let state = AuthoringState {
            editor: KnotEditor::scratch(&address, opened.binding.editable.source),
            projection_target: opened.binding.target,
            last_intent: None,
            status: AuthoringStatus::Current,
            detail: String::new(),
            requests: Vec::new(),
            resolve_available,
            run_available,
            derived: opened.binding.editable.derived,
            width: viewport.0.max(1),
            height: viewport.1.max(1),
        };
        let runner = AuthoringRunner::new(
            dom.clone(),
            authoring_view as fn(&AuthoringState) -> AuthoringView,
            state,
        );
        let session = Self {
            hub,
            registration: opened.registration,
            address,
            visiting,
            base_token,
            projection_input: Some(projection_input),
            events: opened.events,
            revision_refreshes: 0,
            dom,
            runner,
            layout: crate::ui::RetainedLayout::new(),
        };
        if auto_resolve && resolve_available {
            let _ = session.hub.commands.send(HubCommand::Effect {
                registration: session.registration,
                kind: KnotEffectKind::Resolve,
                confirmed: false,
            });
        }
        if auto_run && run_available {
            let _ = session.hub.commands.send(HubCommand::Effect {
                registration: session.registration,
                kind: KnotEffectKind::Run,
                confirmed: false,
            });
        }
        session
    }

    pub fn dispatch_key(&mut self, event: KeyEvent) -> bool {
        self.drain_events();
        if matches!(&event.key, Key::Character(value) if event.mods.ctrl && value.eq_ignore_ascii_case("s"))
        {
            self.save();
            return true;
        }
        if self.runner.focus().is_none() {
            return false;
        }
        let before = self.runner.state().editor.source().to_string();
        self.runner.dispatch_key(event);
        if self.runner.state().editor.source() != before {
            self.runner.update(|state| state.derived = None);
        }
        true
    }

    /// The retained document surface exposed to Genet Probe. The borrow stays
    /// inside `Automatable::with_surfaces`, matching Turnstone's pane DOMs.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The holder's Personae root, when this document is place-held.
    pub fn holder(&self) -> Option<String> {
        parse_place_held_address(&self.address).map(|(root, _)| crate::place::hex32(&root))
    }

    /// Where this document is held and whether it can be reached: `local`,
    /// `visiting`, or `unavailable`. A refusal never reaches a session, so it
    /// is not one of these.
    pub fn hold_status(&self) -> &'static str {
        if matches!(self.runner.state().status, AuthoringStatus::Unavailable) {
            "unavailable"
        } else if self.visiting {
            "visiting"
        } else {
            "local"
        }
    }

    /// Blake3 hex over the derived source text this surface currently holds.
    ///
    /// The derived text when the endpoint disclosed one, the editor's own
    /// source otherwise; a local and a visited window that agree on the
    /// document agree on this string.
    pub fn derived_digest(&self) -> String {
        let state = self.runner.state();
        let source = state
            .derived
            .as_ref()
            .map(|derived| derived.source.as_str())
            .unwrap_or_else(|| state.editor.source());
        blake3::hash(source.as_bytes()).to_hex().to_string()
    }

    pub fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }

    pub fn status(&mut self) -> &'static str {
        self.drain_events();
        match self.runner.state().status {
            AuthoringStatus::Current if self.runner.state().editor.is_dirty() => "unsaved",
            AuthoringStatus::Current => "saved",
            AuthoringStatus::Saving => "saving",
            AuthoringStatus::Resolving => "resolving",
            AuthoringStatus::Running => "running",
            AuthoringStatus::Reloading => "reloading",
            AuthoringStatus::Stale => "stale",
            AuthoringStatus::Rejected => "rejected",
            AuthoringStatus::Revoked => "revoked",
            AuthoringStatus::Unavailable => "unavailable",
        }
    }

    fn save(&mut self) {
        if matches!(
            self.runner.state().status,
            AuthoringStatus::Saving
                | AuthoringStatus::Resolving
                | AuthoringStatus::Running
                | AuthoringStatus::Reloading
                | AuthoringStatus::Revoked
                | AuthoringStatus::Unavailable
        ) || !self.runner.state().editor.is_dirty()
        {
            return;
        }
        let source = self.runner.state().editor.source().to_string();
        let command = HubCommand::Save {
            registration: self.registration,
            base_token: self.base_token.clone(),
            source,
        };
        if self.hub.commands.send(command).is_ok() {
            self.runner.update(|state| {
                state.status = AuthoringStatus::Saving;
                state.detail.clear();
            });
        } else {
            self.runner.update(|state| {
                state.status = AuthoringStatus::Rejected;
                state.detail = "Knot worker stopped".into();
            });
        }
    }

    fn reload(&mut self) {
        if matches!(
            self.runner.state().status,
            AuthoringStatus::Saving
                | AuthoringStatus::Resolving
                | AuthoringStatus::Running
                | AuthoringStatus::Reloading
                | AuthoringStatus::Revoked
                | AuthoringStatus::Unavailable
        ) {
            return;
        }
        if self
            .hub
            .commands
            .send(HubCommand::Reload {
                registration: self.registration,
            })
            .is_ok()
        {
            self.runner.update(|state| {
                state.status = AuthoringStatus::Reloading;
                state.detail.clear();
            });
        } else {
            self.runner.update(|state| {
                state.status = AuthoringStatus::Rejected;
                state.detail = "Knot worker stopped".into();
            });
        }
    }

    /// The current Knot input in Scenograph's durable binding vocabulary.
    ///
    /// A synchronized vault resource carries its authority-issued operation
    /// head as a [`PublicSourceRevision`]. A file or other source without one
    /// instead reports `RuntimeVerified`, and is bindable only through
    /// [`Self::runtime_projection_source`].
    pub fn projection_input_binding(&self) -> Option<&ProjectionInputBinding> {
        self.projection_input.as_ref()
    }

    /// Return the live-only Scenograph source binding for an unversioned Knot
    /// resource.
    ///
    /// The returned witness retains the save token privately. It cannot be
    /// produced for a synchronized vault, whose public operation head belongs
    /// in durable provenance instead.
    pub fn runtime_projection_source(&self) -> Option<RuntimeSourceBinding<KnotRuntimeWitness>> {
        self.projection_input
            .as_ref()
            .filter(|binding| binding.revision_evidence == RevisionEvidence::RuntimeVerified)
            .map(|binding| {
                knot_runtime_source_binding(binding.source.clone(), self.base_token.clone())
            })
    }

    fn invoke_effect(&mut self, kind: KnotEffectKind) {
        let available = match kind {
            KnotEffectKind::Resolve => self.runner.state().resolve_available,
            KnotEffectKind::Run => self.runner.state().run_available,
        };
        if !available
            || matches!(
                self.runner.state().status,
                AuthoringStatus::Saving
                    | AuthoringStatus::Resolving
                    | AuthoringStatus::Running
                    | AuthoringStatus::Reloading
                    | AuthoringStatus::Revoked
                    | AuthoringStatus::Unavailable
            )
            || self.runner.state().editor.is_dirty()
        {
            return;
        }
        if self
            .hub
            .commands
            .send(HubCommand::Effect {
                registration: self.registration,
                kind,
                confirmed: true,
            })
            .is_ok()
        {
            self.runner.update(|state| {
                state.last_intent = None;
                state.status = match kind {
                    KnotEffectKind::Resolve => AuthoringStatus::Resolving,
                    KnotEffectKind::Run => AuthoringStatus::Running,
                };
                state.detail.clear();
            });
        } else {
            self.runner.update(|state| {
                state.status = AuthoringStatus::Rejected;
                state.detail = "Knot worker stopped".into();
            });
        }
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                HubEvent::Remote(binding) => {
                    self.revision_refreshes += 1;
                    let target = binding.target;
                    self.projection_input = Some(knot_projection_input_binding(&binding.editable));
                    if binding.editable.base_token == self.base_token {
                        let resolve_available = binding.resolve_action.is_some();
                        let run_available = binding.run_action.is_some();
                        let derived = binding.editable.derived;
                        if !self.runner.state().editor.is_dirty() {
                            self.runner.update(|state| {
                                state.projection_target = target;
                                state.resolve_available = resolve_available;
                                state.run_available = run_available;
                                state.derived = derived;
                                state.status = AuthoringStatus::Current;
                                state.detail.clear();
                            });
                        } else {
                            self.runner.update(|state| {
                                state.projection_target = target;
                                state.resolve_available = resolve_available;
                                state.run_available = run_available;
                            });
                        }
                        continue;
                    }
                    if self.runner.state().editor.is_dirty() {
                        self.runner.update(|state| {
                            state.projection_target = target;
                            state.status = AuthoringStatus::Stale;
                            state.detail = "the endpoint has a newer document".into();
                        });
                    } else {
                        self.base_token = binding.editable.base_token;
                        let source = binding.editable.source;
                        let derived = binding.editable.derived;
                        let resolve_available = binding.resolve_action.is_some();
                        let run_available = binding.run_action.is_some();
                        let address = self.address.clone();
                        self.runner.update(|state| {
                            state.projection_target = target;
                            state.editor = KnotEditor::scratch(address, source);
                            state.derived = derived;
                            state.resolve_available = resolve_available;
                            state.run_available = run_available;
                            state.status = AuthoringStatus::Current;
                            state.detail.clear();
                        });
                    }
                }
                HubEvent::Intent(receipt) => self.runner.update(|state| {
                    state.last_intent = Some(receipt);
                }),
                HubEvent::Reloaded(binding) => {
                    let target = binding.target;
                    self.projection_input = Some(knot_projection_input_binding(&binding.editable));
                    self.base_token = binding.editable.base_token;
                    let source = binding.editable.source;
                    let derived = binding.editable.derived;
                    let resolve_available = binding.resolve_action.is_some();
                    let run_available = binding.run_action.is_some();
                    let address = self.address.clone();
                    self.runner.update(|state| {
                        state.projection_target = target;
                        state.editor = KnotEditor::scratch(address, source);
                        state.derived = derived;
                        state.resolve_available = resolve_available;
                        state.run_available = run_available;
                        state.status = AuthoringStatus::Current;
                        state.detail.clear();
                    });
                }
                HubEvent::Saved { source, binding } => {
                    let target = binding.target;
                    self.projection_input = Some(knot_projection_input_binding(&binding.editable));
                    self.base_token = binding.editable.base_token;
                    let derived = binding.editable.derived;
                    let resolve_available = binding.resolve_action.is_some();
                    let run_available = binding.run_action.is_some();
                    self.runner.update(|state| {
                        state.projection_target = target;
                        state.editor.accept_saved_source(&source);
                        state.derived = derived;
                        state.resolve_available = resolve_available;
                        state.run_available = run_available;
                        state.status = AuthoringStatus::Current;
                        state.detail.clear();
                    });
                }
                HubEvent::Stale => self.runner.update(|state| {
                    state.status = AuthoringStatus::Stale;
                    state.detail = "the endpoint refused an old base token".into();
                }),
                HubEvent::Rejected(reason) => self.runner.update(|state| {
                    state.status = AuthoringStatus::Rejected;
                    state.detail = reason;
                }),
                HubEvent::Unavailable(reason) => {
                    // The editor is left exactly as it stands. Clearing it
                    // would erase the last thing the holder actually said,
                    // which is the only copy a visitor has any claim to.
                    self.base_token.clear();
                    self.projection_input = None;
                    self.runner.update(|state| {
                        state.resolve_available = false;
                        state.run_available = false;
                        state.status = AuthoringStatus::Unavailable;
                        state.detail = reason;
                    });
                }
                HubEvent::Revoked(reason) => {
                    self.base_token.clear();
                    self.projection_input = None;
                    let address = self.address.clone();
                    self.runner.update(|state| {
                        state.editor = KnotEditor::scratch(address, String::new());
                        state.derived = None;
                        state.resolve_available = false;
                        state.run_available = false;
                        state.status = AuthoringStatus::Revoked;
                        state.detail = reason;
                    });
                }
            }
        }
    }
}

impl Drop for KnotDocumentSession {
    fn drop(&mut self) {
        let _ = self.hub.commands.send(HubCommand::Unregister {
            registration: self.registration,
        });
    }
}

impl DocumentSession<Scene> for KnotDocumentSession {
    fn frame(&mut self, width: u32, height: u32) -> Scene {
        self.drain_events();
        if (self.runner.state().width, self.runner.state().height) != (width, height) {
            self.runner.update(|state| {
                state.width = width;
                state.height = height;
            });
        }
        let sheet = format!("{} {}", crate::ui::CAMBIUM_SHEET, KNOT_SHEET);
        self.layout
            .scene(&mut self.dom.borrow_mut(), &sheet, width, height)
    }

    fn scroll_by(&mut self, _dx: f32, _dy: f32) -> bool {
        false
    }

    fn scroll_for_key(&mut self, _key: SessionScrollKey) -> bool {
        false
    }

    fn click_at(&mut self, x: f32, y: f32) -> SessionClick {
        self.drain_events();
        let size = (self.runner.state().width, self.runner.state().height);
        let hit = {
            let dom = self.dom.borrow();
            let sheet = format!("{} {}", crate::ui::CAMBIUM_SHEET, KNOT_SHEET);
            crate::ui::hit_test(&dom, &sheet, size.0, size.1, x, y)
        };
        let Some(node) = hit else {
            return SessionClick::Miss;
        };
        let _: Vec<()> = self.runner.dispatch_click(node, PointerClick::at((x, y)));
        // One drain, in the order the toolbar was pressed. Taking the whole
        // queue in a single `update` also costs one rebuild rather than one
        // per verb.
        let requests = if self.runner.state().requests.is_empty() {
            Vec::new()
        } else {
            let mut taken = Vec::new();
            self.runner
                .update(|state| taken = std::mem::take(&mut state.requests));
            taken
        };
        for request in requests {
            match request {
                AuthoringRequest::Save => self.save(),
                AuthoringRequest::Resolve => self.invoke_effect(KnotEffectKind::Resolve),
                AuthoringRequest::Run => self.invoke_effect(KnotEffectKind::Run),
                AuthoringRequest::Reload => self.reload(),
            }
        }
        SessionClick::Handled
    }

    fn links(&self) -> Vec<SessionLink> {
        Vec::new()
    }

    fn settled(&mut self) -> bool {
        self.drain_events();
        !matches!(
            self.runner.state().status,
            AuthoringStatus::Saving
                | AuthoringStatus::Resolving
                | AuthoringStatus::Running
                | AuthoringStatus::Reloading
        )
    }

    fn inspect(&self) -> Option<ContentReport> {
        let outline = self
            .runner
            .state()
            .editor
            .outline()
            .into_iter()
            .map(|item| OutlineEntry {
                depth: item.level.saturating_sub(1) as usize,
                role: "heading",
                name: item.text,
            })
            .collect::<Vec<_>>();
        Some(ContentReport {
            title: Some(self.address.clone()),
            headings: outline.iter().map(|entry| entry.name.clone()).collect(),
            outline,
            links: Vec::new(),
            lineage: None,
        })
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;
    use std::time::Duration;

    use cambium::{CompositionEvent, Modifiers};
    use genet_scripted_dom::NodeId;
    use layout_dom_api::{LayoutDom, NodeKind};

    use super::*;

    /// A non-founder process inherits every name its parent exported, blank
    /// ones included: `TURNSTONE_KNOT_ROOT=` must read as "no vault of my own",
    /// not as a path at the current directory.
    #[test]
    fn a_blank_path_setting_reads_as_unset() {
        let name = "TURNSTONE_KNOT_ROOT_ENV_PATH_PROBE";
        // SAFETY: a name used by this test alone, set and read on one thread.
        unsafe {
            std::env::remove_var(name);
        }
        assert_eq!(env_path(name), None, "an absent name is unset");
        for blank in ["", " ", "\t  \n"] {
            unsafe {
                std::env::set_var(name, blank);
            }
            assert_eq!(
                env_path(name),
                None,
                "a blank value is unset, not a path at the current directory"
            );
        }
        unsafe {
            std::env::set_var(name, " C:/vault ");
        }
        assert_eq!(
            env_path(name),
            Some(PathBuf::from(" C:/vault ")),
            "a value with content is passed through verbatim, spaces and all"
        );
        unsafe {
            std::env::remove_var(name);
        }
    }

    fn editable_projection_fixture(
        address: &str,
        public_revision: Option<[u8; 32]>,
        base_token: Vec<u8>,
    ) -> EditableTextV1 {
        EditableTextV1 {
            address: address.to_owned(),
            media_type: "text/vnd.knot".to_owned(),
            encoding: graphshell::protocol::TextEncoding::Utf8,
            source: "# Field\n".to_owned(),
            base_token,
            public_revision,
            derived: None,
        }
    }

    fn knot_projection_recipe(
        binding: ProjectionInputBinding,
    ) -> graphshell::projection_editor::AuthoredProjectionDefinition {
        use graphshell::projection_editor::{
            Appearance, Arrangement, AuthoredProjectionDefinition, Channel, Encoding, Interaction,
            PROJECTION_DEFINITION_VERSION, Provenance, Reading,
        };

        AuthoredProjectionDefinition {
            version: PROJECTION_DEFINITION_VERSION,
            id: "turnstone.knot.outline".to_owned(),
            label: "Knot outline".to_owned(),
            sources: BTreeMap::from([("document".to_owned(), binding)]),
            reading: Reading {
                kind: "knot.outline".to_owned(),
                key: "source_range".to_owned(),
                value: Some("heading".to_owned()),
            },
            encoding: Encoding {
                x: Channel::Field("source_range.start".to_owned()),
                y: Channel::Field("depth".to_owned()),
                color: None,
                label: Some(Channel::Field("heading".to_owned())),
            },
            arrangement: Arrangement::default(),
            interaction: Interaction::default(),
            appearance: Appearance {
                realization: "turnstone.knot-outline".to_owned(),
                title: "Knot outline".to_owned(),
                theme: "turnstone".to_owned(),
            },
            provenance: Provenance {
                author: "turnstone-test".to_owned(),
                source_revision: Some(PublicSourceRevision::from("turnstone.knot.outline/v1")),
                revision_evidence: RevisionEvidence::PublicGeneration,
                note: "fixture".to_owned(),
            },
        }
    }

    #[test]
    fn synced_knot_head_becomes_public_scenograph_revision_without_save_token() {
        let head = [0x5a; 32];
        let editable = editable_projection_fixture(
            "knot://vault/field-note",
            Some(head),
            vec![0xde, 0xad, 0xbe, 0xef],
        );

        let input = knot_projection_input_binding(&editable);
        assert_eq!(input.source.authority, KNOT_PROJECTION_AUTHORITY);
        assert_eq!(input.source.domain, KNOT_PROJECTION_DOMAIN);
        assert_eq!(input.source.resource, "knot://vault/field-note");
        assert_eq!(
            input.expects_generation,
            Some(PublicSourceRevision::new(knot::hex32(&head)))
        );
        assert_eq!(input.revision_evidence, RevisionEvidence::PublicGeneration);
        assert!(editable.public_revision.is_some());

        let definition = knot_projection_recipe(input)
            .bind("document", None)
            .expect("a public source generation binds durably");
        let json = String::from_utf8(definition.to_json_bytes().unwrap()).unwrap();
        assert!(json.contains(&knot::hex32(&head)));
        assert!(!json.contains("base_token"));
        assert!(!json.contains("deadbeef"));
    }

    #[test]
    fn unversioned_knot_source_requires_a_runtime_witness_without_durable_token() {
        let editable = editable_projection_fixture(
            "file:///C:/notes/field.knot",
            None,
            vec![0xde, 0xad, 0xbe, 0xef],
        );
        let input = knot_projection_input_binding(&editable);
        assert_eq!(input.expects_generation, None);
        assert_eq!(input.revision_evidence, RevisionEvidence::RuntimeVerified);
        let runtime_source =
            knot_runtime_source_binding(input.source.clone(), editable.base_token.clone());

        let binding = knot_projection_recipe(input)
            .bind_runtime("document", runtime_source, None)
            .expect("runtime-verified source binds only in this process");
        assert!(binding.with_witness(|witness| witness.matches_save_token(&editable.base_token)));
        let recorded = binding.into_recorded_definition();
        assert_eq!(recorded.provenance.source_revision, None);
        assert_eq!(
            recorded.provenance.revision_evidence,
            RevisionEvidence::RuntimeVerified
        );
        let json = String::from_utf8(recorded.to_json_bytes().unwrap()).unwrap();
        assert!(json.contains("\"revision_evidence\":\"runtime_verified\""));
        assert!(!json.contains("source_revision"));
        assert!(!json.contains("base_token"));
        assert!(!json.contains("deadbeef"));
    }

    #[test]
    fn sealed_cache_age_uses_readable_units() {
        assert_eq!(format_cache_age(999), "0s");
        assert_eq!(format_cache_age(59_999), "59s");
        assert_eq!(format_cache_age(60_000), "1m");
        assert_eq!(format_cache_age(3_600_000), "1h");
        assert_eq!(format_cache_age(86_400_000), "1d");
    }

    #[test]
    fn sealed_cache_attribution_is_visible_in_the_derived_preview() {
        let derived = DerivedTextV1 {
            source: "# Cached".into(),
            summary: "resolved 1; denied 0; failed 0".into(),
            cache: Some(graphshell::protocol::DerivedCacheInfoV1 {
                effect: "resolve".into(),
                sources: vec!["https://example.test/note".into()],
                provider_version: "fixture/v1".into(),
                policy_fingerprint: "policy".into(),
                fetched_at_unix_ms: 1_000,
                source_revision: 1,
            }),
        };

        assert_eq!(
            derived_status_at(&derived, 61_000),
            "resolved 1; denied 0; failed 0\nFetched: 1m ago; 1 source"
        );
    }

    #[test]
    fn projected_action_is_probe_drivable_by_instance_identity() {
        let dom: DomHandle = Rc::new(std::cell::RefCell::new(ScriptedDom::new()));
        let _runner = AuthoringRunner::new(
            dom.clone(),
            authoring_view as fn(&AuthoringState) -> AuthoringView,
            AuthoringState {
                editor: KnotEditor::scratch("knot:test", "# Test"),
                projection_target: InstanceId(17),
                last_intent: Some(ProjectionIntentReceipt {
                    target: InstanceId(17),
                    intent: KNOT_TRANSCLUSION_RESOLVE_INTENT.into(),
                    result: IntentResult::Accepted,
                }),
                status: AuthoringStatus::Current,
                detail: String::new(),
                requests: Vec::new(),
                resolve_available: true,
                run_available: true,
                derived: None,
                width: 900,
                height: 600,
            },
        );
        let dom = dom.borrow();
        let surfaces = [taproot::ProbeSurface {
            name: "knot-authoring",
            dom: &dom,
            rect: [0.0, 0.0, 900.0, 600.0],
            sheet: KNOT_SHEET,
        }];

        let hit = taproot::resolve(
            &surfaces,
            &taproot::Selector::class("knot-resolve")
                .with_attr("data-projection-instance", "17"),
        );
        assert!(
            hit.is_some(),
            "the probe must resolve the action by InstanceId"
        );
        assert!(taproot::text_present(
            &surfaces,
            "Projection intent 17: knot.transclusion.resolve accepted"
        ));
    }

    fn file_address(path: &Path) -> String {
        let path = fs::canonicalize(path).expect("test document should canonicalize");
        #[cfg(windows)]
        {
            let path = path.to_string_lossy();
            let path = path.strip_prefix(r"\\?\").unwrap_or(&path);
            format!("file:///{}", path.replace('\\', "/"))
        }
        #[cfg(not(windows))]
        {
            format!("file://{}", path.to_string_lossy())
        }
    }

    fn find_element(dom: &ScriptedDom, node: NodeId, name: &str) -> Option<NodeId> {
        if dom.kind(node) == NodeKind::Element
            && dom
                .element_name(node)
                .is_some_and(|element| element.local.as_ref() == name)
        {
            return Some(node);
        }
        dom.dom_children(node)
            .find_map(|child| find_element(dom, child, name))
    }

    fn focus_editor(session: &mut KnotDocumentSession) {
        let textarea = {
            let dom = session.dom.borrow();
            find_element(&dom, session.runner.root(), "textarea")
                .expect("authoring view should contain a textarea")
        };
        session
            .runner
            .dispatch_click(textarea, PointerClick::at((1.0, 1.0)));
        assert_eq!(session.runner.focus(), Some(textarea));
    }

    fn wait_for_status(session: &mut KnotDocumentSession, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let actual = session.status();
            if actual == expected {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "expected status {expected}, found {actual}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_for_refresh(session: &mut KnotDocumentSession, previous: u64) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while session.revision_refreshes == previous {
            session.drain_events();
            assert!(
                Instant::now() < deadline,
                "endpoint revision bell did not reach the dirty editor"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_for_derived(session: &mut KnotDocumentSession, summary_prefix: &str) -> DerivedTextV1 {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            session.drain_events();
            if let Some(derived) = session.runner.state().derived.as_ref()
                && derived.summary.starts_with(summary_prefix)
            {
                return derived.clone();
            }
            assert!(
                Instant::now() < deadline,
                "expected derived result starting with {summary_prefix}, status {}",
                session.status()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn type_text(session: &mut KnotDocumentSession, text: &str) {
        assert!(session.dispatch_key(KeyEvent::new(Key::Character(text.into()))));
    }

    fn save_shortcut(session: &mut KnotDocumentSession) {
        assert!(session.dispatch_key(KeyEvent::with_mods(
            Key::Character("s".into()),
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        )));
    }

    fn tree_contains(root: &Path, needle: &[u8]) -> bool {
        fs::read_dir(root).unwrap().flatten().any(|entry| {
            let path = entry.path();
            if path.is_dir() {
                tree_contains(&path, needle)
            } else {
                fs::read(path)
                    .ok()
                    .is_some_and(|bytes| bytes.windows(needle.len()).any(|part| part == needle))
            }
        })
    }

    fn wait_for_clip_status(handle: &KnotClipHandle, expected: KnotClipStatus) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let actual = handle.status();
            if actual == expected {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "expected clip status {expected:?}, found {actual:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn knot_addresses_are_selected_without_claiming_other_files() {
        assert!(is_knot_address("file:///C:/notes/one.djot"));
        assert!(is_knot_address("file:///C:/notes/one.knot"));
        assert!(is_knot_address("KNOT:vault/document"));
        assert!(is_knot_address("file:///tmp/one.DJOT?mode=edit"));
        assert!(is_knot_address("file:///tmp/one.KNOT#heading"));
        assert!(!is_knot_address("file:///tmp/one.md"));
        assert!(!is_knot_address("https://example.test/"));
    }

    #[test]
    fn a_place_held_address_names_its_holder_and_stays_a_knot_address() {
        let root = [0xab; 32];
        let address = place_held_address(&root, "notes/field.knot");
        assert_eq!(
            address,
            format!("knot://{}/notes/field.knot", "ab".repeat(32))
        );
        assert!(
            is_knot_address(&address),
            "the ordinary open route must still recognize it"
        );
        assert_eq!(
            parse_place_held_address(&address),
            Some((root, "notes/field.knot".to_string()))
        );
    }

    /// T5c's rootless half. A member with no vault of its own still has to be
    /// able to visit someone else's document: the engine registers either way,
    /// so a place-held address reaches the visiting path (and fails there,
    /// about the visit) instead of falling off the content registry into the
    /// external-protocol route. Only a LOCAL address is refused, by name.
    #[test]
    fn a_member_with_no_vault_visits_but_holds_nothing() {
        let engine = KnotAuthoringEngine::visit_only(KnotVisits::default(), None);

        let local = "file:///C:/vault/field.knot";
        assert!(is_knot_address(local));
        let refusal = engine
            .spawn(&SessionSpawnRequest::new(local))
            .err()
            .expect("a local address has no vault to open it");
        assert!(
            matches!(&refusal, SessionError::SpawnFailed(reason)
                if reason.contains("no Knot vault is configured")),
            "a local address is refused by the missing vault, not the route: {refusal:?}"
        );

        // The place-held address takes the VISITING branch. No visit is open in
        // this test, so it fails there — and that failure, naming the visit, is
        // the proof it never reached the local-vault branch.
        let held = place_held_address(&[0x5b; 32], "field.knot");
        let visiting = engine
            .spawn(&SessionSpawnRequest::new(held.clone()))
            .err()
            .expect("no visit is open yet");
        assert!(
            matches!(&visiting, SessionError::SpawnFailed(reason)
                if reason.contains("no visit is open to the mere holding")),
            "a place-held address routes to the visiting path: {visiting:?}"
        );
        assert!(
            !format!("{visiting:?}").contains("no Knot vault is configured"),
            "visiting must not need a vault of one's own"
        );

        // And the engine claims the address, which is what keeps the shell from
        // handing it to host.external-protocol.
        assert_eq!(engine.engine_id(), ENGINE_ID);
        assert!(matches!(
            engine.spawn(&SessionSpawnRequest::new("https://example.test/page")),
            Err(SessionError::Unsupported(_))
        ));
    }

    /// A visit-only engine has nowhere to put a clip, whatever the environment
    /// names as the target.
    #[test]
    fn a_visit_only_engine_offers_no_clip_target() {
        let engine =
            KnotAuthoringEngine::visit_only(KnotVisits::default(), Some("field.knot".into()));
        assert!(engine.clip_handle().is_none());
    }

    #[test]
    fn only_a_personae_root_is_read_as_a_holder() {
        // A local vault, a short host, a non-hex host, and a document with no
        // path are all Knot addresses and none of them names a holder.
        for address in [
            "knot://vault/field",
            "knot://abcd/field.knot",
            &format!("knot://{}/field.knot", "zz".repeat(32)),
            &format!("knot://{}", "ab".repeat(32)),
            &format!("knot://{}/", "ab".repeat(32)),
            "file:///C:/vault/field.knot",
        ] {
            assert_eq!(
                parse_place_held_address(address),
                None,
                "{address} must not be read as place-held"
            );
        }
    }

    #[test]
    fn sharing_rewrites_a_locally_held_document_to_its_holder() {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(vault.join("notes")).unwrap();
        let document = vault.join("notes").join("field.knot");
        std::fs::write(&document, "# Field\n").unwrap();
        let root = [0x11; 32];

        let rewritten = place_held_rewrite(&file_address(&document), &vault, &root)
            .expect("a document inside the vault rewrites");
        assert_eq!(
            parse_place_held_address(&rewritten),
            Some((root, "notes/field.knot".to_string()))
        );
        // The resident vault form rewrites by the same rule.
        assert_eq!(
            place_held_rewrite("knot://vault/field-note", &vault, &root),
            Some(place_held_address(&root, "field-note"))
        );
        // Already place-held: left alone, so re-sharing cannot re-attribute
        // somebody else's document to whoever passed it along.
        assert_eq!(place_held_rewrite(&rewritten, &vault, &root), None);
        // Outside the vault: not this mere's to offer.
        let outside = temp.path().join("elsewhere.knot");
        std::fs::write(&outside, "# Elsewhere\n").unwrap();
        assert_eq!(place_held_rewrite(&file_address(&outside), &vault, &root), None);
    }

    #[test]
    fn a_place_held_address_matches_the_holder_own_disclosed_address() {
        let requested = place_held_address(&[0x22; 32], "notes/field.knot");
        // The holder discloses its own local path, which the visitor has no
        // way to know; the path inside the vault is what both ends agree on.
        assert!(address_matches(
            &requested,
            "file:///C:/someone-else/vault/notes/field.knot"
        ));
        assert!(!address_matches(
            &requested,
            "file:///C:/vault/other/field.knot"
        ));
        assert!(!address_matches(&requested, "notes/field.knot"));
        // An ordinary address is still matched exactly and nothing else.
        assert!(address_matches("knot://vault/one", "knot://vault/one"));
        assert!(!address_matches("knot://vault/one", "knot://vault/one-two"));
    }

    #[test]
    fn dom_ranges_only_name_an_observed_representation() {
        let dom_range = serde_json::json!({
            "type": "dom-range",
            "version": 1,
            "anchor": { "path": [0, 1], "offset": 0 },
            "focus": { "path": [0, 1], "offset": 17 },
            "quote": "A useful finding."
        })
        .to_string();
        let source = KnotClipArtifactV1 {
            role: KnotClipArtifactRoleV1::SourceResponse,
            media_type: "text/html".into(),
            canonical_uri: "https://example.test/report".into(),
            bytes: b"<p>A useful finding.</p>".to_vec(),
        };
        assert!(matches!(
            clip_selectors(Some(&dom_range), "A useful finding.", &[source]).as_slice(),
            [KnotClipSelectorV1::TextQuote {
                artifact_role: KnotClipArtifactRoleV1::SourceResponse,
                ..
            }]
        ));

        let observed = KnotClipArtifactV1 {
            role: KnotClipArtifactRoleV1::ObservedRepresentation,
            media_type: "application/vnd.mere.dom+json".into(),
            canonical_uri: "https://example.test/report".into(),
            bytes: br#"{"node":"p","text":"A useful finding."}"#.to_vec(),
        };
        assert!(matches!(
            clip_selectors(Some(&dom_range), "A useful finding.", &[observed]).as_slice(),
            [KnotClipSelectorV1::DomRange {
                artifact_role: KnotClipArtifactRoleV1::ObservedRepresentation,
                ..
            }]
        ));
    }

    #[test]
    #[ignore = "receipt: set KNOT_ENDPOINT_TEST_BIN to a built Mere knot_endpoint"]
    fn real_knot_consumer_saves_rejects_stale_and_reopens() {
        let program = std::env::var_os("KNOT_ENDPOINT_TEST_BIN")
            .expect("KNOT_ENDPOINT_TEST_BIN must name the real Knot endpoint");
        let root =
            std::env::temp_dir().join(format!("turnstone-knot-authoring-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("field.knot");
        fs::write(&path, "# Field\n").unwrap();
        let address = file_address(&path);

        {
            let engine =
                KnotAuthoringEngine::connect_directory(program.clone(), &root, 4096).unwrap();
            let request = SessionSpawnRequest::new(&address).with_viewport(900, 600);
            let mut first = engine.spawn(&request).unwrap();
            let mut second = engine.spawn(&request).unwrap();
            let first = first
                .as_any()
                .downcast_mut::<KnotDocumentSession>()
                .unwrap();
            let second = second
                .as_any()
                .downcast_mut::<KnotDocumentSession>()
                .unwrap();

            focus_editor(first);
            type_text(first, "discard");
            assert!(first.dispatch_key(KeyEvent::with_mods(
                Key::Character("z".into()),
                Modifiers {
                    ctrl: true,
                    ..Modifiers::default()
                },
            )));
            assert_eq!(first.runner.state().editor.source(), "# Field\n");
            type_text(first, "First ");
            let before_preedit = first.runner.state().editor.source().to_string();
            assert!(first.dispatch_key(KeyEvent::new(Key::Composition(
                CompositionEvent::Preedit {
                    text: "かな".into(),
                    selection: Some((3, 3)),
                },
            ))));
            assert_eq!(first.runner.state().editor.source(), before_preedit);
            assert!(
                first.dispatch_key(KeyEvent::new(Key::Composition(CompositionEvent::Commit(
                    "仮名".into()
                ),)))
            );

            focus_editor(second);
            type_text(second, "Second ");
            assert_eq!(second.status(), "unsaved");

            save_shortcut(first);
            wait_for_status(first, "saved");
            wait_for_status(second, "stale");
            assert_eq!(fs::read_to_string(&path).unwrap(), "# Field\nFirst 仮名");

            save_shortcut(second);
            wait_for_status(second, "stale");
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                "# Field\nFirst 仮名",
                "the stale local buffer must not overwrite the accepted save"
            );

            second.reload();
            wait_for_status(second, "saved");
            assert_eq!(second.runner.state().editor.source(), "# Field\nFirst 仮名");
            focus_editor(second);
            type_text(second, " after churn");
            let previous_refreshes = second.revision_refreshes;
            fs::write(root.join("other.knot"), "# Other\n").unwrap();
            wait_for_refresh(second, previous_refreshes);
            assert_eq!(
                second.status(),
                "unsaved",
                "an unrelated revision must retain the local buffer"
            );
            save_shortcut(second);
            wait_for_status(second, "saved");
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                "# Field\nFirst 仮名 after churn"
            );
        }

        {
            let engine = KnotAuthoringEngine::connect_directory(program, &root, 4096).unwrap();
            let request = SessionSpawnRequest::new(&address).with_viewport(900, 600);
            let mut reopened = engine.spawn(&request).unwrap();
            let reopened = reopened
                .as_any()
                .downcast_mut::<KnotDocumentSession>()
                .unwrap();
            assert_eq!(
                reopened.runner.state().editor.source(),
                "# Field\nFirst 仮名 after churn"
            );
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "receipt: set KNOT_ENDPOINT_TEST_BIN to a built Mere knot_endpoint"]
    fn inspector_clip_crosses_the_typed_endpoint_action() {
        let program = std::env::var_os("KNOT_ENDPOINT_TEST_BIN")
            .expect("KNOT_ENDPOINT_TEST_BIN must name the real Knot endpoint");
        let root =
            std::env::temp_dir().join(format!("turnstone-knot-clip-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("field.knot");
        fs::write(&path, "# Field\n").unwrap();
        let address = file_address(&path);

        let engine = KnotAuthoringEngine::connect_directory(program, &root, 4096).unwrap();
        let handle = KnotClipHandle {
            hub: engine.hub.clone().expect("a connected fixture has a local hub"),
            target: address,
            status: Arc::new(Mutex::new(KnotClipStatus::Ready)),
        };
        handle
            .insert(inker::DocumentClip {
                source_url: "https://example.test/report".into(),
                title: Some("The report".into()),
                text: "A useful finding.".into(),
                selector: None,
                links: vec!["https://example.test/source".into()],
                artifacts: Vec::new(),
            })
            .unwrap();
        wait_for_clip_status(&handle, KnotClipStatus::Saved);

        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("```knot.clip.provenance"));
        assert!(saved.contains(r#""source_url":"https://example.test/report""#));
        assert!(saved.contains("# The report"));
        assert!(saved.contains("A useful finding."));
        assert!(saved.contains("https://example.test/source"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "receipt: set KNOT_ENDPOINT_TEST_BIN to a built Mere knot_endpoint"]
    fn inspector_clip_retains_livery_source_evidence_end_to_end() {
        let program = std::env::var_os("KNOT_ENDPOINT_TEST_BIN")
            .expect("KNOT_ENDPOINT_TEST_BIN must name the real Knot endpoint");
        let root =
            std::env::temp_dir().join(format!("turnstone-knot-clip-v2-{}", uuid::Uuid::new_v4()));
        let evidence =
            std::env::temp_dir().join(format!("turnstone-knot-evidence-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        fs::create_dir(&evidence).unwrap();
        let path = root.join("field.djot");
        fs::write(&path, "# Field\n").unwrap();
        let address = file_address(&path);
        let html = "<article><h1>The report</h1><p>A useful finding.</p>\
                    <a href=\"https://example.test/source\">Source</a></article>";
        let source_engine =
            genet_documents::LiverySessionEngine::new(genet_documents::LocalFetcher);
        let source_request =
            SessionSpawnRequest::new("https://example.test/report").with_body(html);
        let source_clip = source_engine
            .spawn(&source_request)
            .unwrap()
            .clip()
            .expect("the Livery session should lower its document");
        let bytes = html.as_bytes().to_vec();
        let digest = blake3::hash(&bytes).to_hex().to_string();

        let engine =
            KnotAuthoringEngine::connect_directory_evidence(program, &root, 4096, &evidence, 4096)
                .unwrap();
        let handle = KnotClipHandle {
            hub: engine.hub.clone().expect("a connected fixture has a local hub"),
            target: address,
            status: Arc::new(Mutex::new(KnotClipStatus::Ready)),
        };
        handle.insert(source_clip).unwrap();
        wait_for_clip_status(&handle, KnotClipStatus::Saved);

        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains(r#""schema":"knot.clip.insert/v2""#));
        assert!(saved.contains(&chirograph::Sha256NamedInformation::of(&bytes).to_string()));
        assert!(saved.contains(&format!("blake3:{digest}")));
        assert!(!saved.contains("urn:blake3:"));
        assert!(!saved.contains("<article>"));

        drop(handle);
        drop(engine);
        std::thread::sleep(POLL_INTERVAL * 2);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let blobs = transport::BlobStore::open(&evidence).await.unwrap();
            let hash = transport::BlobHash::from_bytes(*blake3::hash(&bytes).as_bytes());
            assert_eq!(
                blobs.get_bytes(hash).await.unwrap().as_ref(),
                bytes.as_slice()
            );
            blobs.shutdown().await.unwrap();
        });
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(evidence).unwrap();
    }

    #[test]
    #[ignore = "receipt: set KNOT_ENDPOINT_TEST_BIN to a built Mere knot_endpoint"]
    fn resolve_and_run_cross_the_consented_authoring_path() {
        let program = std::env::var_os("KNOT_ENDPOINT_TEST_BIN")
            .expect("KNOT_ENDPOINT_TEST_BIN must name the real Knot endpoint");
        let root =
            std::env::temp_dir().join(format!("turnstone-knot-effects-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let included = root.join("included.md");
        fs::write(&included, "## Included\n\nFetched text.\n").unwrap();
        let path = root.join("field.knot");
        let authored = format!(
            "# Field\n\n```include {}\nFallback.\n```\n\n```rhai eval\n40 + 2\n```\n",
            file_address(&included)
        );
        fs::write(&path, &authored).unwrap();
        let address = file_address(&path);

        {
            let engine = KnotAuthoringEngine::connect_directory_effects(
                program.clone(),
                &root,
                4096,
                "ask",
                "ask",
            )
            .unwrap();
            let request = SessionSpawnRequest::new(&address).with_viewport(900, 600);
            let mut session = engine.spawn(&request).unwrap();
            let session = session
                .as_any()
                .downcast_mut::<KnotDocumentSession>()
                .unwrap();
            assert!(session.runner.state().resolve_available);
            assert!(session.runner.state().run_available);

            session.invoke_effect(KnotEffectKind::Resolve);
            let resolved = wait_for_derived(session, "resolved 1");
            assert!(resolved.source.contains("Included"));
            assert!(resolved.source.contains("rhai eval"));
            assert_eq!(fs::read_to_string(&path).unwrap(), authored);

            session.invoke_effect(KnotEffectKind::Run);
            let ran = wait_for_derived(session, "ran 1");
            assert!(ran.source.contains("Included"));
            assert!(ran.source.contains("42"));
            assert_eq!(fs::read_to_string(&path).unwrap(), authored);

            focus_editor(session);
            type_text(session, "local edit");
            assert!(
                session.runner.state().derived.is_none(),
                "an edit must drop a derived preview tied to older source"
            );
        }

        {
            let engine = KnotAuthoringEngine::connect_directory_effects(
                program, &root, 4096, "auto", "auto",
            )
            .unwrap();
            let request = SessionSpawnRequest::new(&address).with_viewport(900, 600);
            let mut session = engine.spawn(&request).unwrap();
            let session = session
                .as_any()
                .downcast_mut::<KnotDocumentSession>()
                .unwrap();
            let ran = wait_for_derived(session, "ran 1");
            assert!(ran.source.contains("Included"));
            assert!(ran.source.contains("42"));
            assert_eq!(fs::read_to_string(&path).unwrap(), authored);
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "receipt: set KNOT_ENDPOINT_TEST_BIN to a built Mere knot_endpoint"]
    fn received_communal_auto_run_waits_for_explicit_confirmation() {
        let program = std::env::var_os("KNOT_ENDPOINT_TEST_BIN")
            .expect("KNOT_ENDPOINT_TEST_BIN must name the real Knot endpoint");
        let root =
            std::env::temp_dir().join(format!("turnstone-knot-commons-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let authored = "\
# Received calculation

```rhai eval
40 + 2
```
";

        {
            let engine =
                KnotAuthoringEngine::connect_communal_fixture_effects(program, &root, 4096, "auto")
                    .unwrap();
            let request = SessionSpawnRequest::new("knot://vault/received").with_viewport(900, 600);
            let mut session = engine.spawn(&request).unwrap();
            let session = session
                .as_any()
                .downcast_mut::<KnotDocumentSession>()
                .unwrap();
            assert!(session.runner.state().run_available);
            assert_eq!(session.runner.state().editor.source(), authored);

            wait_for_status(session, "rejected");
            assert_eq!(
                session.runner.state().detail,
                "received Commons documents require explicit effect confirmation"
            );
            assert!(session.runner.state().derived.is_none());

            session.invoke_effect(KnotEffectKind::Run);
            let ran = wait_for_derived(session, "ran 1");
            assert!(ran.source.contains("42"));
            assert_eq!(session.runner.state().editor.source(), authored);
        }

        std::thread::sleep(Duration::from_millis(100));
        assert!(
            !tree_contains(&root, authored.as_bytes()),
            "received source must remain sealed in the process fixture root"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
