// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Mere graph truth adapted to Graphshell's portable endpoint vocabulary.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, SystemTime};

use chartulary::{FacetError, FacetId};
use chirograph::{
    BoundsRelationship, CachePolicy, CardValueV1, ContentHash, EndpointDescriptor,
    IntentInvocation, IntentResult, PortableCardV1, PresentationBinding, PresentationCapability,
    PresentationCodec, PresentationKey, PresentationManifest, PresentationOffer,
    PresentationSemantics, ProjectionOffer, ProjectionRequest, ProjectionSession,
    ProjectionSnapshot, ProtocolVersion, ResourceRequest, ResourceResponse, SemanticRole,
};
use graphshell_endpoint::{IntentSink, PresentationSource, ProjectionCatalog, ProjectionSource};
use mere::kernel::graph::apply::{GraphDelta, apply_graph_delta};
use mere::kernel::graph::{Author, CapturedDelta, Graph, NodeFacetStore, NodeKey, RelationKind};
use mere::kernel::persistence::GraphSnapshot;
use mere::kernel::time::wall_clock_now;
use muniment::{Backend, JsonSlots, StoreError};
use pandect::{GraphSession, MereSessions, Pending, SessionError};
use sceno::{
    Arrangement, Footprint, InstanceId, ProjectedItem, Rect, Representation, RoutedRelation, Scene,
    Score, Size2, SourceRef, Transform2, Vec2,
};
use scenotime::{Revision, SceneEpoch, SceneSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::access::{AccessContext, AccessError, AccessHistory, access_history, record_access};
use crate::handlers::{
    HandlerOffer, HandlerRegistry, OpenAddressV1, handler_from_intent, intent_id,
};

/// The footprint every served item is measured at, in scene units.
const SERVED_FOOTPRINT: (f32, f32) = (240.0, 112.0);
/// The viewport the served arrangement is solved for.
const SERVED_VIEWPORT: (u32, u32) = (1280, 720);
/// The zoom the representation ladder is evaluated at. A served scene has no
/// viewer camera to read, so the rung is selected at this declared zoom and a
/// viewer scales it; see the projection grammar adoption plan, A3.
const SERVED_ZOOM: f32 = 1.0;

/// The one document Graphshell stored before sessions. It is read once into
/// the first session's baseline and never deleted (reservoir plan §7 item 25).
pub const HOST_SLOT: &str = "graphshell/mere-host/v1";
/// The projection epoch, advanced at every open so that an intent observed
/// before a restart reads as stale.
pub const EPOCH_SLOT: &str = "graphshell/projection-epoch/v1";
/// The channel Graphshell's own edits come through, in their author.
pub const GRAPHSHELL: &str = "graphshell";
pub const LOCAL_SESSION: &str = "local:mere";

pub const FIXTURE_WEB_ADDRESS: &str = "https://example.test/i2p-port";
pub const FIXTURE_NON_WEB_ADDRESS: &str = "i2p://reference/service";
pub const FIXTURE_FILE_ADDRESS: &str = "file:///Graphshell/reference-notes.md";
pub const FIXTURE_SCENE_ADDRESS: &str = "mere://scene/reference-host";
pub const FIXTURE_REMOTE_ADDRESS: &str = "graphshell://projection/loopback-g1";
pub const FIXTURE_PERSONA_ADDRESS: &str = "personae://persona/alice";
pub const FIXTURE_DEVICE_ONE_ADDRESS: &str = "personae://device/laptop";
pub const FIXTURE_DEVICE_TWO_ADDRESS: &str = "personae://device/phone";
pub const FIXTURE_KEY_ADDRESS: &str = "personae://key/ssh-ed25519/test";
pub const FIXTURE_GRANT_ADDRESS: &str = "personae://grant/open-addresses";
pub const FIXTURE_RECEIPT_ADDRESS: &str = "personae://receipt/signing/test";
pub const UNKNOWN_FIXTURE_FACET: &str = "future.graphshell.transport-route/v7";

/// Public identity selection injected by the host.
///
/// It deliberately carries references only. Vault handles, private keys, and
/// signing authority remain in Personae on the native side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedPersonaRef {
    pub persona: String,
    pub profile: String,
}

#[derive(Debug)]
pub enum MereHostError {
    Store(StoreError),
    Session(SessionError),
    Access(AccessError),
    Facet(FacetError),
    InvalidSnapshot(String),
    WrongSession,
    MissingResource,
}

