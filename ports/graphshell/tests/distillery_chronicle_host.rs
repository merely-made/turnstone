// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Distillery W1 through the real admitted resident-host path.

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;

use chirograph::{
    CapabilityProfile, Carrier, CarrierRequestBody, CarrierResponseBody, ContentHash,
    PortableCardV1, PresentationCapability, ProtocolVersion, ResourceRequest, ResumeReply,
    SessionOpen,
};
use distillery::{ChronicleObserver, ChronicleRevision, ResidentReceipt};
use graphshell::admission::open_session;
use graphshell::carrier::projection_policy;
use graphshell::native::endpoint_catalog::{ResidentEndpointCatalog, ResidentEndpointRoute};
use graphshell::native::projection_host::ResidentProjectionHost;
use graphshell::network_carrier::{
    CarrierRuntime, NetworkCarrier, dial_projection_session, projection_binding,
};
use graphshell_client::ClientState;
use graphshell_client::frozen::FrozenScene;
use mesh::{Job, JobBoard, JobBoardSnapshot};
use notochord::{LocalNetworkPolicy, NetworkId, ProfileRef, TrafficClass, TrustedRoot};
use personae::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
};
use personae::{IdentityProvider, InMemoryProvider};
use serde::Deserialize;
use transport::PeerID;
use transport::memory::MemoryTransport;

const NETWORK: NetworkId = NetworkId([3; 32]);
const ROOT_AUTHORITY: [u8; 32] = [7; 32];
const NOW_MS: u64 = 50;

fn owner() -> InMemoryProvider {
    InMemoryProvider::from_seed([1; 32])
}

fn viewer() -> InMemoryProvider {
    InMemoryProvider::from_seed([4; 32])
}

fn profile_ref() -> ProfileRef {
    ProfileRef {
        id: "mere.base".into(),
        revision: 1,
    }
}

fn grant(subject: [u8; 32]) -> SignedDelegationCertificate {
    SignedDelegationCertificate::issue(
        &owner(),
        DelegationCertificate::new(
            DelegationParent::Root(ROOT_AUTHORITY),
            owner().master_public_key().to_bytes(),
            subject,
            CapabilityScope {
                domain: graphshell::admission::GRAPHSHELL_DOMAIN.into(),
                resource: NETWORK.0.to_vec(),
                path_prefix: graphshell::admission::PROJECTION_SERVICE.into(),
                actions: [graphshell::admission::CONNECT_ACTION.to_string()]
                    .into_iter()
                    .collect(),
            },
            5,
            10,
            Some(NOW_MS + 3_600_000),
            1,
            [1; 32],
        ),
    )
    .expect("issue certificate")
}

fn policy() -> LocalNetworkPolicy {
    projection_policy(
        NETWORK,
        vec![TrustedRoot {
            authority: ROOT_AUTHORITY,
            issuer: owner().master_public_key().to_bytes(),
        }],
        vec![profile_ref()],
        None,
    )
}

fn board() -> JobBoard {
    #[derive(Deserialize)]
    struct Fixture {
        jobs: Vec<Job>,
    }

    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../distillery/tests/fixtures/chronicle/distillery_board.json"
    ))
    .expect("W0 Chronicle fixture parses");
    JobBoard::fold_from_snapshot(
        [0; 32],
        &JobBoardSnapshot { jobs: fixture.jobs },
        std::iter::empty(),
    )
}

