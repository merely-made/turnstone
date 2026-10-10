// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's Graphshell endpoint.
//!
//! The adapter reads Turnstone's live graph, asks Mere cartography to disclose a
//! product-free score and scene, and keeps all rendered card data beside that
//! scene. Incoming intents return through Turnstone's ordinary [`Action`] spine
//! only after Servitor has evaluated the endpoint's projected grant.
//!
//! Graph coverage is local context beside the disclosed scene. The current
//! Chirograph V1 snapshot, FrozenScene and accessibility scene outputs do not
//! report that context; they must not be described as coverage-aware outputs.

use std::collections::{BTreeMap, HashMap};

use chartulary::{Container, EditSpec, GraphLog, Relation};
use chirograph::{
    AdvertisedAction, BoundsRelationship, CachePolicy, CapabilityProfile, CardValueV1, ContentHash,
    EndpointDescriptor, IntentEffect, IntentInvocation, IntentReference, IntentResult,
    NativeGlyphV1, PortableCardV1, PresentationBinding, PresentationCapability, PresentationCodec,
    PresentationKey, PresentationManifest, PresentationOffer, PresentationSemantics,
    ProjectionOffer, ProjectionRequest, ProjectionSession, ProjectionSnapshot, ProtocolVersion,
    ResourceRequest, ResourceResponse, SemanticRole,
};
use graphshell_client::{
    ClientState, PresentationResolution, ResolutionError, ResolvedPresentation,
};
use graphshell_endpoint::{IntentSink, PresentationSource, ProjectionCatalog, ProjectionSource};
use identity::IdentityProvider;
use mere::kernel::graph::{Author, CoverageNote, NodeKey};
use sceno::{Arrangement, Score, Spiral};
use scenotime::{Revision, SceneEpoch, SceneSnapshot};
use servitor::delegation::{DelegationTable, root_certificate};
use servitor::{Cap, Gate, Grant, Mode, ScopePath, Subject};

use crate::action::Action;
use crate::app::App;

const SESSION: &str = "loopback:turnstone:g3";
const LAYOUT_SCOPE: &str = "projection/layout/";
const GRAPH_SCOPE: &str = "graph/open/";

/// The endpoint's capabilities as parsed scopes. Both are places in the audit
/// graph with unbounded interiors, so they are scopes rather than powers
/// (capability-model round, 2026-07-23).
fn layout_scope() -> ScopePath {
    ScopePath::parse(LAYOUT_SCOPE).expect("a valid scope")
}

fn graph_scope() -> ScopePath {
    ScopePath::parse(GRAPH_SCOPE).expect("a valid scope")
}
type EndpointAuthorRestore = crate::host_journal::AuthorRestore;


const FIT_INTENT: &str = "turnstone.fit-view";

/// The catalogue's label for fitting the view.
fn fit_label() -> &'static str {
    cambium::catalogue::label(cambium::catalogue::ids::VIEW_FIT).unwrap_or("Fit to view")
}
const OPEN_INTENT: &str = "turnstone.open-address";

/// Typed refusals at the projection boundary. Version mismatch is checked
/// before scene/resource state is changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectionRefusal {
    WrongSession,
    UnsupportedProtocol,
    UnsupportedScore { received: u16, supported: u16 },
    UnsupportedArrangement,
    InvalidScene(String),
    Presentation(String),
}