impl std::fmt::Display for MereHostError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "Mere host storage: {error}"),
            Self::Session(error) => write!(formatter, "Mere session: {error}"),
            Self::Access(error) => write!(formatter, "{error}"),
            Self::Facet(error) => write!(formatter, "{error}"),
            Self::InvalidSnapshot(error) => write!(formatter, "Mere projection: {error}"),
            Self::WrongSession => write!(formatter, "request names another projection session"),
            Self::MissingResource => {
                write!(formatter, "resource was not disclosed by this session")
            },
        }
    }
}

impl std::error::Error for MereHostError {}

impl From<StoreError> for MereHostError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}

impl From<SessionError> for MereHostError {
    fn from(value: SessionError) -> Self {
        Self::Session(value)
    }
}

impl From<AccessError> for MereHostError {
    fn from(value: AccessError) -> Self {
        Self::Access(value)
    }
}

impl From<FacetError> for MereHostError {
    fn from(value: FacetError) -> Self {
        Self::Facet(value)
    }
}

/// What [`HOST_SLOT`] held, as far as reading it once needs.
#[derive(Clone, Serialize, Deserialize)]
struct PersistedMereHost {
    graph: GraphSnapshot,
    facets: NodeFacetStore,
    projection_epoch: u64,
}

/// The H1 local host: Mere owns truth, Muniment owns bytes, and Graphshell
/// projects scenes and typed intents over both. Truth is a pandect
/// [`GraphSession`]: every edit is journaled under the selected persona, via
/// the channel it came through (reservoir plan §7 item 23).
pub struct MereHost<B> {
    sessions: MereSessions<B>,
    slots: JsonSlots<B>,
    graph_session: GraphSession<B>,
    /// Sessions switched away from, kept until their last changes are stored.
    retired: Vec<GraphSession<B>>,
    via: String,
    selected_persona: SelectedPersonaRef,
    handlers: HandlerRegistry,
    access_context: AccessContext,
    projection_epoch: u64,
    epoch_stored: u64,
    pub(crate) projection_revision: u64,
    resources: BTreeMap<ContentHash, Vec<u8>>,
    instance_targets: Vec<NodeKey>,
    reopened: bool,
}

/// What [`MereHost::stage`] put in a batch, to mark stored once it commits.
pub(crate) struct Staged {
    current: Pending,
    retired: Vec<Pending>,
}

fn saved_at(saved_at_secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(saved_at_secs)
}

fn author_of(persona: &SelectedPersonaRef, via: &str) -> Author {
    Author::person(persona.persona.clone()).via(via)
}

impl<B: Backend + Clone> MereHost<B> {
    fn assemble(
        backend: B,
        graph_session: GraphSession<B>,
        selected_persona: SelectedPersonaRef,
        handlers: HandlerRegistry,
        access_context: AccessContext,
        epoch: (u64, u64),
        reopened: bool,
    ) -> Self {
        Self {
            sessions: MereSessions::new(backend.clone()),
            slots: JsonSlots::new(backend),
            graph_session,
            retired: Vec::new(),
            via: GRAPHSHELL.to_string(),
            selected_persona,
            handlers,
            access_context,
            projection_epoch: epoch.0,
            epoch_stored: epoch.1,
            projection_revision: 1,
            resources: BTreeMap::new(),
            instance_targets: Vec::new(),
            reopened,
        }
    }

    /// A host on a new, empty session, stored at its first persist.
    pub fn empty(
        backend: B,
        selected_persona: SelectedPersonaRef,
        handlers: HandlerRegistry,
        access_context: AccessContext,
    ) -> Self {
        let graph_session = MereSessions::new(backend.clone())
            .begin(author_of(&selected_persona, GRAPHSHELL), None);
        Self::assemble(
            backend,
            graph_session,
            selected_persona,
            handlers,
            access_context,
            (1, 0),
            false,
        )
    }

