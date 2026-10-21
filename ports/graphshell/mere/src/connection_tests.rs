// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use chirograph::{
    AdvertisedAction, BoundsRelationship, CachePolicy, ContentHash, EndpointDescriptor,
    IntentEffect, IntentReference, IntentResult, PresentationBinding, PresentationCodec,
    PresentationKey, PresentationManifest, PresentationOffer, PresentationSemantics,
    ProjectionOffer, ProjectionRequest, ProjectionSession, ProjectionSnapshot, ProtocolVersion,
    ResourceResponse, SemanticRole,
};
use mere_overmap::wire::{self, SessionGraphV1};
use pandect::SessionId;
use sceno::{Footprint, InstanceId, ProjectedItem, Representation, Scene, SourceRef, Transform2};
use scenotime::{Revision, SceneEpoch, SceneSnapshot};
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::pin,
    task::{Context, Poll, Waker},
};

const RECORD: &str = "00000000-0000-0000-0000-000000000007";
const ADDRESS: &str = "https://example.test/one";

#[derive(Default)]
struct FixtureTransport {
    requests: Vec<CarrierRequestBody>,
    notices: VecDeque<CarrierNotice>,
    responses: VecDeque<Result<CarrierResponseBody, String>>,
    resources: BTreeMap<ContentHash, (ProjectionSession, Vec<u8>)>,
}

impl RouteTransport for FixtureTransport {
    async fn request(&mut self, body: CarrierRequestBody) -> Result<CarrierResponseBody, String> {
        if let CarrierRequestBody::Resource(request) = &body {
            let (session, bytes) = self
                .resources
                .get(&request.resource)
                .cloned()
                .ok_or_else(|| "unknown fixture resource".to_string())?;
            self.requests.push(body);
            return Ok(CarrierResponseBody::Resource(ResourceResponse::new(
                session, bytes,
            )));
        }
        self.requests.push(body);
        self.responses
            .pop_front()
            .unwrap_or_else(|| Err("unexpected fixture request".into()))
    }

    async fn take_notice(&mut self) -> Result<Option<CarrierNotice>, String> {
        Ok(self.notices.pop_front())
    }
}

fn run<F: Future>(future: F) -> F::Output {
    let mut context = Context::from_waker(Waker::noop());
    let mut future = pin!(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture future unexpectedly suspended"),
    }
}

fn projection(name: &str) -> ProjectionOffer {
    ProjectionOffer {
        label: name.into(),
        request: ProjectionRequest {
            version: ProtocolVersion::V1,
            session: ProjectionSession(name.into()),
            score: sceno::score::Score::new(sceno::score::Arrangement::Spiral(Default::default())),
        },
    }
}

fn opened() -> SessionOpened {
    SessionOpened {
        version: ProtocolVersion::V1,
        descriptor: EndpointDescriptor {
            label: "Fixture mere".into(),
            projections: vec![
                projection(wire::MERE_SESSIONS),
                projection(wire::MERE_GRAPH),
            ],
        },
        status: chirograph::SessionStatus::Live,
        expires_at_ms: None,
    }
}

fn card(title: &str, values: &[(&str, &str)], media: Vec<ContentHash>) -> Vec<u8> {
    serde_json::to_vec(&chirograph::PortableCardV1 {
        title: title.into(),
        values: values
            .iter()
            .map(|(label, value)| chirograph::CardValueV1 {
                label: (*label).into(),
                value: (*value).into(),
            })
            .collect(),
        badges: vec![],
        media,
    })
    .unwrap()
}

fn snapshot(
    session: &str,
    sources: &[(&str, &str, &str, Vec<ContentHash>)],
) -> (ProjectionSnapshot, BTreeMap<ContentHash, Vec<u8>>) {
    let mut scene = Scene::new();
    let mut manifest = PresentationManifest::default();
    let mut resources = BTreeMap::new();
    for (index, (adapter, id, title, media)) in sources.iter().enumerate() {
        let bytes = if *adapter == wire::SESSION_ITEM_SOURCE {
            card(title, &[], media.clone())
        } else if *adapter == mere::canvas::MERE_GRAPH_ADAPTER {
            card(title, &[("Address", ADDRESS)], media.clone())
        } else {
            card(title, &[], media.clone())
        };
        let hash = ContentHash::of(&bytes);
        resources.insert(hash, bytes.clone());
        let source = scene.intern_source(SourceRef::new(*adapter, *id));
        scene.items.push(ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::IDENTITY,
            footprint: Footprint::Rect {
                size: sceno::Size2::new(1.0, 1.0),
            },
            representation: Representation::Card,
            layer: 0,
            visible: true,
            hit: None,
            channels: vec![],
        });
        let key = PresentationKey(format!("{adapter}:{id}"));
        manifest.bindings.push(PresentationBinding {
            instance: InstanceId(index as u32),
            key: key.clone(),
        });
        let actions = if *adapter == wire::SESSION_ITEM_SOURCE {
            vec![AdvertisedAction {
                intent: IntentReference(wire::SET_VIEW_INTENT.into()),
                label: "Save view".into(),
                explanation: "Save the application's view state.".into(),
                payload_schema: wire::SET_VIEW_SCHEMA.into(),
                input_form: None,
                effect: IntentEffect::Curation,
            }]
        } else {
            vec![]
        };
        manifest.offers.insert(
            key,
            vec![PresentationOffer {
                codec: PresentationCodec::PortableCardV1,
                resource: hash,
                byte_size: bytes.len() as u64,
                requires: PresentationCapability::PortableCard,
                semantics: PresentationSemantics {
                    label: title.to_string(),
                    role: SemanticRole::Graphic,
                    bounds: BoundsRelationship::FitWithinFootprint,
                    actions,
                },
            }],
        );
    }
    let scene = SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), scene).unwrap();
    (
        ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: ProjectionSession(session.into()),
            scene,
            presentation: manifest,
            cache_policy: CachePolicy::default(),
        },
        resources,
    )
}