impl std::fmt::Display for ProjectionRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongSession => f.write_str("projection request names the wrong session"),
            Self::UnsupportedProtocol => {
                f.write_str("projection request uses an unsupported protocol")
            }
            Self::UnsupportedScore {
                received,
                supported,
            } => write!(
                f,
                "projection score version {received} is unsupported; expected {supported}"
            ),
            Self::UnsupportedArrangement => {
                f.write_str("G3 Turnstone endpoint currently accepts Spiral arrangements")
            }
            Self::InvalidScene(reason) => write!(f, "invalid disclosed scene: {reason}"),
            Self::Presentation(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for ProjectionRefusal {}

/// The product endpoint. Its default card extent can be replaced by the host
/// once a real retained card has measured itself.
pub struct TurnstoneEndpoint {
    app: App,
    session: ProjectionSession,
    card_extent: (f32, f32),
    snapshot: Option<ProjectionSnapshot>,
    snapshot_coverage: Option<CoverageNote>,
    resources: BTreeMap<ContentHash, Vec<u8>>,
    gate: Gate,
    authority: DelegationTable,
    audit: GraphLog<Container, Relation>,
    subject: Subject,
}

impl TurnstoneEndpoint {
    pub fn new(app: App) -> Result<Self, String> {
        Self::with_card_extent(app, (248.0, 168.0))
    }

    /// Deterministic live graph used by the cross-process projection receipt.
    pub fn fixture() -> Result<Self, String> {
        Self::new(App::projection_fixture())
    }

    pub fn with_card_extent(app: App, card_extent: (f32, f32)) -> Result<Self, String> {
        let session = ProjectionSession(SESSION.into());
        // The endpoint's identity is DERIVED from the profile identity, not
        // hashed out of its own name. Before the capability-model audit the
        // subject was `blake3(session name)` — not a key at all, so nothing
        // could ever prove it was this endpoint — and the endpoint then
        // granted ITSELF the layout capability. A refusal there stated only
        // "the endpoint did not grant itself", which is not an authority
        // statement. Now: a per-session keypair derived from the user's
        // master key, holding a capability the USER delegated to it.
        // The endpoint speaks for the profile, so it waits for the vault to
        // unlock like everything else that does.
        let root = app
            .identity
            .clone()
            .ok_or_else(|| crate::denizen::PENDING_IDENTITY.to_string())?;
        let salt = format!("turnstone/projection-endpoint/{}", session.0);
        let endpoint_key = root
            .derive_keypair(salt.as_bytes())
            .map_err(|error| format!("failed to derive the endpoint identity: {error:?}"))?;
        let subject = Subject::new(endpoint_key.public_key().to_bytes());
        let gate = Gate::new();
        let mut audit = GraphLog::new();
        let layout = Cap::Scope(layout_scope());
        gate.project_grant(
            &mut audit,
            &Grant::new(subject, layout.clone(), Mode::Write),
        )
        .map_err(|error| format!("failed to project endpoint grant: {error:?}"))?;

        // The AUTHORITY is a signed delegation from the profile identity: the
        // user lets this endpoint present layout for this session, and
        // nothing else. The projection above stays the browsable audit record
        // of the same fact. Depth 0 — the endpoint may act, never delegate
        // onward; that bumps to 1 when a remote viewer with its own key needs
        // a narrower capability from it (the sub-delegation consumer named in
        // the capability plan's follow-ons).
        //
        // One clock read: issuing at a later instant than the table's own
        // `now` would leave `not_before` in its future and the endpoint
        // authorized for nothing.
        let now = crate::denizen::now_ms();
        let certificate = root_certificate(
            IdentityProvider::master_public_key(root.as_ref()).to_bytes(),
            subject,
            &layout,
            Mode::Write,
            format!("projection:{}", session.0).into_bytes(),
            now,
            None,
            0,
            *blake3::hash(salt.as_bytes()).as_bytes(),
        );
        // Signed inside djinn (D11): only the certificate crosses.
        let signed = root
            .issue_certificate(certificate)
            .map_err(|error| format!("failed to sign the endpoint delegation: {error:?}"))?;
        let mut authority = DelegationTable::new(
            IdentityProvider::master_public_key(root.as_ref()).to_bytes(),
        );
        authority.adopt(signed);
        authority.set_now(now);

        Ok(Self {
            app,
            session,
            card_extent,
            snapshot: None,
            snapshot_coverage: None,
            resources: BTreeMap::new(),
            gate,
            authority,
            audit,
            subject,
        })
    }

    pub fn session(&self) -> &ProjectionSession {
        &self.session
    }

    pub fn app(&self) -> &App {
        &self.app
    }

    /// Known graph limits beside the last successful local snapshot.
    ///
    /// None means no snapshot has been installed. An empty note means no known
    /// limit in that supplied graph, never world completeness. Chirograph V1
    /// does not carry this note to remote clients.
    pub fn snapshot_coverage(&self) -> Option<&CoverageNote> {
        self.snapshot_coverage.as_ref()
    }

    pub fn audit(&self) -> &GraphLog<Container, Relation> {
        &self.audit
    }

    fn actions() -> Vec<AdvertisedAction> {
        vec![
            AdvertisedAction {
                intent: IntentReference(FIT_INTENT.into()),
                // The stack's label for the verb (SE49), so the advertised
                // card and Turnstone's own palette say the same thing.
                label: fit_label().into(),
                explanation: "Frame the disclosed Turnstone graph without changing it.".into(),
                payload_schema: r#"{"type":"null"}"#.into(),
                input_form: None,
                effect: IntentEffect::Curation,
            },
            AdvertisedAction {
                intent: IntentReference(OPEN_INTENT.into()),
                label: "Open address".into(),
                explanation: "Ask Turnstone to add or select an address in graph truth.".into(),
                payload_schema: r#"{"type":"string","format":"uri"}"#.into(),
                input_form: None,
                effect: IntentEffect::DomainTruth,
            },
        ]
    }

    fn presentation(&mut self, scene: &SceneSnapshot) -> Result<PresentationManifest, String> {
        let mut manifest = PresentationManifest::default();
        let graph = self.app.graph_runtimes.graph();
        for (instance, item) in scene.active_items_in_order() {
            let source = scene
                .tables
                .sources
                .get(item.source.0 as usize)
                .and_then(Option::as_ref)
                .ok_or_else(|| format!("scene item {} has no source", instance.0))?;
            let id = uuid::Uuid::parse_str(&source.id)
                .map_err(|error| format!("invalid Mere node id {}: {error}", source.id))?;
            let (key, node) = graph
                .get_node_by_id(id)
                .ok_or_else(|| format!("Mere node {id} vanished during disclosure"))?;
            let label = graph.node_display_label(key);
            let semantics = PresentationSemantics {
                label: label.clone(),
                role: SemanticRole::Article,
                bounds: BoundsRelationship::FillFootprint,
                actions: Self::actions(),
            };
            let card = PortableCardV1 {
                title: label.clone(),
                values: vec![
                    CardValueV1 {
                        label: "Address".into(),
                        value: node.url().to_string(),
                    },
                    CardValueV1 {
                        label: "Source".into(),
                        value: "Turnstone graph".into(),
                    },
                ],
                badges: vec!["turnstone".into(), "granted projection".into()],
                media: Vec::new(),
            };
            let glyph = NativeGlyphV1 {
                label,
                icon: Some("◇".into()),
                color: Some("#d8a657".into()),
            };
            let card_bytes = serde_json::to_vec(&card)
                .map_err(|error| format!("could not encode card: {error}"))?;
            let glyph_bytes = serde_json::to_vec(&glyph)
                .map_err(|error| format!("could not encode glyph: {error}"))?;
            let card_hash = ContentHash::of(&card_bytes);
            let glyph_hash = ContentHash::of(&glyph_bytes);
            self.resources.insert(card_hash, card_bytes.clone());
            self.resources.insert(glyph_hash, glyph_bytes.clone());
            let key = PresentationKey(source.id.clone());
            manifest.bindings.push(PresentationBinding {
                instance,
                key: key.clone(),
            });
            manifest.offers.insert(
                key,
                vec![
                    PresentationOffer {
                        codec: PresentationCodec::PortableCardV1,
                        resource: card_hash,
                        byte_size: card_bytes.len() as u64,
                        requires: PresentationCapability::PortableCard,
                        semantics: semantics.clone(),
                    },
                    PresentationOffer {
                        codec: PresentationCodec::NativeGlyphV1,
                        resource: glyph_hash,
                        byte_size: glyph_bytes.len() as u64,
                        requires: PresentationCapability::NativeGlyph,
                        semantics,
                    },
                ],
            );
        }
        Ok(manifest)
    }

    fn active_revision(&self) -> Option<(SceneEpoch, Revision)> {
        self.snapshot
            .as_ref()
            .map(|snapshot| (snapshot.scene.epoch, snapshot.scene.revision))
    }

    fn intent_was_advertised(&self, intent: &IntentInvocation) -> bool {
        self.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.scene.active_item(intent.target).is_some()
                && snapshot
                    .presentation
                    .offers_for(intent.target)
                    .is_some_and(|offers| {
                        offers.iter().any(|offer| {
                            offer
                                .semantics
                                .actions
                                .iter()
                                .any(|action| action.intent.0 == intent.intent)
                        })
                    })
        })
    }

    fn petition(&mut self, scope: &ScopePath, node: Container) -> Result<(), servitor::GateError> {
        let revision = self.audit.revision();
        self.gate.petition(
            &self.authority,
            &mut self.audit,
            self.subject,
            scope,
            revision,
            vec![EditSpec::InsertNode(node)],
        )?;
        Ok(())
    }
}