    /// Reopen the live session changed last (§7 item 24). A store from before
    /// sessions has its old slot read once into a new session's baseline and
    /// left where it is (§7 item 25). A store with no live session begins an
    /// empty one; the old slot is read only while no session exists at all.
    pub async fn open(
        backend: B,
        selected_persona: SelectedPersonaRef,
        handlers: HandlerRegistry,
        access_context: AccessContext,
    ) -> Result<Self, MereHostError> {
        let sessions = MereSessions::new(backend.clone());
        let slots = JsonSlots::new(backend.clone());
        let stored_epoch: Option<u64> = slots.load(EPOCH_SLOT).await?;
        let author = author_of(&selected_persona, GRAPHSHELL);
        let (graph_session, epoch, reopened) = match sessions.latest_live().await? {
            Some(latest) => (
                sessions.open(latest.session_id).await?,
                stored_epoch.unwrap_or(0),
                true,
            ),
            None if !sessions.list().await?.is_empty() => (
                sessions.begin(author, None),
                stored_epoch.unwrap_or(0),
                false,
            ),
            None => match slots.load::<PersistedMereHost>(HOST_SLOT).await? {
                Some(saved) => {
                    // The old host's own load, so the first baseline is the
                    // graph it would have opened.
                    let mut graph = Graph::from_snapshot(&saved.graph);
                    graph.overlay_facets(saved.facets);
                    let mut migrated = sessions.begin(author, Some(graph));
                    migrated.flush(wall_clock_now()).await?;
                    (
                        migrated,
                        stored_epoch.unwrap_or(saved.projection_epoch),
                        true,
                    )
                },
                None => (
                    sessions.begin(author, None),
                    stored_epoch.unwrap_or(0),
                    false,
                ),
            },
        };
        let epoch = epoch.wrapping_add(1);
        slots.save(EPOCH_SLOT, &epoch).await?;
        Ok(Self::assemble(
            backend,
            graph_session,
            selected_persona,
            handlers,
            access_context,
            (epoch, epoch),
            reopened,
        ))
    }

    /// Switch to a new session begun from `graph`, keeping the one it replaces
    /// until its last changes are stored. A new session is a new epoch.
    pub(crate) fn begin_session(&mut self, graph: Graph) {
        let next = self.sessions.begin(self.author(), Some(graph));
        self.retired
            .push(std::mem::replace(&mut self.graph_session, next));
        self.projection_epoch = self.projection_epoch.wrapping_add(1);
        self.projection_revision = 1;
        self.resources.clear();
        self.instance_targets.clear();
    }
}

impl<B: Backend> MereHost<B> {
    /// Store every change not yet stored.
    ///
    /// The clock is injected so tests and importing hosts stamp stable times.
    pub async fn persist(&mut self, saved_at_secs: u64) -> Result<(), MereHostError> {
        let at = saved_at(saved_at_secs);
        for session in &mut self.retired {
            session.flush(at).await?;
        }
        self.retired.clear();
        self.graph_session.flush(at).await?;
        if self.epoch_stored != self.projection_epoch {
            self.slots.save(EPOCH_SLOT, &self.projection_epoch).await?;
            self.epoch_stored = self.projection_epoch;
        }
        Ok(())
    }

    /// Put every change not yet stored into `batch`: capture's write buffer,
    /// so the graph, access records and browsing traces land in one outer
    /// `Backend::apply`. Hand the result to [`staged`](Self::staged) once the
    /// batch commits; until then the changes stay pending.
    pub(crate) async fn stage<T: Backend>(
        &self,
        batch: &T,
        saved_at_secs: u64,
    ) -> Result<Staged, MereHostError> {
        let at = saved_at(saved_at_secs);
        let retired = self
            .retired
            .iter()
            .map(|session| session.pending(at))
            .collect::<Result<Vec<_>, _>>()?;
        let current = self.graph_session.pending(at)?;
        for pending in retired.iter().chain([&current]) {
            if !pending.is_empty() {
                batch.apply(pending.ops()).await?;
            }
        }
        Ok(Staged { current, retired })
    }

    /// Mark what [`stage`](Self::stage) put in a batch as stored.
    pub(crate) fn staged(&mut self, staged: Staged) {
        let count = staged.retired.len();
        for (session, pending) in self.retired.iter_mut().zip(staged.retired) {
            session.stored(pending);
        }
        self.retired.drain(..count);
        self.graph_session.stored(staged.current);
    }

    pub fn graph(&self) -> &Graph {
        self.graph_session.graph()
    }

    /// Whether this host opened stored truth, a session or the old slot,
    /// rather than beginning empty or from the reference fixture.
    pub fn was_reopened(&self) -> bool {
        self.reopened
    }