fn fixture(read_only: bool, with_material: bool) -> (FixtureTransport, SessionId) {
    fixture_with_state(
        read_only,
        with_material,
        7,
        Some(ADDRESS),
        Some("grid.default"),
    )
}

fn fixture_with_state(
    read_only: bool,
    with_material: bool,
    id: u128,
    focus: Option<&str>,
    strategy: Option<&str>,
) -> (FixtureTransport, SessionId) {
    let id = SessionId(uuid::Uuid::from_u128(id));
    let material_bytes = if with_material {
        let graph = mere::kernel::graph::Graph::new().to_snapshot();
        let mut view = pandect::ViewIntent::default();
        view.focus = focus.map(str::to_owned);
        view.strategy = strategy.map(str::to_owned);
        serde_json::to_vec(&SessionGraphV1 {
            schema: wire::SESSION_GRAPH_SCHEMA.into(),
            application: "graphshell".into(),
            views: BTreeMap::from([("graphshell.mere".into(), view)]),
            session: id,
            read_only,
            epoch: SceneEpoch(1),
            revision: Revision(1),
            graph,
            facets: Default::default(),
        })
        .unwrap()
    } else {
        vec![]
    };
    let material_hash = ContentHash::of(&material_bytes);
    let (sessions, mut session_resources) = snapshot(wire::MERE_SESSIONS, &[]);
    let (graph, mut graph_resources) = snapshot(
        wire::MERE_GRAPH,
        &[
            (
                wire::SESSION_ITEM_SOURCE,
                &id.as_uuid().to_string(),
                "Attached session",
                if with_material {
                    vec![material_hash]
                } else {
                    vec![]
                },
            ),
            (mere::canvas::MERE_GRAPH_ADAPTER, RECORD, "One", vec![]),
        ],
    );
    if with_material {
        graph_resources.insert(material_hash, material_bytes);
    }
    session_resources.append(&mut graph_resources);
    let mut transport = FixtureTransport::default();
    for snapshot in [sessions, graph] {
        transport
            .responses
            .push_back(Ok(CarrierResponseBody::Snapshot(Box::new(snapshot))));
    }
    transport.resources.extend(
        session_resources
            .into_iter()
            .map(|(hash, bytes)| (hash, (ProjectionSession(wire::MERE_GRAPH.into()), bytes))),
    );
    (transport, id)
}

#[test]
fn missing_explicit_session_material_fails_closed_and_clears_panel_state() {
    let (transport, _) = fixture(false, false);
    let profile = CapabilityProfile::new([PresentationCapability::PortableCard]);
    let mut connection = Connection::from_opened(transport, opened(), profile).unwrap();
    let result = run(connection.read());
    let error = result
        .err()
        .expect("a session item without scoped material must be refused");
    assert!(error.contains("explicit session"), "{error}");
    assert!(error.contains("scoped graph material"), "{error}");
    assert!(connection.panel.primary().is_none());
    assert!(connection.panel.records().is_empty());
}

#[test]
fn valid_explicit_session_material_supplies_saved_view_and_read_only_state() {
    let (transport, _) = fixture(true, true);
    let profile = CapabilityProfile::new([PresentationCapability::PortableCard]);
    let mut connection = Connection::from_opened(transport, opened(), profile).unwrap();
    let reading = run(connection.read()).unwrap();
    assert!(reading.read_only);
    assert_eq!(reading.saved_focus.as_deref(), Some(RECORD));
    assert_eq!(reading.model.layout, "grid.default");
    assert!(reading.view_saved);
    assert!(!reading.model.hide_graph);
    assert!(reading.cards.contains_key(RECORD));
}

