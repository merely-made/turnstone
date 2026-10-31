// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! One explicitly chosen holder-owned graph. Admission and transport are the
//! Graphshell host's; Pandect owns the graph journal and acknowledged writes.
//! This module never opens a vault or accepts an author from an edit payload.
use chirograph::*;
use graphshell_endpoint::{
    IntentSink, PresentationSource, ProjectionCatalog, ProjectionNoticeSource, ProjectionSource,
    ResumableProjectionSource,
};
use graphshell_host::lifecycle::AdmittedEndpointContext;
use insigne::{
    DerivedKeyAttestation,
    delegation::{
        CapabilityScope, DelegationCertificate, DelegationParent, DelegationRevocation,
        SignedDelegationCertificate, SignedDelegationRevocation,
    },
};
use mere::kernel::graph::{Author, apply::GraphDelta};
use muniment::JsonSlots;
use pandect::{GraphSession, SessionId};
use personae::IdentityProvider;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::SystemTime,
};
use uuid::Uuid;

pub mod control;
pub mod network;
pub const TRANSPORT_SALT: &[u8] = b"graphshell/endpoint-session/coop/v1";
pub const DOMAIN: &str = "mere.graphshell";
pub const EDIT_INTENT: &str = "graphshell.coop.edit/v1";
pub const EDIT_SCHEMA: &str = "graphshell.coop/Edit/v1";
const MAX_NODES: usize = 256;
const MAX_TEXT: usize = 4096;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_millis() as u64
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn path(graph: SessionId) -> String {
    format!("/services/projection/coop/{}", graph.as_uuid())
}
pub fn network(owner: [u8; 32], graph: SessionId) -> [u8; 32] {
    *blake3::hash(
        &[
            b"graphshell.coop/v1".as_slice(),
            &owner,
            graph.as_uuid().as_bytes(),
        ]
        .concat(),
    )
    .as_bytes()
}
pub fn scope(owner: [u8; 32], graph: SessionId) -> CapabilityScope {
    CapabilityScope {
        domain: DOMAIN.into(),
        resource: network(owner, graph).to_vec(),
        path_prefix: path(graph),
        actions: ["connect".into()].into_iter().collect(),
    }
}

fn invitation_scope(owner: [u8; 32], graph: SessionId, write: bool) -> CapabilityScope {
    let mut value = scope(owner, graph);
    if write {
        value.actions.insert("edit".into());
    }
    value
}