/// Build the scene Turnstone discloses: the live graph projected through the
/// spiral score, solved, and carried into routed relations.
///
/// Extracted from the endpoint's `snapshot` so the frozen-projection pane can
/// freeze exactly what a remote peer would be served, rather than a second
/// recipe that drifts from the first.
pub(crate) fn disclose_scene(
    graph: &mere::kernel::graph::Graph,
    focused: Option<NodeKey>,
    card_extent: (f32, f32),
    spiral: sceno::Spiral,
) -> cartography::CoveredScene {
    let extents: HashMap<NodeKey, (f32, f32)> =
        graph.nodes().map(|(key, _)| (key, card_extent)).collect();
    // The spiral reads keyed signals, most recent first (Mere F132). A
    // disclosure is a one-off projection with no canvas behind it, so it
    // takes a fresh registry for this graph.
    let signals = mere::canvas::ChannelRegistry::new().disclose(
        graph,
        &[cartography::ORDER_RECENCY, cartography::WEIGHT_RECENCY],
        focused,
    );
    let mut mapped = cartography::project_spiral_score(
        graph,
        &signals,
        cartography::ORDER_RECENCY,
        Some(&extents),
        focused,
    );
    mapped.score.arrangement = Arrangement::Spiral(spiral);
    let solved = scenomise::solve(&mapped.score);
    let cartography::CoveredScene { mut scene, coverage } = cartography::scene_from_projection(
        &mapped.projection,
        |key| {
            graph
                .get_node(key)
                .expect("projection key came from the live graph")
                .id
                .to_string()
        },
        |key| extents.get(&key).copied(),
    );
    for ((item, score_item), solved_item) in scene
        .items
        .iter_mut()
        .zip(mapped.score.items.iter())
        .zip(solved.items.iter())
    {
        item.transform = solved_item.transform;
        item.representation = score_item.representation.clone();
        item.layer = score_item.layer;
        item.visible = score_item.visible;
    }
    for relation in &mut scene.relations {
        let from = scene.items[relation.from.0 as usize].transform.translate;
        let to = scene.items[relation.to.0 as usize].transform.translate;
        relation.points = vec![from, to];
    }
    scene.bounds = solved.bounds;
    scene.generation = mapped.score.generation;
    cartography::CoveredScene { scene, coverage }
}