    /// The session truth lives in: its journal, changes and authors.
    pub fn graph_session(&self) -> &GraphSession<B> {
        &self.graph_session
    }

    pub fn selected_persona(&self) -> &SelectedPersonaRef {
        &self.selected_persona
    }

    pub fn session(&self) -> ProjectionSession {
        ProjectionSession(LOCAL_SESSION.to_string())
    }

    pub fn projection_revision(&self) -> Revision {
        Revision(self.projection_revision)
    }

    pub fn set_access_context(&mut self, context: AccessContext) {
        self.access_context = context;
    }

    pub fn access_history_for(&self, address: &str) -> Result<AccessHistory, MereHostError> {
        let (key, _) = self
            .graph()
            .get_node_by_url(address)
            .ok_or(AccessError::UnknownNode)?;
        Ok(access_history(self.graph(), key)?)
    }

    pub fn facet_value(&self, address: &str, facet: &str) -> Option<&Value> {
        let (_, node) = self.graph().get_node_by_url(address)?;
        self.graph().facets().get(&node.id, &FacetId::new(facet))
    }

    pub fn instance_for_address(&self, address: &str) -> Option<InstanceId> {
        self.instance_targets
            .iter()
            .position(|key| {
                self.graph()
                    .get_node(*key)
                    .is_some_and(|node| node.url() == address)
            })
            .map(|index| InstanceId(index as u32))
    }

    pub fn local_request(&self) -> ProjectionRequest {
        ProjectionRequest {
            version: ProtocolVersion::V1,
            session: self.session(),
            score: self.score(),
        }
    }

    pub(crate) fn score(&self) -> Score {
        self.served_layout()
            .score
            .unwrap_or_else(|| Score::new(Arrangement::Spiral(Default::default())))
    }

    /// The one layout every served projection is built from.
    ///
    /// Every node is measured at the served card footprint and the ladder is
    /// evaluated at a declared zoom of 1.0, so the representation each item
    /// carries is the registry's selection rather than an assertion. The
    /// offer's score and the snapshot come from this same call, which is what
    /// keeps them from disagreeing about a rung.
    fn served_layout(&self) -> mere::canvas::CanvasStrategyProjection {
        let extents: HashMap<NodeKey, (f32, f32)> = self
            .graph()
            .nodes()
            .map(|(key, _)| (key, SERVED_FOOTPRINT))
            .collect();
        mere::canvas::project_canvas_strategy_with_score_for_view(
            "phyllotaxis.default",
            self.graph(),
            None,
            SERVED_VIEWPORT.0,
            SERVED_VIEWPORT.1,
            None,
            Some(&extents),
            true,
            SERVED_ZOOM,
            None,
        )
    }

    fn author(&self) -> Author {
        author_of(&self.selected_persona, &self.via)
    }

    /// Run `edit` on the graph as one journaled change by the selected
    /// persona, via the current channel.
    fn edit_graph<R>(&mut self, edit: impl FnOnce(&mut Graph) -> R) -> R {
        let author = self.author();
        self.graph_session.edit_now(author, edit).0
    }

    /// Run `work` with its edits coming through `via`, such as a browser
    /// extension's captures, rather than through Graphshell (§7 item 23).
    pub(crate) fn through<R>(&mut self, via: String, work: impl FnOnce(&mut Self) -> R) -> R {
        let outer = std::mem::replace(&mut self.via, via);
        let result = work(self);
        self.via = outer;
        result
    }

    pub(crate) fn set_facet(
        &mut self,
        key: NodeKey,
        facet: &str,
        value: Value,
    ) -> Result<(), MereHostError> {
        self.graph().get_node(key).ok_or(AccessError::UnknownNode)?;
        let result = self.edit_graph(|graph| {
            apply_graph_delta(
                graph,
                GraphDelta::SetNodeFacet {
                    key,
                    facet: facet.to_string(),
                    value,
                },
            )
        });
        if matches!(
            result,
            mere::kernel::graph::apply::GraphDeltaResult::NodeMetadataUpdated(true)
        ) {
            self.projection_revision = self.projection_revision.wrapping_add(1);
        }
        Ok(())
    }