/// Root acts stay in the custodian. Tests inject an in-memory owner explicitly.
pub trait Signer: IdentityProvider {
    fn certificate(
        &self,
        value: DelegationCertificate,
    ) -> Result<SignedDelegationCertificate, String>;
    fn revocation(&self, value: DelegationRevocation)
    -> Result<SignedDelegationRevocation, String>;
}
impl Signer for djinn::door::custody_identity::CustodyIdentity {
    fn certificate(
        &self,
        value: DelegationCertificate,
    ) -> Result<SignedDelegationCertificate, String> {
        self.issue_certificate(value).map_err(|e| e.to_string())
    }
    fn revocation(
        &self,
        value: DelegationRevocation,
    ) -> Result<SignedDelegationRevocation, String> {
        self.issue_revocation(value).map_err(|e| e.to_string())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contact {
    pub version: u16,
    pub binding: DerivedKeyAttestation,
}
impl Contact {
    pub fn create(identity: &dyn IdentityProvider) -> Result<Self, String> {
        Ok(Self {
            version: 1,
            binding: identity
                .attest_derived_key(TRANSPORT_SALT)
                .map_err(|e| e.to_string())?,
        })
    }
    pub fn check(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("unsupported contact version".into());
        }
        self.binding
            .check(TRANSPORT_SALT)
            .map_err(|e| format!("invalid contact attestation: {e:?}"))?;
        Ok(())
    }
    pub fn root(&self) -> [u8; 32] {
        *self.binding.master()
    }
    pub fn peer(&self) -> [u8; 32] {
        *self.binding.derived()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invitation {
    pub version: u16,
    pub graph: SessionId,
    pub owner: Contact,
    pub guest: Contact,
    pub write: bool,
    pub grant: SignedDelegationCertificate,
}
impl Invitation {
    pub fn check(&self) -> Result<(), String> {
        self.owner.check()?;
        self.guest.check()?;
        let c = &self.grant.certificate;
        self.grant
            .check()
            .map_err(|e| format!("invalid invitation signature: {e:?}"))?;
        if self.version != 1
            || c.issuer != self.owner.root()
            || c.subject != self.guest.peer()
            || c.parent != DelegationParent::Root(self.owner.root())
            || c.scope != invitation_scope(self.owner.root(), self.graph, self.write)
            || c.remaining_delegation_depth != 0
        {
            return Err("invitation does not bind this owner, graph and guest".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u16,
    owner: [u8; 32],
    graph: SessionId,
    epoch: u64,
    invitation: Option<Invitation>,
    revocation: Option<SignedDelegationRevocation>,
}
fn slot(graph: SessionId) -> String {
    format!("graphshell/coop/v1/{}", graph.as_uuid())
}

/// Only this object writes the selected session. Its lock also serializes an
/// edit against a revocation, including the durable acknowledgement of each.
/// A live application host supplies its existing graph under its own lock.
/// `transact` must compare the stored journal position and install only after
/// the same durable acknowledgement used by the ordinary graph view.
pub trait GraphAuthority: Send + Sync {
    fn read(&self) -> GraphReading;
    fn transact(
        &self,
        expected: Revision,
        author: Author,
        deltas: Vec<GraphDelta>,
    ) -> Result<(), String>;
}
pub struct GraphReading {
    pub graph: mere::kernel::graph::Graph,
    pub revision: Revision,
    pub authors: Vec<Author>,
}
pub struct Owner<B> {
    backend: B,
    graph: Option<GraphSession<B>>,
    authority: Option<Arc<dyn GraphAuthority>>,
    access_guard: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    saved: Saved,
}
pub type SharedOwner<B> = Arc<Mutex<Owner<B>>>;
impl<B: pandect::graph_session::LegacyTransactionBackend + Clone> Owner<B> {
    pub async fn open(backend: B, graph: SessionId, owner: [u8; 32]) -> Result<Self, String> {
        let session = GraphSession::open(backend.clone(), graph)
            .await
            .map_err(|e| e.to_string())?;
        session.ensure_writable().map_err(|e| e.to_string())?;
        Self::load(backend, graph, owner, Some(session), None).await
    }
    /// Attach without opening a second GraphSession or writer for this graph.
    pub async fn attach(
        backend: B,
        graph: SessionId,
        owner: [u8; 32],
        authority: Arc<dyn GraphAuthority>,
    ) -> Result<Self, String> {
        Self::load(backend, graph, owner, None, Some(authority)).await
    }
    async fn load(
        backend: B,
        graph: SessionId,
        owner: [u8; 32],
        session: Option<GraphSession<B>>,
        authority: Option<Arc<dyn GraphAuthority>>,
    ) -> Result<Self, String> {
        let slots = JsonSlots::new(backend.clone());
        let mut saved = slots
            .load::<Saved>(&slot(graph))
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or(Saved {
                version: 1,
                owner,
                graph,
                epoch: 0,
                invitation: None,
                revocation: None,
            });
        if saved.version != 1 || saved.owner != owner || saved.graph != graph {
            return Err("co-op owner or graph does not match this store".into());
        }
        if let Some(invite) = &saved.invitation {
            invite.check()?;
            if invite.owner.root() != owner || invite.graph != graph {
                return Err("stored invitation belongs to another graph".into());
            }
        }
        if let Some(r) = &saved.revocation {
            let invitation = saved
                .invitation
                .as_ref()
                .ok_or("revocation without invitation")?;
            if r.revocation.issuer != owner
                || r.revocation.certificate != invitation.grant.certificate.id()
            {
                return Err("stored revocation names another invitation".into());
            }
            r.check()
                .map_err(|e| format!("invalid stored revocation: {e:?}"))?;
        }
        saved.epoch = saved.epoch.checked_add(1).ok_or("co-op epoch exhausted")?;
        slots
            .save(&slot(graph), &saved)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Self {
            backend,
            graph: session,
            authority,
            access_guard: None,
            saved,
        })
    }
    pub fn set_access_guard(&mut self, guard: Arc<dyn Fn() -> bool + Send + Sync>) {
        self.access_guard = Some(guard);
    }
    pub fn graph_id(&self) -> SessionId {
        self.saved.graph
    }
    pub fn owner_root(&self) -> [u8; 32] {
        self.saved.owner
    }
    pub fn invitation(&self) -> Option<&Invitation> {
        self.saved.invitation.as_ref()
    }
    pub fn revocation(&self) -> Option<&SignedDelegationRevocation> {
        self.saved.revocation.as_ref()
    }
    pub fn epoch(&self) -> SceneEpoch {
        SceneEpoch(self.saved.epoch)
    }
    /// Position in this selected graph's stored journal, not Pandect's
    /// process-local notification counter. The separately stored epoch changes
    /// on reopen; this pair is a projection position, not a content identity.
    pub fn revision(&self) -> Revision {
        self.read_graph().revision
    }
    /// Only standalone owners hold a session here. An attached application's
    /// session stays in that application's authority.
    pub fn graph(&self) -> Option<&GraphSession<B>> {
        self.graph.as_ref()
    }
    fn read_graph(&self) -> GraphReading {
        if let Some(authority) = &self.authority {
            return authority.read();
        }
        let session = self.graph().expect("standalone graph");
        GraphReading {
            graph: session.graph().clone(),
            revision: Revision(session.journal().live_cursor().0),
            authors: session
                .changes()
                .iter()
                .rev()
                .take(32)
                .rev()
                .map(|c| c.author.clone())
                .collect(),
        }
    }
    pub async fn invite(
        &mut self,
        signer: &dyn Signer,
        guest: Contact,
        write: bool,
        expires: u64,
    ) -> Result<Invitation, String> {
        guest.check()?;
        if signer.master_public_key().to_bytes() != self.saved.owner
            || guest.root() == self.saved.owner
        {
            return Err("invite requires the owner and a distinct guest".into());
        }
        if self.saved.revocation.is_none()
            && let Some(existing) = &self.saved.invitation
        {
            if existing.guest.binding == guest.binding
                && existing.write == write
                && existing
                    .grant
                    .certificate
                    .expires_at_ms
                    .is_some_and(|end| now_ms() <= end)
            {
                return Ok(existing.clone());
            }
            return Err("end the existing guest's access before issuing another invitation".into());
        }
        let now = now_ms();
        if expires <= now || expires - now > 30 * 24 * 60 * 60 * 1000 {
            return Err("choose an expiry within thirty days".into());
        }
        let mut nonce = [0; 32];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut nonce);
        let grant = signer.certificate(DelegationCertificate::new(
            DelegationParent::Root(self.saved.owner),
            self.saved.owner,
            guest.peer(),
            invitation_scope(self.saved.owner, self.saved.graph, write),
            now.saturating_sub(30_000),
            now.saturating_sub(30_000),
            Some(expires),
            0,
            nonce,
        ))?;
        let invitation = Invitation {
            version: 1,
            graph: self.saved.graph,
            owner: Contact::create(signer)?,
            guest,
            write,
            grant,
        };
        invitation.check()?;
        let mut saved = self.saved.clone();
        saved.invitation = Some(invitation.clone());
        saved.revocation = None;
        self.save(saved).await?;
        Ok(invitation)
    }
    pub async fn revoke(&mut self, signer: &dyn Signer) -> Result<(), String> {
        if signer.master_public_key().to_bytes() != self.saved.owner {
            return Err("only the owner can end guest access".into());
        }
        let invite = self
            .saved
            .invitation
            .as_ref()
            .ok_or("no guest invitation")?;
        let mut nonce = [0; 32];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut nonce);
        let revocation = signer.revocation(DelegationRevocation::new(
            invite.grant.certificate.id(),
            self.saved.owner,
            invite.grant.certificate.scope.clone(),
            now_ms(),
            nonce,
        ))?;
        let mut saved = self.saved.clone();
        saved.revocation = Some(revocation);
        self.save(saved).await
    }
    async fn save(&mut self, saved: Saved) -> Result<(), String> {
        JsonSlots::new(self.backend.clone())
            .save(&slot(self.saved.graph), &saved)
            .await
            .map_err(|e| e.to_string())?;
        self.saved = saved;
        Ok(())
    }
    fn guest(&self, peer: [u8; 32], write: bool) -> Result<[u8; 32], String> {
        if self.access_guard.as_ref().is_some_and(|guard| !guard()) {
            return Err("owner authority unavailable".into());
        }
        let i = self
            .saved
            .invitation
            .as_ref()
            .ok_or("no guest invitation")?;
        if i.guest.peer() != peer {
            return Err("this guest is not invited".into());
        }
        if self.saved.revocation.is_some() {
            return Err("guest access was revoked".into());
        }
        if i.grant
            .certificate
            .expires_at_ms
            .is_none_or(|end| now_ms() > end)
        {
            return Err("guest access expired".into());
        }
        if write && !i.write {
            return Err("guest has read-only access".into());
        }
        Ok(i.guest.root())
    }
    pub fn admits(
        &self,
        principal: &notochord::AdmittedPrincipal,
        chain: &[SignedDelegationCertificate],
    ) -> Result<(), String> {
        self.guest(principal.subject, false)?;
        let i = self.saved.invitation.as_ref().unwrap();
        if principal.action.domain != DOMAIN
            || principal.action.path != path(self.saved.graph)
            || principal.action.action != "connect"
            || chain.len() != 1
            || chain[0] != i.grant
        {
            return Err("invitation does not authorize this graph route".into());
        }
        Ok(())
    }
    pub fn owner_data(&self) -> Data {
        self.data()
    }
    fn data(&self) -> Data {
        let reading = self.read_graph();
        Data {
            graph: self.saved.graph,
            owner: hex(&self.saved.owner),
            guest: self.saved.invitation.as_ref().map(|i| hex(&i.guest.root())),
            access: if self.saved.revocation.is_some() {
                "revoked"
            } else if self.saved.invitation.as_ref().is_some_and(|i| {
                i.grant
                    .certificate
                    .expires_at_ms
                    .is_some_and(|end| now_ms() > end)
            }) {
                "expired"
            } else {
                self.saved
                    .invitation
                    .as_ref()
                    .map_or("uninvited", |i| if i.write { "editor" } else { "reader" })
            }
            .into(),
            epoch: self.epoch(),
            revision: reading.revision,
            nodes: reading
                .graph
                .nodes()
                .map(|(_, n)| Node {
                    id: n.id,
                    address: n.url().to_string(),
                    title: n.title.clone(),
                })
                .collect(),
            relations: reading
                .graph
                .projected_relations()
                .filter_map(|(_, relation)| {
                    Some(Relation {
                        from: reading.graph.get_node(relation.from)?.id,
                        to: reading.graph.get_node(relation.to)?.id,
                        kind: relation
                            .kind
                            .predicate_iri()
                            .unwrap_or("related")
                            .to_string(),
                    })
                })
                .collect(),
            authors: reading.authors,
        }
    }
    pub fn guest_data(&self, peer: [u8; 32]) -> Result<Data, String> {
        self.guest(peer, false)?;
        Ok(self.data())
    }
    pub async fn edit_owner(&mut self, edit: Edit) -> Result<(), String> {
        self.edit(self.saved.owner, edit).await
    }
    async fn edit(&mut self, root: [u8; 32], edit: Edit) -> Result<(), String> {
        if edit.graph != self.saved.graph {
            return Err("edit names another graph".into());
        }
        if edit.epoch != self.epoch() || edit.revision != self.revision() {
            return Err("graph changed; refresh before retrying".into());
        }
        let reading = self.read_graph();
        let g = &reading.graph;
        let deltas = match edit.operation {
            Operation::Add { id, address, title } => {
                if g.node_count() >= MAX_NODES
                    || g.get_node_key_by_id(id).is_some()
                    || address.trim().is_empty()
                    || address.len() > MAX_TEXT
                    || title.len() > MAX_TEXT
                {
                    return Err("invalid or excessive new address".into());
                }
                let add = GraphDelta::AddNode {
                    id: Some(id),
                    url: address,
                    position: Default::default(),
                };
                let mut candidate = g.clone();
                candidate.set_recorder(None);
                mere::kernel::graph::apply::apply_graph_delta(&mut candidate, add.clone());
                let key = candidate
                    .get_node_key_by_id(id)
                    .ok_or("new node was not created")?;
                vec![add, GraphDelta::SetNodeTitle { key, title }]
            },
            Operation::Rename { id, title } => {
                if title.len() > MAX_TEXT {
                    return Err("title is too long".into());
                }
                let key = g
                    .get_node_key_by_id(id)
                    .ok_or("node is outside the selected graph")?;
                vec![GraphDelta::SetNodeTitle { key, title }]
            },
        };
        // Resolve stable IDs inside the transaction. Add + title is one journaled
        // edit; no numeric key from the guest crosses the authority boundary.
        let author = Author::person(format!("personae:{}", hex(&root))).via("graphshell.coop");
        if let Some(authority) = &self.authority {
            authority.transact(edit.revision, author, deltas)?;
        } else {
            self.graph
                .as_mut()
                .expect("standalone graph")
                .transact_deltas(author, deltas, SystemTime::now())
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub address: String,
    pub title: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    pub from: Uuid,
    pub to: Uuid,
    pub kind: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Data {
    pub graph: SessionId,
    pub owner: String,
    pub guest: Option<String>,
    pub access: String,
    pub epoch: SceneEpoch,
    pub revision: Revision,
    pub nodes: Vec<Node>,
    pub authors: Vec<Author>,
    #[serde(default)]
    pub relations: Vec<Relation>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub graph: SessionId,
    pub epoch: SceneEpoch,
    pub revision: Revision,
    pub operation: Operation,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Operation {
    Add {
        id: Uuid,
        address: String,
        title: String,
    },
    Rename {
        id: Uuid,
        title: String,
    },
}
impl Data {
    pub fn edit(&self, operation: Operation) -> Edit {
        Edit {
            graph: self.graph,
            epoch: self.epoch,
            revision: self.revision,
            operation,
        }
    }
}

/// Per-admission view. No session chooser, archive, filesystem, or invite
/// action is exposed to a guest. Even direct calls recheck durable policy.
pub struct Endpoint<B> {
    owner: SharedOwner<B>,
    context: AdmittedEndpointContext,
    resources: BTreeMap<ContentHash, Vec<u8>>,
    seen: Option<Revision>,
}
impl<B: pandect::graph_session::LegacyTransactionBackend + Clone> Endpoint<B> {
    pub fn new(owner: SharedOwner<B>, context: AdmittedEndpointContext) -> Result<Self, String> {
        owner
            .lock()
            .map_err(|_| "owner lock")?
            .guest(context.subject(), false)?;
        Ok(Self {
            owner,
            context,
            resources: BTreeMap::new(),
            seen: None,
        })
    }
    pub fn request(&self) -> ProjectionRequest {
        ProjectionRequest {
            version: ProtocolVersion::V1,
            session: self.context.session().clone(),
            score: sceno::Score::new(sceno::Arrangement::Spiral(Default::default())),
        }
    }
    fn check_session(&self, session: &ProjectionSession) -> Result<(), String> {
        if session != self.context.session() {
            Err("projection names another session".into())
        } else {
            Ok(())
        }
    }
}
impl<B: pandect::graph_session::LegacyTransactionBackend + Clone> ProjectionCatalog
    for Endpoint<B>
{
    fn describe(&self) -> EndpointDescriptor {
        EndpointDescriptor {
            label: "Shared graph".into(),
            projections: vec![ProjectionOffer {
                label: "Invited graph".into(),
                request: self.request(),
            }],
        }
    }
}
impl<B: pandect::graph_session::LegacyTransactionBackend + Clone> ProjectionSource for Endpoint<B> {
    type Error = String;
    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, String> {
        self.check_session(&request.session)?;
        let data = self
            .owner
            .lock()
            .map_err(|_| "owner lock")?
            .guest_data(self.context.subject())?;
        let card = PortableCardV1 {
            title: format!("Shared graph {}", data.graph.as_uuid()),
            values: vec![CardValueV1 {
                label: "graphshell.coop/state/v1".into(),
                value: serde_json::to_string(&data).map_err(|e| e.to_string())?,
            }],
            badges: vec![data.access.clone()],
            media: vec![],
        };
        let bytes = serde_json::to_vec(&card).map_err(|e| e.to_string())?;
        let hash = ContentHash::of(&bytes);
        self.resources.clear();
        self.resources.insert(hash, bytes.clone());
        let mut scene = sceno::Scene::new();
        let source = scene.intern_source(sceno::SourceRef::new(
            "graphshell.coop",
            data.graph.as_uuid().to_string(),
        ));
        scene.items.push(sceno::ProjectedItem {
            source,
            space: sceno::Scene::WORLD,
            transform: sceno::Transform2::default(),
            footprint: sceno::Footprint::Rect {
                size: sceno::Size2::new(1000., 700.),
            },
            representation: sceno::Representation::Card,
            layer: 0,
            visible: true,
            hit: None,
            channels: Vec::new(),
        });
        let key = PresentationKey("graph".into());
        let mut presentation = PresentationManifest::default();
        presentation.bindings.push(PresentationBinding {
            instance: sceno::InstanceId(0),
            key: key.clone(),
        });
        presentation.offers.insert(
            key,
            vec![PresentationOffer {
                codec: PresentationCodec::PortableCardV1,
                resource: hash,
                byte_size: bytes.len() as u64,
                requires: PresentationCapability::PortableCard,
                semantics: PresentationSemantics {
                    label: format!(
                        "Shared graph {} · {} objects",
                        data.graph.as_uuid(),
                        data.nodes.len()
                    ),
                    role: SemanticRole::Graphic,
                    bounds: BoundsRelationship::FitWithinFootprint,
                    actions: vec![AdvertisedAction {
                        intent: IntentReference(EDIT_INTENT.into()),
                        label: "Edit shared graph".into(),
                        explanation: "Save an attributed edit to this invited graph".into(),
                        payload_schema: EDIT_SCHEMA.into(),
                        input_form: None,
                        effect: IntentEffect::DomainTruth,
                    }],
                },
            }],
        );
        self.seen = Some(data.revision);
        Ok(ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: self.context.session().clone(),
            scene: scenotime::SceneSnapshot::from_dense(data.epoch, data.revision, scene)
                .map_err(|e| format!("{e:?}"))?,
            presentation,
            cache_policy: CachePolicy::default(),
        })
    }
}
impl<B: pandect::graph_session::LegacyTransactionBackend + Clone> PresentationSource
    for Endpoint<B>
{
    type Error = String;
    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, String> {
        self.check_session(&request.session)?;
        self.owner
            .lock()
            .map_err(|_| "owner lock")?
            .guest(self.context.subject(), false)?;
        let bytes = self
            .resources
            .get(&request.resource)
            .ok_or("resource was not disclosed")?
            .clone();
        Ok(ResourceResponse {
            session: request.session,
            resource: request.resource,
            bytes,
        })
    }
}
impl<B: pandect::graph_session::LegacyTransactionBackend + Clone> IntentSink for Endpoint<B> {
    type Error = String;
    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, String> {
        self.check_session(&intent.session)?;
        let mut owner = self.owner.lock().map_err(|_| "owner lock")?;
        let outcome = (|| {
            let root = owner.guest(self.context.subject(), true)?;
            if intent.intent != EDIT_INTENT
                || intent.target != sceno::InstanceId(0)
                || intent.payload.len() > 16384
            {
                return Err("action is not offered by this graph".into());
            }
            let edit: Edit = serde_json::from_slice(&intent.payload).map_err(|e| e.to_string())?;
            if intent.observed_epoch != edit.epoch || intent.observed_revision != edit.revision {
                return Err("edit observation does not match invocation".into());
            }
            pollster::block_on(owner.edit(root, edit))
        })();
        Ok(match outcome {
            Ok(()) => IntentResult::Accepted,
            Err(reason) => IntentResult::Rejected { reason },
        })
    }
}
impl<B: pandect::graph_session::LegacyTransactionBackend + Clone> ResumableProjectionSource
    for Endpoint<B>
{
    type Error = String;
    fn resume(&mut self, request: ResumeRequest) -> Result<ResumeReply, String> {
        self.check_session(&request.session)?;
        self.snapshot(self.request())
            .map(|s| ResumeReply::Snapshot(Box::new(s)))
    }
}
impl<B: pandect::graph_session::LegacyTransactionBackend + Clone> ProjectionNoticeSource
    for Endpoint<B>
{
    type Error = String;
    fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, String> {
        let data = self
            .owner
            .lock()
            .map_err(|_| "owner lock")?
            .guest_data(self.context.subject())?;
        if self.seen != Some(data.revision) {
            self.seen = Some(data.revision);
            Ok(Some(CarrierNotice {
                session: self.context.session().clone(),
                epoch: data.epoch,
                revision: data.revision,
            }))
        } else {
            Ok(None)
        }
    }
}
#[cfg(test)]
mod tests;