fn open_body() -> CarrierRequestBody {
    CarrierRequestBody::Open(Box::new(SessionOpen {
        version: ProtocolVersion::V1,
        capabilities: CapabilityProfile::new([PresentationCapability::PortableCard]),
    }))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chronicle_is_admitted_served_and_resumed_over_the_resident_host() {
    let viewer = viewer();
    let subject = viewer.master_public_key().to_bytes();
    let client_peer = PeerID::from_bytes(&subject).expect("client peer");
    let server_peer =
        PeerID::from_bytes(&owner().master_public_key().to_bytes()).expect("server peer");
    let (server, client) = MemoryTransport::pair(server_peer, client_peer);

    let observer = ChronicleObserver::new(
        &board(),
        &[
            ResidentReceipt::MaintenanceIdle,
            ResidentReceipt::StopRequested,
        ],
        ChronicleRevision::new(41, 7, 11),
    );
    let route_observer = observer.clone();
    let mut catalog = ResidentEndpointCatalog::new();
    catalog
        .register_resumable_notifying(
            "distillery.chronicle",
            "Distillery Chronicle",
            move |context| Ok(route_observer.endpoint(context.session().clone())),
        )
        .expect("Chronicle route registers");
    let route = ResidentEndpointRoute::new("distillery.chronicle", Duration::from_millis(10))
        .expect("Chronicle route");
    let mut host = ResidentProjectionHost::new(policy(), route, catalog);

    let (mounted_tx, mounted_rx) = mpsc::channel();
    let handle = tokio::runtime::Handle::current();
    let client_task = tokio::task::spawn_blocking(move || {
        let hello = open_session(
            &viewer,
            NETWORK,
            profile_ref(),
            TrafficClass::Interactive,
            [5; 32],
            &projection_binding(client_peer),
            vec![grant(subject)],
        )
        .expect("issue hello");
        let stream = handle
            .block_on(dial_projection_session(
                &client,
                server_peer,
                &hello,
                &policy().limits,
            ))
            .expect("dial")
            .expect("the owner admits the viewer");
        let mut carrier = NetworkCarrier::over(stream, CarrierRuntime::borrowed(handle));

        let opened = match carrier.request(open_body()).expect("open") {
            CarrierResponseBody::Opened(opened) => opened,
            other => panic!("expected opened session, got {other:?}"),
        };
        assert_eq!(opened.descriptor.label, "Distillery Chronicle");
        let request = opened.descriptor.projections[0].request.clone();
        let projection_session = request.session.clone();
        let snapshot = match carrier
            .request(CarrierRequestBody::Snapshot(request))
            .expect("snapshot")
        {
            CarrierResponseBody::Snapshot(snapshot) => *snapshot,
            other => panic!("expected snapshot, got {other:?}"),
        };
        assert_eq!(snapshot.scene.revision.0, 11);
        assert_eq!(snapshot.presentation.bindings.len(), 3);
        let mut client_state = ClientState::default();
        client_state
            .apply_snapshot(snapshot)
            .expect("client applies Chronicle snapshot");

        let initial_resources: Vec<_> = client_state
            .mounted(&projection_session)
            .expect("mounted Chronicle")
            .presentation
            .offers
            .values()
            .flatten()
            .map(|offer| offer.resource)
            .collect();
        for resource in initial_resources {
            let response = match carrier
                .request(CarrierRequestBody::Resource(ResourceRequest {
                    session: projection_session.clone(),
                    resource,
                }))
                .expect("PortableCard resource")
            {
                CarrierResponseBody::Resource(response) => response,
                other => panic!("expected resource, got {other:?}"),
            };
            assert!(response.has_valid_address());
            assert_eq!(ContentHash::of(&response.bytes), response.resource);
            let _: PortableCardV1 =
                serde_json::from_slice(&response.bytes).expect("PortableCardV1");
            client_state
                .apply_resource(response)
                .expect("client caches advertised resource");
        }
        mounted_tx
            .send(projection_session.clone())
            .expect("mounted");

        let notice = carrier.wait_for_notice().expect("Chronicle revision bell");
        assert_eq!(notice.session, projection_session);
        assert_eq!(notice.revision.0, 12);
        let resume_request = client_state
            .resume_request(&projection_session)
            .expect("mounted acknowledgement");
        let reply = match carrier
            .request(CarrierRequestBody::Resume(resume_request))
            .expect("resume")
        {
            CarrierResponseBody::Resume(reply) => reply,
            other => panic!("expected resume, got {other:?}"),
        };
        assert!(matches!(reply, ResumeReply::Diffs(ref diffs) if diffs.len() == 1));
        client_state
            .apply_resume(&projection_session, reply)
            .expect("client applies contiguous Chronicle diff");

        let (bindings, offers) = {
            let mounted = client_state
                .mounted(&projection_session)
                .expect("resumed Chronicle");
            (
                mounted.presentation.bindings.clone(),
                mounted.presentation.offers.clone(),
            )
        };
        let mut names = HashMap::new();
        let mut details = HashMap::new();
        for binding in bindings {
            let offer = offers
                .get(&binding.key)
                .and_then(|offers| offers.first())
                .expect("bound card offer");
            names.insert(binding.instance, offer.semantics.label.clone());
            let response = match carrier
                .request(CarrierRequestBody::Resource(ResourceRequest {
                    session: projection_session.clone(),
                    resource: offer.resource,
                }))
                .expect("updated PortableCard resource")
            {
                CarrierResponseBody::Resource(response) => response,
                other => panic!("expected updated resource, got {other:?}"),
            };
            let card: PortableCardV1 =
                serde_json::from_slice(&response.bytes).expect("updated PortableCardV1");
            details.insert(
                binding.instance,
                card.values
                    .iter()
                    .map(|value| format!("{}: {}", value.label, value.value))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
            client_state
                .apply_resource(response)
                .expect("client caches updated advertised resource");
        }
        let mounted = client_state
            .mounted(&projection_session)
            .expect("resumed Chronicle");
        assert_eq!(mounted.scene.revision.0, 12);
        let frozen = FrozenScene::freeze_snapshot_with_details(
            &mounted.scene,
            "Distillery Chronicle",
            &names,
            &details,
        );
        assert_eq!(frozen.instances.len(), 3);
        let html = frozen.to_html("chronicle-host");
        assert_eq!(html.matches("<tr data-projection-instance=").count(), 3);
        assert!(html.contains("Observation tick: 3"));

        carrier.request(CarrierRequestBody::Close).expect("close");
        carrier.shutdown().expect("shutdown");
        projection_session
    });

    let served = host
        .accept_one(&server, || NOW_MS)
        .await
        .expect("host accepts")
        .expect("viewer admitted");
    assert_eq!(served.subject(), subject);
    let mounted_session = mounted_rx.recv().expect("client mounted");
    assert_eq!(served.session(), &mounted_session);

    observer
        .observe(
            &board(),
            &[
                ResidentReceipt::MaintenanceIdle,
                ResidentReceipt::StopRequested,
                ResidentReceipt::MaintenanceIdle,
            ],
            ChronicleRevision::new(42, 7, 12),
        )
        .expect("resident advances Chronicle");

    let client_session = client_task.await.expect("client thread");
    assert_eq!(client_session, mounted_session);
    let summary = served.finished().await.expect("join").expect("served");
    assert_eq!(
        summary.answered, 10,
        "open, snapshot, six resources, resume, close"
    );
}