impl ProjectionSource for TurnstoneEndpoint {
    type Error = ProjectionRefusal;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
        if request.session != self.session {
            return Err(ProjectionRefusal::WrongSession);
        }
        if request.version.major != ProtocolVersion::V1.major {
            return Err(ProjectionRefusal::UnsupportedProtocol);
        }
        if request.score.version != sceno::SCORE_VERSION {
            return Err(ProjectionRefusal::UnsupportedScore {
                received: request.score.version,
                supported: sceno::SCORE_VERSION,
            });
        }
        let Arrangement::Spiral(spiral) = request.score.arrangement else {
            return Err(ProjectionRefusal::UnsupportedArrangement);
        };

        let graph = self.app.graph_runtimes.graph();
        let cartography::CoveredScene { scene, coverage } = disclose_scene(
            graph,
            self.app.graph_runtimes.focused_key(),
            self.card_extent,
            spiral,
        );

        let revision = Revision(graph.revision().max(1));
        let scene = SceneSnapshot::from_dense(SceneEpoch(1), revision, scene)
            .map_err(|error| ProjectionRefusal::InvalidScene(format!("{error:?}")))?;
        self.resources.clear();
        let presentation = self
            .presentation(&scene)
            .map_err(ProjectionRefusal::Presentation)?;
        let snapshot = ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: self.session.clone(),
            scene,
            presentation,
            cache_policy: CachePolicy::default(),
        };
        self.snapshot = Some(snapshot.clone());
        self.snapshot_coverage = Some(coverage);
        Ok(snapshot)
    }
}

impl ProjectionCatalog for TurnstoneEndpoint {
    fn describe(&self) -> EndpointDescriptor {
        EndpointDescriptor {
            label: "Turnstone".into(),
            projections: vec![ProjectionOffer {
                label: "Browsing graph".into(),
                request: ProjectionRequest {
                    version: ProtocolVersion::V1,
                    session: self.session.clone(),
                    score: Score::new(Arrangement::Spiral(Spiral::default())),
                },
            }],
        }
    }
}

impl PresentationSource for TurnstoneEndpoint {
    type Error = String;

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
        if request.session != self.session {
            return Err("resource request names the wrong session".into());
        }
        let bytes = self
            .resources
            .get(&request.resource)
            .cloned()
            .ok_or_else(|| "resource was not disclosed by this session".to_string())?;
        Ok(ResourceResponse {
            session: self.session.clone(),
            resource: request.resource,
            bytes,
        })
    }
}