#[test]
fn selected_record_offers_use_owner_item_after_the_hidden_session_item() {
    let (mut transport, _) = fixture(false, true);
    let action = graphshell::handlers::HandlerOffer {
        id: "fixture".into(),
        label: "Open with fixture".into(),
        explanation: "Configured by the owner".into(),
        address_kinds: vec![],
    }
    .advertised_action();
    for response in &mut transport.responses {
        if let Ok(CarrierResponseBody::Snapshot(snapshot)) = response
            && snapshot.session.0 == wire::MERE_GRAPH
        {
            let key = snapshot
                .presentation
                .bindings
                .iter()
                .find(|binding| binding.instance == InstanceId(1))
                .unwrap()
                .key
                .clone();
            snapshot.presentation.offers.get_mut(&key).unwrap()[0]
                .semantics
                .actions
                .push(action.clone());
        }
    }
    let profile = CapabilityProfile::new([PresentationCapability::PortableCard]);
    let mut connection = Connection::from_opened(transport, opened(), profile).unwrap();
    let reading = run(connection.read()).unwrap();
    let (target, offered) = reading.owner_offers[RECORD].clone();
    assert_eq!(target.target, InstanceId(1));
    assert_eq!(offered, vec![action.clone()]);
    assert_eq!(reading.owner_offers.len(), 1);
    let command = action.command_invocation(
        serde_json::to_vec(&graphshell::handlers::OpenAddressV1 {
            address: ADDRESS.into(),
            handler: "fixture".into(),
        })
        .unwrap(),
    );
    connection
        .transport
        .responses
        .push_back(Ok(CarrierResponseBody::Intent(IntentResult::Rejected {
            reason: "owner refused this request".into(),
        })));
    let (refresh, _) = fixture(false, true);
    connection.transport.responses.extend(refresh.responses);
    connection.take_owner_command(RECORD.into(), target.clone(), action, command);
    let reading = run(connection.read()).unwrap();
    assert!(
        reading
            .model
            .notice
            .unwrap()
            .contains("owner refused this request")
    );
    let request = connection
        .transport
        .requests
        .iter()
        .find_map(|request| match request {
            CarrierRequestBody::Intent(request)
                if request.intent == "graphshell.address.open/fixture" =>
            {
                Some(request)
            },
            _ => None,
        })
        .unwrap();
    assert_eq!(request.target, InstanceId(1));
    assert_eq!(request.observed_epoch, target.observed_epoch);
    assert_eq!(request.observed_revision, target.observed_revision);
}

#[test]
fn read_only_activation_uses_shared_inspection_without_a_carrier_intent() {
    let (transport, _) = fixture(true, true);
    let profile = CapabilityProfile::new([PresentationCapability::PortableCard]);
    let mut connection = Connection::from_opened(transport, opened(), profile).unwrap();
    let initial = run(connection.read()).unwrap();
    assert!(initial.read_only);

    connection.take(MereViewRequest::Activate(RECORD.into()));
    let reading = run(connection.read()).unwrap();
    assert_eq!(reading.selected.as_deref(), Some(RECORD));
    assert!(reading.read_only);
    assert!(
        connection
            .transport
            .requests
            .iter()
            .all(|request| !matches!(request, CarrierRequestBody::Intent(_)))
    );
}

#[test]
fn failed_transport_read_purges_previous_panel_state() {
    let (transport, _) = fixture(true, true);
    let profile = CapabilityProfile::new([PresentationCapability::PortableCard]);
    let mut connection = Connection::from_opened(transport, opened(), profile).unwrap();
    let previous = run(connection.read()).unwrap();
    assert!(previous.read_only);
    connection.take(MereViewRequest::Host("refresh".into()));
    connection
        .transport
        .responses
        .push_back(Err("fixture transport failed".into()));
    assert!(run(connection.read()).is_err());
    assert_eq!(connection.panel.status(), "unavailable");
    assert!(connection.panel.primary().is_none());
    assert!(connection.panel.records().is_empty());
}

#[test]
fn notice_at_an_already_mounted_revision_is_ignored() {
    let (mut transport, _) = fixture(true, true);
    transport.notices.push_back(CarrierNotice {
        session: ProjectionSession(wire::MERE_GRAPH.into()),
        epoch: SceneEpoch(1),
        revision: Revision(1),
    });
    let profile = CapabilityProfile::new([PresentationCapability::PortableCard]);
    let mut connection = Connection::from_opened(transport, opened(), profile).unwrap();
    run(connection.read()).unwrap();
    assert!(run(connection.take_notice()).unwrap());
    let reading = run(connection.read()).unwrap();
    assert!(reading.read_only);
    assert_eq!(reading.graph_epoch, 1);
}

