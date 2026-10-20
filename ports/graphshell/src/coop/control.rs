// SPDX-License-Identifier: MPL-2.0
//! Owner-only application-door control. Never register this on a peer catalog.
use super::*;
pub const INTENT: &str = "graphshell.coop.owner/v1";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Request {
    Read {
        graph: SessionId,
    },
    Invite {
        graph: SessionId,
        guest: Contact,
        write: bool,
    },
    Revoke {
        graph: SessionId,
    },
    Edit(Edit),
}
impl Request {
    pub fn graph(&self) -> SessionId {
        match self {
            Self::Read { graph } | Self::Invite { graph, .. } | Self::Revoke { graph } => *graph,
            Self::Edit(edit) => edit.graph,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub data: Data,
    pub contact: network::Card,
    pub invitation: Option<Invitation>,
}
pub struct Endpoint {
    session: ProjectionSession,
    call: Arc<dyn Fn(Request) -> Result<Reply, String> + Send + Sync>,
    current: Option<Vec<u8>>,
    authorize: Arc<dyn Fn() -> Result<(), String> + Send + Sync>,
}
impl Endpoint {
    pub fn new(
        session: ProjectionSession,
        authorize: Arc<dyn Fn() -> Result<(), String> + Send + Sync>,
        call: Arc<dyn Fn(Request) -> Result<Reply, String> + Send + Sync>,
    ) -> Self {
        Self {
            session,
            call,
            authorize,
            current: None,
        }
    }
    pub fn request(&self) -> ProjectionRequest {
        ProjectionRequest {
            version: ProtocolVersion::V1,
            session: self.session.clone(),
            score: sceno::Score::new(sceno::Arrangement::Spiral(Default::default())),
        }
    }
}
impl ProjectionCatalog for Endpoint {
    fn describe(&self) -> EndpointDescriptor {
        EndpointDescriptor {
            label: "Graph co-op controls".into(),
            projections: vec![ProjectionOffer {
                label: "Owner controls".into(),
                request: self.request(),
            }],
        }
    }
}
impl IntentSink for Endpoint {
    type Error = String;
    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, String> {
        (self.authorize)()?;
        if intent.session != self.session
            || intent.intent != INTENT
            || intent.target != sceno::InstanceId(0)
            || intent.payload.len() > 32768
        {
            return Err("not an owner control request".into());
        }
        let request = serde_json::from_slice(&intent.payload).map_err(|e| e.to_string())?;
        match (self.call)(request) {
            Ok(reply) => {
                self.current = Some(
                    serde_json::to_vec(&PortableCardV1 {
                        title: "Graph co-op controls".into(),
                        values: vec![CardValueV1 {
                            label: INTENT.into(),
                            value: serde_json::to_string(&reply).map_err(|e| e.to_string())?,
                        }],
                        badges: vec![],
                        media: vec![],
                    })
                    .map_err(|e| e.to_string())?,
                );
                Ok(IntentResult::Accepted)
            },
            Err(reason) => {
                self.current = None;
                Ok(IntentResult::Rejected { reason })
            },
        }
    }
}
impl ProjectionSource for Endpoint {
    type Error = String;
    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, String> {
        (self.authorize)()?;
        if request.session != self.session {
            return Err("another control session".into());
        }
        let bytes = self
            .current
            .as_ref()
            .ok_or("choose an existing graph first")?;
        let mut scene = sceno::Scene::new();
        let source = scene.intern_source(sceno::SourceRef::new(INTENT, "owner"));
        scene.items.push(sceno::ProjectedItem {
            source,
            space: sceno::Scene::WORLD,
            transform: Default::default(),
            footprint: sceno::Footprint::Rect {
                size: sceno::Size2::new(1., 1.),
            },
            representation: sceno::Representation::Card,
            layer: 0,
            visible: false,
            hit: None,
            channels: vec![],
        });
        let key = PresentationKey(INTENT.into());
        let mut presentation = PresentationManifest::default();
        presentation.bindings.push(PresentationBinding {
            instance: sceno::InstanceId(0),
            key: key.clone(),
        });
        presentation.offers.insert(
            key,
            vec![PresentationOffer {
                codec: PresentationCodec::PortableCardV1,
                resource: ContentHash::of(bytes),
                byte_size: bytes.len() as u64,
                requires: PresentationCapability::PortableCard,
                semantics: PresentationSemantics {
                    label: "Owner controls".into(),
                    role: SemanticRole::Graphic,
                    bounds: BoundsRelationship::FitWithinFootprint,
                    actions: vec![],
                },
            }],
        );
        Ok(ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: self.session.clone(),
            scene: scenotime::SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), scene)
                .map_err(|e| format!("{e:?}"))?,
            presentation,
            cache_policy: CachePolicy::default(),
        })
    }
}
impl PresentationSource for Endpoint {
    type Error = String;
    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, String> {
        (self.authorize)()?;
        let bytes = self.current.as_ref().ok_or("no current reply")?;
        if request.session != self.session || request.resource != ContentHash::of(bytes) {
            return Err("undisclosed owner resource".into());
        }
        Ok(ResourceResponse::new(self.session.clone(), bytes.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    #[test]
    fn owner_control_rechecks_access_before_serving_a_cached_reply() {
        let (owner, a, _, graph) =
            pollster::block_on(crate::coop::tests::fixture(muniment::MemoryBackend::new()));
        let reply = Reply {
            data: owner.owner_data(),
            contact: network::Card {
                version: 1,
                graph,
                owner: Contact::create(&a).unwrap(),
                ticket: "unused in this control test".into(),
            },
            invitation: None,
        };
        let allowed = Arc::new(AtomicBool::new(true));
        let current = allowed.clone();
        let mut endpoint = Endpoint::new(
            ProjectionSession("owner-control-test".into()),
            Arc::new(move || {
                if current.load(Ordering::SeqCst) {
                    Ok(())
                } else {
                    Err("owner access ended".into())
                }
            }),
            Arc::new(move |_| Ok(reply.clone())),
        );
        let request = endpoint.request();
        let intent = IntentInvocation {
            session: request.session.clone(),
            target: sceno::InstanceId(0),
            observed_epoch: SceneEpoch(1),
            observed_revision: Revision(1),
            intent: INTENT.into(),
            payload: serde_json::to_vec(&Request::Read { graph }).unwrap(),
        };
        assert_eq!(
            endpoint.invoke(intent.clone()).unwrap(),
            IntentResult::Accepted
        );
        let snapshot = endpoint.snapshot(request.clone()).unwrap();
        let resource = ResourceRequest {
            session: request.session.clone(),
            resource: snapshot
                .presentation
                .offers
                .values()
                .flatten()
                .next()
                .unwrap()
                .resource,
        };
        assert!(endpoint.resource(resource.clone()).is_ok());
        allowed.store(false, Ordering::SeqCst);
        assert!(endpoint.resource(resource).is_err());
        assert!(endpoint.snapshot(request).is_err());
        assert!(endpoint.invoke(intent).is_err());
    }
}