impl IntentSink for TurnstoneEndpoint {
    type Error = String;

    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, Self::Error> {
        if intent.session != self.session {
            return Err("intent names the wrong session".into());
        }
        let Some((epoch, revision)) = self.active_revision() else {
            return Err("intent arrived before a snapshot".into());
        };
        if intent.observed_epoch != epoch || intent.observed_revision != revision {
            return Ok(IntentResult::Stale {
                current_epoch: epoch,
                current_revision: revision,
            });
        }
        if !self.intent_was_advertised(&intent) {
            return Ok(IntentResult::Rejected {
                reason: "target or intent was not disclosed by this snapshot".into(),
            });
        }

        match intent.intent.as_str() {
            FIT_INTENT => {
                if !intent.payload.is_empty() {
                    return Ok(IntentResult::Rejected {
                        reason: "fit-view accepts an empty payload".into(),
                    });
                }
                let audit = Container::new(format!("{LAYOUT_SCOPE}fit-{}", self.audit.revision()))
                    .with_title("Fit disclosed Turnstone graph");
                match self.petition(&layout_scope(), audit) {
                    Ok(()) => {
                        self.app.update(Action::FitView);
                        Ok(IntentResult::Accepted)
                    }
                    Err(error) => Ok(IntentResult::Rejected {
                        reason: format!("authority gate refused fit-view: {error:?}"),
                    }),
                }
            }
            OPEN_INTENT => {
                let Ok(address) = String::from_utf8(intent.payload) else {
                    return Ok(IntentResult::Rejected {
                        reason: "open-address payload is not UTF-8".into(),
                    });
                };
                if address.trim().is_empty() {
                    return Ok(IntentResult::Rejected {
                        reason: "open-address requires an address".into(),
                    });
                }
                let audit = Container::new(format!("{GRAPH_SCOPE}open-{}", self.audit.revision()))
                    .with_title(format!("Open {address}"));
                match self.petition(&graph_scope(), audit) {
                    Ok(()) => {
                        let _restore_author = EndpointAuthorRestore::enter(
                            self.app.journal.clone(),
                            Author::engine(self.subject.to_hex(), env!("CARGO_PKG_VERSION"))
                                .via("turnstone"),
                        );
                        self.app.update(Action::OpenAddress(address));
                        Ok(IntentResult::Accepted)
                    }
                    Err(error) => Ok(IntentResult::Rejected {
                        reason: format!("authority gate refused graph change: {error:?}"),
                    }),
                }
            }
            _ => Ok(IntentResult::Rejected {
                reason: "unknown endpoint intent".into(),
            }),
        }
    }
}

/// Material returned by the executable G3 canary and its receipt tests.
pub struct G3Run {
    pub session: ProjectionSession,
    pub presentations: Vec<ResolvedPresentation>,
    pub layout: graphshell::view::ProjectionLayoutView,
    pub fit_result: IntentResult,
    pub open_result: IntentResult,
    pub graph_revision_before: u64,
    pub graph_revision_after: u64,
    pub graph_nodes_before: usize,
    pub graph_nodes_after: usize,
    pub audit_revision: u64,
    pub audit_author: String,
}

pub fn run_g3_canary() -> Result<G3Run, String> {
    let mut endpoint = TurnstoneEndpoint::fixture()?;
    let session = endpoint.session().clone();
    let request = ProjectionRequest {
        version: ProtocolVersion::V1,
        session: session.clone(),
        score: Score::new(Arrangement::Spiral(Spiral::default())),
    };
    let snapshot = endpoint
        .snapshot(request)
        .map_err(|error| error.to_string())?;
    let layout = graphshell::view::ProjectionLayoutView::from_scene(&snapshot.scene);
    let graph_revision_before = endpoint.app().graph_runtimes.graph().revision();
    let graph_nodes_before = endpoint.app().graph_runtimes.graph().nodes().count();
    let mut client = ClientState::default();
    client
        .apply_snapshot(snapshot)
        .map_err(|error| format!("client rejected Turnstone snapshot: {error:?}"))?;
    let profile = CapabilityProfile::new([
        PresentationCapability::PortableCard,
        PresentationCapability::NativeGlyph,
    ]);
    let presentations = resolve_all(&mut endpoint, &mut client, &session, &profile)?;
    let ack = client
        .acknowledgement(&session)
        .ok_or_else(|| "client did not acknowledge the Turnstone snapshot".to_string())?;
    let target = client
        .mounted(&session)
        .and_then(|mounted| {
            mounted
                .scene
                .active_items_in_order()
                .first()
                .map(|(id, _)| *id)
        })
        .ok_or_else(|| "Turnstone snapshot disclosed no target".to_string())?;
    let fit_result = endpoint.invoke(IntentInvocation {
        session: session.clone(),
        target,
        observed_epoch: ack.epoch,
        observed_revision: ack.revision,
        intent: FIT_INTENT.into(),
        payload: Vec::new(),
    })?;
    let open_result = endpoint.invoke(IntentInvocation {
        session: session.clone(),
        target,
        observed_epoch: ack.epoch,
        observed_revision: ack.revision,
        intent: OPEN_INTENT.into(),
        payload: b"mere://graphshell/rejected".to_vec(),
    })?;
    let graph_revision_after = endpoint.app().graph_runtimes.graph().revision();
    let graph_nodes_after = endpoint.app().graph_runtimes.graph().nodes().count();
    let audit_revision = endpoint.audit().revision();
    let audit_author = endpoint
        .audit()
        .log()
        .entries()
        .last()
        .map(|entry| entry.author.as_str().to_string())
        .unwrap_or_default();

    Ok(G3Run {
        session,
        presentations,
        layout,
        fit_result,
        open_result,
        graph_revision_before,
        graph_revision_after,
        graph_nodes_before,
        graph_nodes_after,
        audit_revision,
        audit_author,
    })
}