#[test]
fn newer_notice_refreshes_the_shared_session() {
    let (transport, _) = fixture(true, true);
    let profile = CapabilityProfile::new([PresentationCapability::PortableCard]);
    let mut connection = Connection::from_opened(transport, opened(), profile).unwrap();
    let initial = run(connection.read()).unwrap();
    assert!(initial.read_only);
    let before = connection
        .transport
        .requests
        .iter()
        .filter(|request| matches!(request, CarrierRequestBody::Snapshot(_)))
        .count();

    let (refresh, _) = fixture(true, true);
    connection.transport.responses.extend(refresh.responses);
    connection.transport.resources.extend(refresh.resources);
    connection.transport.notices.push_back(CarrierNotice {
        session: ProjectionSession(wire::MERE_GRAPH.into()),
        epoch: SceneEpoch(1),
        revision: Revision(2),
    });
    assert!(run(connection.take_notice()).unwrap());
    let refreshed = run(connection.read()).unwrap();
    assert!(refreshed.read_only);
    let after = connection
        .transport
        .requests
        .iter()
        .filter(|request| matches!(request, CarrierRequestBody::Snapshot(_)))
        .count();
    assert!(after > before, "newer notice did not remount projections");
}

#[test]
fn switching_explicit_session_clears_inspection_and_saved_view_for_same_record() {
    let (transport, _) = fixture(true, true);
    let profile = CapabilityProfile::new([PresentationCapability::PortableCard]);
    let mut connection = Connection::from_opened(transport, opened(), profile).unwrap();
    let initial = run(connection.read()).unwrap();
    assert!(initial.read_only);
    connection.take(MereViewRequest::Activate(RECORD.into()));
    assert_eq!(
        run(connection.read()).unwrap().selected.as_deref(),
        Some(RECORD)
    );

    let (next_session, _) = fixture_with_state(true, true, 8, None, None);
    connection
        .transport
        .responses
        .extend(next_session.responses);
    connection
        .transport
        .resources
        .extend(next_session.resources);
    connection.take(MereViewRequest::Host("refresh".into()));
    let switched = run(connection.read()).unwrap();
    assert!(switched.cards.contains_key(RECORD));
    assert_eq!(switched.selected, None);
    assert_eq!(switched.saved_focus, None);
}

#[test]
fn writable_activation_and_layout_save_through_shared_view_intent() {
    let (transport, _) = fixture_with_state(false, true, 7, None, None);
    let profile = CapabilityProfile::new([PresentationCapability::PortableCard]);
    let mut connection = Connection::from_opened(transport, opened(), profile).unwrap();
    assert!(!run(connection.read()).unwrap().read_only);

    let selected = RECORD;
    connection.take(MereViewRequest::Activate(selected.into()));
    let (updated, _) = fixture_with_state(false, true, 7, Some(ADDRESS), Some("spectral.default"));
    connection
        .transport
        .responses
        .push_back(Ok(CarrierResponseBody::Intent(IntentResult::Accepted)));
    connection.transport.responses.extend(updated.responses);
    connection.transport.resources.extend(updated.resources);
    let activated = run(connection.read()).unwrap();
    assert_eq!(activated.saved_focus.as_deref(), Some(RECORD));
    assert!(activated.view_saved);
    let activation_payload = connection
        .transport
        .requests
        .iter()
        .find_map(|request| match request {
            CarrierRequestBody::Intent(invocation) => Some(invocation.payload.as_slice()),
            _ => None,
        })
        .expect("activation should persist through the carrier");
    let saved: wire::SetViewV1 = serde_json::from_slice(activation_payload).unwrap();
    assert_eq!(saved.state.focus.as_deref(), Some(ADDRESS));

    connection.take(MereViewRequest::Layout("phyllotaxis.default".into()));
    let (updated, _) =
        fixture_with_state(false, true, 7, Some(ADDRESS), Some("phyllotaxis.default"));
    connection
        .transport
        .responses
        .push_back(Ok(CarrierResponseBody::Intent(IntentResult::Accepted)));
    connection.transport.responses.extend(updated.responses);
    connection.transport.resources.extend(updated.resources);
    let laid_out = run(connection.read()).unwrap();
    assert_eq!(laid_out.model.layout, "phyllotaxis.default");
    assert!(laid_out.view_saved);
    let saved_views: Vec<_> = connection
        .transport
        .requests
        .iter()
        .filter_map(|request| match request {
            CarrierRequestBody::Intent(invocation) => {
                serde_json::from_slice::<wire::SetViewV1>(&invocation.payload).ok()
            },
            _ => None,
        })
        .collect();
    assert_eq!(saved_views.len(), 2);
    assert_eq!(saved_views[1].state.focus.as_deref(), Some(ADDRESS));
    assert_eq!(
        saved_views[1].state.strategy.as_deref(),
        Some("phyllotaxis.default")
    );
}