    pub(crate) fn mutate_product_graph<R>(&mut self, mutate: impl FnOnce(&mut Graph) -> R) -> R {
        let result = self.edit_graph(mutate);
        self.projection_revision = self.projection_revision.wrapping_add(1);
        result
    }

    /// Apply edits in stable-id form as one change: an import's.
    pub(crate) fn apply_edits(&mut self, edits: Vec<CapturedDelta>) -> Result<(), MereHostError> {
        let author = self.author();
        self.graph_session.apply_now(author, edits)?;
        self.projection_revision = self.projection_revision.wrapping_add(1);
        Ok(())
    }

    fn build_snapshot(&mut self) -> Result<ProjectionSnapshot, MereHostError> {
        let layout = self.served_layout();
        // The ladder's fallback is Glyph; an item the score does not name gets
        // the same answer the ladder would have given it.
        let rungs: HashMap<&str, &Representation> = layout
            .score
            .iter()
            .flat_map(|score| score.items.iter())
            .filter(|item| item.source.adapter == mere::canvas::MERE_GRAPH_ADAPTER)
            .map(|item| (item.source.id.as_str(), &item.representation))
            .collect();
        let mut scene = Scene::new();
        let mut presentation = PresentationManifest::default();
        let mut resources = BTreeMap::new();
        let mut instance_targets = Vec::with_capacity(layout.positions.len());
        let mut instance_of = HashMap::with_capacity(layout.positions.len());
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        for (index, (key, position)) in layout.positions.iter().copied().enumerate() {
            let node = self
                .graph()
                .get_node(key)
                .expect("Mere canvas returned a key from this graph");
            let instance = InstanceId(index as u32);
            instance_targets.push(key);
            instance_of.insert(key, instance);
            min_x = min_x.min(position.x);
            min_y = min_y.min(position.y);
            max_x = max_x.max(position.x);
            max_y = max_y.max(position.y);

            let source = scene.intern_source(SourceRef::new("mere.graph", node.id.to_string()));
            scene.items.push(ProjectedItem {
                source,
                space: Scene::WORLD,
                transform: Transform2::translation(position.x, position.y),
                footprint: Footprint::Rect {
                    size: Size2::new(SERVED_FOOTPRINT.0, SERVED_FOOTPRINT.1),
                },
                representation: rungs
                    .get(node.id.to_string().as_str())
                    .map(|rung| (*rung).clone())
                    .unwrap_or(Representation::Glyph),
                layer: 0,
                visible: true,
                hit: None,
                channels: Vec::new(),
            });

            let kind = node.primary_address().address_kind();
            let mut tags: Vec<String> = node.tags.iter().cloned().collect();
            tags.sort();
            let card = PortableCardV1 {
                title: node.title.clone(),
                values: vec![
                    CardValueV1 {
                        label: "Address".to_string(),
                        value: node.url().to_string(),
                    },
                    CardValueV1 {
                        label: "Kind".to_string(),
                        value: address_kind_label(kind).to_string(),
                    },
                    CardValueV1 {
                        label: "Accesses".to_string(),
                        value: access_history(self.graph(), key)?.records.len().to_string(),
                    },
                ],
                badges: tags,
                media: Vec::new(),
            };
            let bytes =
                serde_json::to_vec(&card).expect("PortableCardV1 fixture always serializes");
            let resource = ContentHash::of(&bytes);
            let key_ref = PresentationKey(format!("mere:{}", node.id));
            let mut semantics = PresentationSemantics {
                label: node.title.clone(),
                role: SemanticRole::Article,
                bounds: BoundsRelationship::FillFootprint,
                actions: Vec::new(),
            };
            self.handlers.attach_actions(&mut semantics, kind);
            presentation.bindings.push(PresentationBinding {
                instance,
                key: key_ref.clone(),
            });
            presentation.offers.insert(
                key_ref,
                vec![PresentationOffer {
                    codec: PresentationCodec::PortableCardV1,
                    resource,
                    byte_size: bytes.len() as u64,
                    requires: PresentationCapability::PortableCard,
                    semantics,
                }],
            );
            resources.insert(resource, bytes);
        }

        for relation in self.graph().relations() {
            let (Some(&from), Some(&to)) = (
                instance_of.get(&relation.from),
                instance_of.get(&relation.to),
            ) else {
                continue;
            };
            let from_position = layout.positions[from.0 as usize].1;
            let to_position = layout.positions[to.0 as usize].1;
            scene.relations.push(RoutedRelation {
                from,
                to,
                space: Scene::WORLD,
                points: vec![
                    Vec2::new(from_position.x, from_position.y),
                    Vec2::new(to_position.x, to_position.y),
                ],
                kind: Some(relation_kind_label(relation.kind).to_string()),
                weight: Some(1.0),
            });
        }

        scene.bounds = if layout.positions.is_empty() {
            Rect::new(Vec2::new(0.0, 0.0), Size2::new(0.0, 0.0))
        } else {
            Rect::new(
                Vec2::new(min_x - 120.0, min_y - 56.0),
                Size2::new(
                    max_x - min_x + SERVED_FOOTPRINT.0,
                    max_y - min_y + SERVED_FOOTPRINT.1,
                ),
            )
        };
        scene.generation = self.projection_revision;
        let scene = SceneSnapshot::from_dense(
            SceneEpoch(self.projection_epoch),
            Revision(self.projection_revision),
            scene,
        )
        .map_err(|error| MereHostError::InvalidSnapshot(format!("{error:?}")))?;

        self.resources = resources;
        self.instance_targets = instance_targets;
        Ok(ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: self.session(),
            scene,
            presentation,
            cache_policy: CachePolicy::default(),
        })
    }
}