fn resolve_all(
    endpoint: &mut TurnstoneEndpoint,
    client: &mut ClientState,
    session: &ProjectionSession,
    profile: &CapabilityProfile,
) -> Result<Vec<ResolvedPresentation>, String> {
    let instances: Vec<_> = client
        .mounted(session)
        .ok_or_else(|| "client did not mount the Turnstone projection".to_string())?
        .scene
        .active_items_in_order()
        .into_iter()
        .map(|(instance, _)| instance)
        .collect();
    let mut resolved = Vec::new();
    for instance in instances {
        let presentation = match client
            .resolve(session, instance, profile)
            .map_err(resolution_error)?
        {
            PresentationResolution::Ready(presentation) => presentation,
            PresentationResolution::NeedsResource(request) => {
                let response = endpoint.resource(request)?;
                client
                    .apply_resource(response)
                    .map_err(|error| format!("client refused Turnstone resource: {error:?}"))?;
                match client
                    .resolve(session, instance, profile)
                    .map_err(resolution_error)?
                {
                    PresentationResolution::Ready(presentation) => presentation,
                    PresentationResolution::NeedsResource(_) => {
                        return Err("Turnstone resource remained unresolved after transfer".into());
                    }
                }
            }
        };
        resolved.push(presentation);
    }
    Ok(resolved)
}

fn resolution_error(error: ResolutionError) -> String {
    format!("could not resolve Turnstone presentation: {error:?}")
}

