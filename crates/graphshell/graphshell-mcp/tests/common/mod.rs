// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A route that behaves like djinn's: it answers `Open`, refuses what the
//! owner has not granted, and checks the revision a caller observed. Shared by
//! the tests and the `notes_route` example.
#![allow(dead_code)]

use chirograph::{
    AdvertisedAction, BoundsRelationship, CachePolicy, Carrier, CarrierError, CarrierNotice,
    CarrierRequestBody, CarrierResponseBody, ContentHash, EndpointDescriptor, IntentEffect,
    IntentReference, IntentResult, PortableCardV1, PresentationBinding, PresentationCapability,
    PresentationCodec, PresentationKey, PresentationManifest, PresentationOffer,
    PresentationSemantics, ProjectionOffer, ProjectionRequest, ProjectionSession,
    ProjectionSnapshot, ProtocolVersion, ResourceChunkResponse, ResourceResponse, SemanticRole,
    SessionOpened, SessionStatus,
};
use sceno::{
    Arrangement, Footprint, InstanceId, ProjectedItem, Representation, Scene, Score, Size2,
    SourceIx, SourceRef, Spiral, Transform2,
};
use scenotime::{Revision, SceneEpoch, SceneSnapshot};

const SESSION: &str = "mock:notes";

/// A one-projection route holding notes; each is an instance with a card.
pub struct Route {
    pub notes: Vec<String>,
    pub revision: u64,
    pub granted: bool,
    pub seen_open: u32,
    pub gone: bool,
}

impl Route {
    pub fn new(notes: &[&str]) -> Self {
        Self {
            notes: notes.iter().map(|n| n.to_string()).collect(),
            revision: 1,
            granted: true,
            seen_open: 0,
            gone: false,
        }
    }

    fn request() -> ProjectionRequest {
        ProjectionRequest {
            version: ProtocolVersion::V1,
            session: ProjectionSession(SESSION.into()),
            score: Score::new(Arrangement::Spiral(Spiral::default())),
        }
    }

    fn card(&self, index: usize) -> Vec<u8> {
        let card = PortableCardV1 {
            title: self.notes[index].clone(),
            values: Vec::new(),
            badges: vec!["note".into()],
            media: Vec::new(),
        };
        serde_json::to_vec(&card).unwrap()
    }

    fn snapshot(&self) -> ProjectionSnapshot {
        let mut scene = Scene::new();
        let mut manifest = PresentationManifest::default();
        for (index, _) in self.notes.iter().enumerate() {
            scene
                .sources
                .push(SourceRef::new("mock", format!("note-{index}")));
            scene.items.push(ProjectedItem {
                source: SourceIx(index as u32),
                space: Scene::WORLD,
                transform: Transform2::translation(index as f32 * 80.0, 0.0),
                footprint: Footprint::Rect {
                    size: Size2::new(64.0, 40.0),
                },
                representation: Representation::Card,
                layer: 0,
                visible: true,
                hit: None,
                channels: Vec::new(),
            });
            let key = PresentationKey(format!("note:{index}"));
            let bytes = self.card(index);
            manifest.bindings.push(PresentationBinding {
                instance: InstanceId(index as u32),
                key: key.clone(),
            });
            manifest.offers.insert(
                key,
                vec![PresentationOffer {
                    codec: PresentationCodec::PortableCardV1,
                    resource: ContentHash::of(&bytes),
                    byte_size: bytes.len() as u64,
                    requires: PresentationCapability::PortableCard,
                    semantics: PresentationSemantics {
                        label: "Note".into(),
                        role: SemanticRole::Article,
                        bounds: BoundsRelationship::FitWithinFootprint,
                        actions: vec![AdvertisedAction {
                            intent: IntentReference("notes.append".into()),
                            label: "Append a note".into(),
                            explanation: "Add a note after this one.".into(),
                            payload_schema: "notes.append/v1".into(),
                            input_form: None,
                            effect: IntentEffect::DomainTruth,
                        }],
                    },
                }],
            );
        }
        ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: ProjectionSession(SESSION.into()),
            scene: SceneSnapshot::from_dense(SceneEpoch(3), Revision(self.revision), scene)
                .unwrap(),
            presentation: manifest,
            cache_policy: CachePolicy::default(),
        }
    }
}

impl Carrier for Route {
    fn request(&mut self, body: CarrierRequestBody) -> Result<CarrierResponseBody, CarrierError> {
        if self.gone {
            return Err(CarrierError::Disconnected("the resident went away".into()));
        }
        match body {
            CarrierRequestBody::Open(open) => {
                assert!(
                    open.capabilities
                        .capabilities
                        .contains(&PresentationCapability::PortableCard),
                    "the server must ask for cards or the route offers none"
                );
                self.seen_open += 1;
                Ok(CarrierResponseBody::Opened(Box::new(SessionOpened {
                    version: ProtocolVersion::V1,
                    descriptor: EndpointDescriptor {
                        label: "mock".into(),
                        projections: vec![ProjectionOffer {
                            label: "Notes".into(),
                            request: Self::request(),
                        }],
                    },
                    status: SessionStatus::Live,
                    expires_at_ms: None,
                })))
            },
            CarrierRequestBody::Snapshot(_) => {
                Ok(CarrierResponseBody::Snapshot(Box::new(self.snapshot())))
            },
            CarrierRequestBody::ResourceChunk(request) => {
                let bytes = (0..self.notes.len())
                    .map(|i| self.card(i))
                    .find(|bytes| ContentHash::of(bytes) == request.resource)
                    .ok_or_else(|| CarrierError::Refused("unknown resource".into()))?;
                let whole = ResourceResponse::new(request.session, bytes);
                Ok(CarrierResponseBody::ResourceChunk(Box::new(
                    ResourceChunkResponse::slice(&whole, request.offset, request.length),
                )))
            },
            CarrierRequestBody::Intent(intent) => {
                if !self.granted {
                    return Err(CarrierError::Refused(
                        "app mcp.test holds no write grant on this route".into(),
                    ));
                }
                if intent.observed_revision != Revision(self.revision)
                    || intent.observed_epoch != SceneEpoch(3)
                {
                    return Ok(CarrierResponseBody::Intent(IntentResult::Stale {
                        current_epoch: SceneEpoch(3),
                        current_revision: Revision(self.revision),
                    }));
                }
                if intent.intent != "notes.append" {
                    return Ok(CarrierResponseBody::Intent(IntentResult::Rejected {
                        reason: format!("no intent {}", intent.intent),
                    }));
                }
                self.notes.push(String::from_utf8(intent.payload).unwrap());
                self.revision += 1;
                Ok(CarrierResponseBody::Intent(IntentResult::Accepted))
            },
            other => Err(CarrierError::Refused(format!("unexpected {other:?}"))),
        }
    }

    fn take_notice(&mut self) -> Option<CarrierNotice> {
        None
    }

    fn wait_for_notice(&mut self) -> Result<CarrierNotice, CarrierError> {
        Err(CarrierError::Disconnected("no notices".into()))
    }

    fn shutdown(&mut self) -> Result<(), CarrierError> {
        Ok(())
    }
}