impl<B: Backend> ProjectionCatalog for MereHost<B> {
    fn describe(&self) -> EndpointDescriptor {
        EndpointDescriptor {
            label: "Local Mere graph".to_string(),
            projections: vec![ProjectionOffer {
                label: "Current graph".to_string(),
                request: self.local_request(),
            }],
        }
    }
}

impl<B: Backend> ProjectionSource for MereHost<B> {
    type Error = MereHostError;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
        if request.session != self.session() || request.version.major != ProtocolVersion::V1.major {
            return Err(MereHostError::WrongSession);
        }
        self.build_snapshot()
    }
}

impl<B: Backend> PresentationSource for MereHost<B> {
    type Error = MereHostError;

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
        if request.session != self.session() {
            return Err(MereHostError::WrongSession);
        }
        let bytes = self
            .resources
            .get(&request.resource)
            .cloned()
            .ok_or(MereHostError::MissingResource)?;
        Ok(ResourceResponse {
            session: request.session,
            resource: request.resource,
            bytes,
        })
    }
}

impl<B: Backend> IntentSink for MereHost<B> {
    type Error = MereHostError;

    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, Self::Error> {
        if intent.session != self.session() {
            return Err(MereHostError::WrongSession);
        }
        if intent.observed_epoch != SceneEpoch(self.projection_epoch)
            || intent.observed_revision != Revision(self.projection_revision)
        {
            return Ok(IntentResult::Stale {
                current_epoch: SceneEpoch(self.projection_epoch),
                current_revision: Revision(self.projection_revision),
            });
        }
        let Some(&target) = self.instance_targets.get(intent.target.0 as usize) else {
            return Ok(IntentResult::Rejected {
                reason: "intent target is not in the disclosed scene".to_string(),
            });
        };
        let Some(handler_id) = handler_from_intent(&intent.intent) else {
            return Ok(IntentResult::Rejected {
                reason: "intent was not advertised by this endpoint".to_string(),
            });
        };
        let Some(handler) = self.handlers.get(handler_id) else {
            return Ok(IntentResult::Rejected {
                reason: "selected handler is unavailable".to_string(),
            });
        };
        let payload: OpenAddressV1 = match serde_json::from_slice(&intent.payload) {
            Ok(payload) => payload,
            Err(error) => {
                return Ok(IntentResult::Rejected {
                    reason: format!("open payload is invalid: {error}"),
                });
            },
        };
        let node = self
            .graph()
            .get_node(target)
            .expect("disclosed target remains in the graph");
        if payload.handler != handler.id
            || intent.intent != intent_id(&payload.handler)
            || payload.address != node.url()
            || !handler.supports(node.primary_address().address_kind())
        {
            return Ok(IntentResult::Rejected {
                reason: "open payload does not match the advertised target".to_string(),
            });
        }
        let context = self.access_context.clone();
        self.edit_graph(|graph| record_access(graph, target, &context, &payload.handler))?;
        self.projection_revision = self.projection_revision.wrapping_add(1);
        Ok(IntentResult::Accepted)
    }
}