pub fn render_g3_receipt() -> Result<String, String> {
    let run = run_g3_canary()?;
    let result_name = |result: &IntentResult| match result {
        IntentResult::Accepted => "Accepted".to_string(),
        IntentResult::Rejected { .. } => "Rejected".to_string(),
        IntentResult::Stale { .. } => "Stale".to_string(),
    };
    let result_detail = |result: &IntentResult, accepted: &str| match result {
        IntentResult::Accepted => accepted.to_string(),
        IntentResult::Rejected { reason } => reason.clone(),
        IntentResult::Stale { .. } => "The client observation was stale.".into(),
    };
    Ok(graphshell::view::render_projection_receipt(
        &graphshell::view::ProjectionReceiptView {
            eyebrow: "Graphshell · G3 receipt".into(),
            title: "Turnstone truth, projected.".into(),
            lede: "Mere cartography maps the live browser graph into one Scenograph scene. Graphshell resolves the endpoint-owned cards and returns both intents through the same Servitor authority gate.".into(),
            session: run.session.0,
            status: format!(
                "Live · {} nodes · graph revision {}",
                run.graph_nodes_after, run.graph_revision_after
            ),
            presentations: run.presentations,
            layout: Some(run.layout),
            intents: vec![
                graphshell::view::IntentReceiptView {
                    label: format!("{} · curation", fit_label()),
                    result: result_name(&run.fit_result),
                    detail: result_detail(
                        &run.fit_result,
                        "The projected layout grant admitted the harmless view action.",
                    ),
                },
                graphshell::view::IntentReceiptView {
                    label: "Open address · graph truth".into(),
                    result: result_name(&run.open_result),
                    detail: result_detail(
                        &run.open_result,
                        "The graph-changing action was admitted and lowered through App::update.",
                    ),
                },
            ],
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mere::kernel::graph::{CoverageLayer, CoverageLimit, CoverageNote};

    fn scoped_coverage() -> CoverageNote {
        let layers = [
            CoverageLayer::Possession,
            CoverageLayer::Residency,
            CoverageLayer::Disclosure,
            CoverageLayer::Synchronization,
            CoverageLayer::Projection,
        ];
        CoverageNote {
            limits: layers
                .into_iter()
                .enumerate()
                .map(|(index, layer)| CoverageLimit {
                    layer,
                    reason: format!("known limit {index}"),
                    resources: vec![uuid::Uuid::from_u128(100 + index as u128)],
                    surfaces: vec![uuid::Uuid::from_u128(200 + index as u128)],
                    count: if index % 2 == 0 { None } else { Some(index) },
                })
                .collect(),
        }
    }

    fn set_coverage(app: &mut App, note: CoverageNote) {
        let mut context = app.graph_runtimes.graph().clone();
        context.set_known_coverage(note);
        app.graph_runtimes
            .active_canvas_mut()
            .refresh_semantic_context(&context);
    }

    #[test]
    fn disclosed_scene_retains_every_scoped_coverage_layer() {
        let mut app = App::projection_fixture();
        let known = scoped_coverage();
        set_coverage(&mut app, known.clone());
        let disclosed = disclose_scene(
            app.graph_runtimes.graph(),
            app.graph_runtimes.focused_key(),
            (248.0, 168.0),
            Spiral::default(),
        );
        for limit in &known.limits {
            assert!(disclosed.coverage.limits.contains(limit), "lost {limit:?}");
        }
        assert_eq!(disclosed.scene.items.len(), 3);
        assert_eq!(disclosed.scene.relations.len(), 2);
    }

    #[test]
    fn coverage_survives_disclosure_and_snapshot_refusal() {
        let mut app = App::projection_fixture();
        let known = scoped_coverage();
        set_coverage(&mut app, known.clone());
        let mut endpoint = TurnstoneEndpoint::new(app).unwrap();
        assert!(endpoint.snapshot_coverage().is_none());
        let request = ProjectionRequest {
            version: ProtocolVersion::V1,
            session: endpoint.session().clone(),
            score: Score::new(Arrangement::Spiral(Spiral::default())),
        };
        let first = endpoint.snapshot(request.clone()).unwrap();
        for limit in &known.limits {
            assert!(endpoint.snapshot_coverage().unwrap().limits.contains(limit));
        }
        assert_eq!(first.scene.active_item_count(), 3);
        assert_eq!(first.presentation.bindings.len(), 3);

        let truth_revision = endpoint.app.graph_runtimes.graph().revision();
        let mut changed = known.clone();
        changed.limits[0].count = Some(7);
        changed.limits[1].resources = vec![uuid::Uuid::from_u128(999)];
        set_coverage(&mut endpoint.app, changed.clone());
        assert_eq!(endpoint.app.graph_runtimes.graph().revision(), truth_revision);
        let second = endpoint.snapshot(request.clone()).unwrap();
        assert_eq!(second.scene.epoch, first.scene.epoch);
        assert_eq!(second.scene.revision, first.scene.revision);
        for limit in &changed.limits {
            assert!(endpoint.snapshot_coverage().unwrap().limits.contains(limit));
        }
        assert!(!endpoint.snapshot_coverage().unwrap().limits.contains(&known.limits[0]));
        assert!(!endpoint.snapshot_coverage().unwrap().limits.contains(&known.limits[1]));

        let prior_coverage = endpoint.snapshot_coverage().unwrap().clone();
        let prior_resources = endpoint.resources.clone();
        let mut legacy = request;
        legacy.score.version = 4;
        assert!(matches!(
            endpoint.snapshot(legacy),
            Err(ProjectionRefusal::UnsupportedScore { received: 4, supported: 5 })
        ));
        assert_eq!(endpoint.snapshot_coverage(), Some(&prior_coverage));
        assert_eq!(endpoint.snapshot.as_ref(), Some(&second));
        assert_eq!(endpoint.resources, prior_resources);

        // V1 carries scene/presentation only. Local retention is not a claim
        // that remote clients received these known limits.
        let wire = serde_json::to_value(&second).unwrap();
        assert!(wire.get("coverage").is_none());
    }

    #[test]
    fn score_five_accepts_and_score_four_refuses_without_replacing_snapshot() {
        assert_eq!(
            sceno::SCORE_VERSION,
            5,
            "this repin qualifies the version-five family"
        );
        let mut endpoint = TurnstoneEndpoint::new(App::projection_fixture()).unwrap();
        let request = ProjectionRequest {
            version: ProtocolVersion::V1,
            session: endpoint.session().clone(),
            score: Score::new(Arrangement::Spiral(Spiral::default())),
        };
        endpoint.snapshot(request.clone()).unwrap();
        let prior_revision = endpoint.snapshot.as_ref().unwrap().scene.revision;
        let prior_resources = endpoint.resources.clone();
        let mut legacy = request;
        legacy.score.version = 4;
        assert!(matches!(
            endpoint.snapshot(legacy),
            Err(ProjectionRefusal::UnsupportedScore {
                received: 4,
                supported: 5
            })
        ));
        assert_eq!(
            endpoint.snapshot.as_ref().unwrap().scene.revision,
            prior_revision
        );
        assert_eq!(endpoint.resources, prior_resources);
    }

    /// The endpoint's authority is DELEGATED by the user, not self-issued.
    /// Before the capability-model audit its subject was `blake3(session
    /// name)` — not a key — and it granted itself the layout capability, so a
    /// refusal only said "it did not grant itself". Now the refusal is an
    /// authority statement: the capability descends from the profile identity
    /// and is worthless to anyone else.
    #[test]
    fn the_endpoint_holds_a_delegation_from_the_profile_identity() {
        use servitor::AuthorityProvider;

        let app = App::projection_fixture();
        let user = app.personae_root().expect("the fixture's vault is not locked");
        let endpoint = TurnstoneEndpoint::new(app).unwrap();
        let layout = Cap::Scope(layout_scope());

        assert_eq!(
            endpoint.authority.root(),
            user,
            "the chain roots at the user, not at the endpoint itself"
        );
        assert_ne!(
            endpoint.subject.0,
            *blake3::hash(SESSION.as_bytes()).as_bytes(),
            "the subject is a derived KEY, not a hash of its own name"
        );
        assert!(
            endpoint
                .authority
                .covers(endpoint.subject, &layout, Mode::Write),
            "the delegated layout capability covers"
        );
        assert!(
            !endpoint
                .authority
                .covers(endpoint.subject, &Cap::Scope(graph_scope()), Mode::Write),
            "and graph truth was never delegated"
        );

        // The load-bearing half: the SAME certificates under a different root
        // authorize nothing. A self-issued grant could not fail this way.
        let mut foreign = DelegationTable::new([9u8; 32]);
        for cert in endpoint.authority.certificates() {
            foreign.adopt(cert.clone());
        }
        foreign.set_now(endpoint.authority.now());
        assert!(
            !foreign.covers(endpoint.subject, &layout, Mode::Write),
            "the endpoint's authority is anchored to THIS user's identity"
        );
    }

    #[test]
    fn live_mere_graph_becomes_cards_and_routed_relations() {
        let mut endpoint = TurnstoneEndpoint::new(App::projection_fixture()).unwrap();
        let snapshot = endpoint
            .snapshot(ProjectionRequest {
                version: ProtocolVersion::V1,
                session: endpoint.session().clone(),
                score: Score::new(Arrangement::Spiral(Spiral::default())),
            })
            .unwrap();
        assert_eq!(snapshot.scene.active_item_count(), 3);
        assert_eq!(snapshot.scene.tables.relations.iter().flatten().count(), 2);
        assert!(
            snapshot
                .scene
                .tables
                .sources
                .iter()
                .flatten()
                .all(|source| source.adapter == cartography::MERE_GRAPH_ADAPTER)
        );
        assert_eq!(snapshot.presentation.bindings.len(), 3);
    }

    #[test]
    fn projected_grant_accepts_view_and_refuses_graph_change() {
        let run = run_g3_canary().unwrap();
        assert_eq!(run.fit_result, IntentResult::Accepted);
        assert!(matches!(run.open_result, IntentResult::Rejected { .. }));
        assert_eq!(run.graph_nodes_before, run.graph_nodes_after);
        assert_eq!(run.graph_revision_before, run.graph_revision_after);
        assert_eq!(run.audit_revision, 2, "grant plus accepted petition");
        assert!(run.audit_author.starts_with("denizen:"));
        assert_eq!(run.layout.placements.len(), 3);
        assert_eq!(run.layout.relations.len(), 2);
    }

    #[test]
    fn committed_receipt_matches_the_live_turnstone_endpoint() {
        let committed =
            include_str!("../docs/receipts/g3_turnstone_endpoint.html").replace("\r\n", "\n");
        assert_eq!(render_g3_receipt().unwrap(), committed);
    }
}