fn address_kind_label(kind: mere::kernel::address::AddressKind) -> &'static str {
    use mere::kernel::address::AddressKind;
    match kind {
        AddressKind::Http => "web",
        AddressKind::File => "file",
        AddressKind::Data => "data",
        AddressKind::GraphshellClip => "clip",
        AddressKind::Directory => "directory",
        AddressKind::Unknown => "custom",
    }
}

fn relation_kind_label(kind: RelationKind) -> &'static str {
    match kind {
        RelationKind::Semantic(_) | RelationKind::OpenPredicate => "semantic",
        RelationKind::Traversal => "traversal",
        RelationKind::Containment(_) => "containment",
        RelationKind::Arrangement(_) => "arrangement",
        RelationKind::Imported(_) => "imported",
        RelationKind::Provenance(_) => "provenance",
    }
}

/// The H1 fixture's two explicit handler choices.
pub fn fixture_handlers() -> HandlerRegistry {
    use mere::kernel::address::AddressKind;
    HandlerRegistry::new(vec![
        HandlerOffer {
            id: "graphshell.inspect".to_string(),
            label: "Inspect in Graphshell".to_string(),
            explanation: "Keep the address in the graph portal.".to_string(),
            address_kinds: vec![
                AddressKind::Http,
                AddressKind::File,
                AddressKind::Data,
                AddressKind::GraphshellClip,
                AddressKind::Directory,
                AddressKind::Unknown,
            ],
        },
        HandlerOffer {
            id: "system.default".to_string(),
            label: "Open in another application".to_string(),
            explanation: "Ask the native host to hand this address to a selected application."
                .to_string(),
            address_kinds: vec![
                AddressKind::Http,
                AddressKind::File,
                AddressKind::Directory,
                AddressKind::Unknown,
            ],
        },
    ])
}

#[cfg(test)]
mod tests {
    use muniment::MemoryBackend;

    use super::*;

    fn selected_persona() -> SelectedPersonaRef {
        SelectedPersonaRef {
            persona: FIXTURE_PERSONA_ADDRESS.to_string(),
            profile: "profile:graphshell-a3".to_string(),
        }
    }

    /// The rung a served item carries, looked up by the node's address.
    fn served_rung(snapshot: &ProjectionSnapshot, graph: &Graph, address: &str) -> Representation {
        let id = graph
            .get_node_by_url(address)
            .expect("fixture node")
            .1
            .id
            .to_string();
        snapshot
            .scene
            .active_items_in_order()
            .into_iter()
            .map(|(_, item)| item)
            .find(|item| {
                snapshot.scene.tables.sources[item.source.0 as usize]
                    .as_ref()
                    .is_some_and(|source| source.id == id)
            })
            .map(|item| item.representation.clone())
            .expect("the address is served")
    }

    /// A3 stage one, applied to the product endpoint: the served snapshot
    /// carries the rung the registry selected at the declared zoom, and the
    /// offer's score says the same thing about every item. The fixture spreads
    /// last-visited times from 10 ms to 110 ms, so the default ladder has a
    /// recency split to make; that spread is asserted first, as the control.
    #[test]
    fn served_snapshot_selects_rungs_from_the_ladder_at_declared_zoom() {
        let mut host =
            MereHost::fixture(MemoryBackend::new(), selected_persona(), fixture_handlers())
                .expect("fixture");
        let oldest = host.graph().get_node_by_url(FIXTURE_WEB_ADDRESS).unwrap().0;
        let newest = host
            .graph()
            .get_node_by_url(FIXTURE_RECEIPT_ADDRESS)
            .unwrap()
            .0;
        assert!(
            host.graph().node_last_visited(oldest) < host.graph().node_last_visited(newest),
            "the fixture must spread visit times or the ladder has nothing to select on"
        );

        let request = host.local_request();
        let snapshot = host.snapshot(request).expect("snapshot");

        assert_eq!(
            served_rung(&snapshot, host.graph(), FIXTURE_WEB_ADDRESS),
            Representation::Glyph,
            "the least recently visited node falls to the ladder's fallback"
        );
        assert_eq!(
            served_rung(&snapshot, host.graph(), FIXTURE_RECEIPT_ADDRESS),
            Representation::Card,
            "a recent node measured at the served footprint earns a card at zoom 1.0"
        );

        // The offer's score and the snapshot come from one layout, so they
        // agree about every item, not only the two named above.
        let score = host.score();
        let mut compared = 0;
        for (_, item) in snapshot.scene.active_items_in_order() {
            let source = snapshot.scene.tables.sources[item.source.0 as usize]
                .as_ref()
                .expect("served items name a source");
            let scored = score
                .items
                .iter()
                .find(|scored| scored.source == *source)
                .expect("every served item is in the offer's score");
            assert_eq!(scored.representation, item.representation, "{}", source.id);
            compared += 1;
        }
        assert_eq!(
            compared,
            score.items.len(),
            "the score names exactly the served items"
        );
    }

    fn access_context() -> AccessContext {
        AccessContext {
            persona: FIXTURE_PERSONA_ADDRESS.to_string(),
            device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
            at_ms: 3_000,
        }
    }

    async fn opened(backend: &MemoryBackend) -> MereHost<MemoryBackend> {
        MereHost::open(
            backend.clone(),
            selected_persona(),
            fixture_handlers(),
            access_context(),
        )
        .await
        .expect("open")
    }

    /// A graph's snapshot, undated, and its facets, encoded.
    fn encoded(graph: &Graph) -> (Vec<u8>, Vec<u8>) {
        let mut snapshot = graph.to_snapshot();
        snapshot.timestamp_secs = 0;
        (
            serde_json::to_vec(&snapshot).unwrap(),
            serde_json::to_vec(graph.facets()).unwrap(),
        )
    }

    #[test]
    fn a_store_from_before_sessions_is_read_once_and_left_in_place() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let old =
                MereHost::fixture(MemoryBackend::new(), selected_persona(), fixture_handlers())
                    .expect("fixture");
            let document = PersistedMereHost {
                graph: old.graph().to_snapshot(),
                facets: old.graph().facets().clone(),
                projection_epoch: 7,
            };
            let slots = JsonSlots::new(backend.clone());
            slots.save(HOST_SLOT, &document).await.expect("old slot");
            let slot_bytes = backend.get(HOST_SLOT).await.unwrap();

            let migrated = opened(&backend).await;
            assert!(migrated.was_reopened());
            let mut expected = Graph::from_snapshot(&document.graph);
            expected.overlay_facets(document.facets.clone());
            assert_eq!(
                encoded(migrated.graph()),
                encoded(&expected),
                "the first baseline is the graph the old host would have opened"
            );
            assert_eq!(
                migrated.projection_epoch, 8,
                "the epoch continues from the slot"
            );
            let sessions = MereSessions::new(backend.clone());
            assert_eq!(sessions.list().await.unwrap().len(), 1, "stored at once");
            assert_eq!(
                backend.get(HOST_SLOT).await.unwrap(),
                slot_bytes,
                "the old slot stays where it is"
            );

            // Once a session exists the slot is never read again: rewriting it
            // changes nothing the next open sees.
            let mut rewritten = document.clone();
            rewritten.graph = Graph::new().to_snapshot();
            rewritten.facets = NodeFacetStore::new();
            slots
                .save(HOST_SLOT, &rewritten)
                .await
                .expect("rewrite slot");
            let mut reopened = opened(&backend).await;
            assert_eq!(encoded(reopened.graph()), encoded(migrated.graph()));
            assert_eq!(sessions.list().await.unwrap().len(), 1);
            assert_eq!(reopened.projection_epoch, 9, "every open is a new epoch");

            // Nor with every session in the trash: the next open begins empty.
            let at = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
            reopened
                .graph_session
                .trash(author_of(&selected_persona(), GRAPHSHELL), at)
                .await
                .unwrap();
            let after_trash = opened(&backend).await;
            assert_eq!(after_trash.graph().node_count(), 0);
            assert!(!after_trash.was_reopened());
        });
    }

    #[test]
    fn graphshell_s_own_edits_are_the_selected_persona_s_via_graphshell() {
        let host = MereHost::fixture(MemoryBackend::new(), selected_persona(), fixture_handlers())
            .expect("fixture");
        let author = Author::person(FIXTURE_PERSONA_ADDRESS).via(GRAPHSHELL);
        let journal = host.graph_session().journal();
        assert!(!journal.entries().is_empty());
        assert!(journal.entries().iter().all(|entry| entry.author == author));
    }
}
