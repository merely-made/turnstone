// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The identity endpoint served over the browser carrier and
//! `graphshell_host`'s admitted session loop.
//!
//! It was a test in `session_loop` until that loop moved to
//! `graphshell-host` (Graphshell stack lift plan, L2). The loop is the
//! host's; the browser carrier, the identity endpoint and the fixed resident
//! authority are this port's, so the composed receipt stays here.

use std::sync::{Arc, RwLock};

use chirograph::{
    CapabilityProfile, CarrierRequest, CarrierRequestBody, CarrierResponseBody, IntentInvocation,
    IntentResult, PresentationCapability, ProtocolVersion, ResourceRequest, ResumeRequest,
    SessionOpen,
};
use graphshell_client::{ClientState, PresentationResolution, ResolvedContent};
use graphshell_endpoint::door::wire::BrowserChallenge;
use graphshell_host::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE};
use graphshell_host::lifecycle::SessionAuthority;
use graphshell_host::session_loop::{SessionEnd, serve_admitted_session};
use insigne::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
};
use notochord::{
    LocalNetworkPolicy, NetworkId, ProfileRef, RevocationLedger, ServiceAccess, ServiceRule,
    TrustedRoot,
};
use personae::delegation::Issue;
use personae::{IdentityProvider, InMemoryProvider};

use crate::browser_carrier::{
    BrowserLauncher, BrowserLink, BrowserMessage, CHROMIUM_EXTENSION_ID, admit_browser_session,
};
use crate::identity_endpoint::IdentityEndpoint;
use crate::identity_projection::{SIGNING_APPROVE_ONCE_INTENT, SigningDecisionIntentV1};

const NOW_MS: u64 = 50;
const EXPIRY_MS: u64 = 100;
const ROOT_AUTHORITY: [u8; 32] = [7; 32];
const NETWORK: [u8; 32] = [3; 32];

fn owner() -> InMemoryProvider {
    InMemoryProvider::from_seed([1; 32])
}

fn viewer() -> InMemoryProvider {
    InMemoryProvider::from_seed([4; 32])
}

fn grant() -> SignedDelegationCertificate {
    SignedDelegationCertificate::issue(
        &owner(),
        DelegationCertificate::new(
            DelegationParent::Root(ROOT_AUTHORITY),
            owner().master_public_key().to_bytes(),
            viewer().master_public_key().to_bytes(),
            CapabilityScope {
                domain: GRAPHSHELL_DOMAIN.into(),
                resource: NETWORK.to_vec(),
                path_prefix: PROJECTION_SERVICE.into(),
                actions: [CONNECT_ACTION.to_string()].into_iter().collect(),
            },
            5,
            10,
            Some(EXPIRY_MS),
            1,
            [1; 32],
        ),
    )
    .expect("issue certificate")
}

fn carrier_request(id: u64, body: CarrierRequestBody) -> CarrierRequest {
    CarrierRequest { id, body }
}

#[tokio::test]
async fn browser_carrier_delivers_public_identity_cards_and_an_approval() {
    use base64::Engine;
    use ssh_key::{Algorithm, LineEnding};

    // A fixed authority holding one pending signing request: the carrier
    // and endpoint are what is under test here. The keeper's own approval
    // path (agent, broker, signature) is castellan's and djinn's to prove.
    let mut private =
        ssh_key::PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
    private.set_comment("admitted-identity");
    let private_openssh = private.to_openssh(LineEnding::LF).unwrap().to_string();
    let pending = personae::signing::SigningRequest::new(
        "research",
        "SHA256:admitted-identity",
        "ssh.sign",
        b"approved by admitted browser client",
        "ssh-agent",
    );
    let pending_id = pending.request_id;
    let mut fixed = crate::identity_endpoint::test_support::fixed_snapshot();
    fixed
        .pending_signing
        .push(personae::signing::PendingSigningRequest {
            request: pending,
            policy: personae::signing::SigningPolicy::PerUse,
            expires_at_ms: u64::MAX,
        });
    let host = crate::identity_endpoint::test_support::FixedIdentity::with_snapshot(fixed);

    let launcher =
        BrowserLauncher::parse(&[format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/")]).unwrap();
    let challenge = BrowserChallenge::fresh();
    let link = BrowserLink::accept(
        launcher,
        &challenge,
        BrowserMessage::Connect {
            schema: "mere.graphshell/browser-connect/v1".to_string(),
            host_nonce: challenge.host_nonce.clone(),
            client_nonce: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([0x31; 32]),
        },
    )
    .unwrap();
    let mut policy = LocalNetworkPolicy::closed(NetworkId(NETWORK));
    policy.accepted_profiles = vec![ProfileRef {
        id: "mere.base".into(),
        revision: 1,
    }];
    policy.trusted_roots = vec![TrustedRoot {
        authority: ROOT_AUTHORITY,
        issuer: owner().master_public_key().to_bytes(),
    }];
    policy.services.insert(
        PROJECTION_SERVICE.to_string(),
        ServiceRule::new(
            ServiceAccess::MemberOnly,
            GRAPHSHELL_DOMAIN,
            [CONNECT_ACTION],
            false,
            None,
        ),
    );
    let (mut browser, mut admitted) = admit_browser_session(
        &viewer(),
        NetworkId(NETWORK),
        ProfileRef {
            id: "mere.base".into(),
            revision: 1,
        },
        vec![grant()],
        &link,
        &policy,
        &RevocationLedger::default(),
        NOW_MS,
    )
    .await
    .unwrap();
    let authority = SessionAuthority::retain_admitted(&admitted);
    let mut endpoint = IdentityEndpoint::for_admitted(Arc::clone(&host), &authority);
    let projection = endpoint.request();
    let expected_session = projection.session.clone();
    assert_eq!(
        projection.session,
        authority.session().clone(),
        "the browser cannot choose the projection session id"
    );

    let server_task = tokio::spawn(async move {
        let revocations = RwLock::new(RevocationLedger::default());
        let mut resume =
            |_: &mut IdentityEndpoint<crate::identity_endpoint::test_support::FixedIdentity>,
             _: ResumeRequest| {
                Err("identity resume is not implemented".to_string())
            };
        serve_admitted_session(
            &mut admitted,
            &authority,
            &revocations,
            &mut endpoint,
            &mut resume,
            || NOW_MS,
        )
        .await
        .unwrap()
    });

    let opened = browser
        .request(&carrier_request(
            1,
            CarrierRequestBody::Open(Box::new(SessionOpen {
                version: ProtocolVersion { major: 1, minor: 0 },
                capabilities: CapabilityProfile::default(),
            })),
        ))
        .await
        .unwrap();
    assert!(matches!(opened.body, Ok(CarrierResponseBody::Opened(_))));

    let snapshot_response = browser
        .request(&carrier_request(
            2,
            CarrierRequestBody::Snapshot(projection),
        ))
        .await
        .unwrap();
    let snapshot = match snapshot_response.body {
        Ok(CarrierResponseBody::Snapshot(snapshot)) => *snapshot,
        other => panic!("expected identity snapshot, got {other:?}"),
    };
    assert_eq!(snapshot.session, expected_session);
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains(&private_openssh)
    );

    let resources = snapshot
        .presentation
        .offers
        .values()
        .flatten()
        .map(|offer| offer.resource)
        .collect::<Vec<_>>();
    let session = snapshot.session.clone();
    let mut client = ClientState::default();
    client.apply_snapshot(snapshot).unwrap();
    for (offset, resource) in resources.into_iter().enumerate() {
        let response = browser
            .request(&carrier_request(
                3 + offset as u64,
                CarrierRequestBody::Resource(ResourceRequest {
                    session: session.clone(),
                    resource,
                }),
            ))
            .await
            .unwrap();
        let resource = match response.body {
            Ok(CarrierResponseBody::Resource(resource)) => resource,
            other => panic!("expected identity resource, got {other:?}"),
        };
        let text = String::from_utf8(resource.bytes.clone()).unwrap();
        assert!(!text.contains(&private_openssh));
        assert!(!text.contains("BEGIN OPENSSH PRIVATE KEY"));
        client.apply_resource(resource).unwrap();
    }

    let first = client
        .mounted(&session)
        .unwrap()
        .scene
        .active_items_in_order()[0]
        .0;
    let resolved = client
        .resolve(
            &session,
            first,
            &CapabilityProfile::new([PresentationCapability::PortableCard]),
        )
        .unwrap();
    assert!(matches!(
        resolved,
        PresentationResolution::Ready(resolved)
            if matches!(resolved.content, ResolvedContent::PortableCard(_))
    ));

    let mounted = client.mounted(&session).unwrap();
    let target = mounted
        .presentation
        .bindings
        .iter()
        .find(|binding| {
            mounted.presentation.offers.get(&binding.key).unwrap()[0]
                .semantics
                .actions
                .iter()
                .any(|action| action.intent.0 == SIGNING_APPROVE_ONCE_INTENT)
        })
        .unwrap()
        .instance;
    let request_id = match client
        .resolve(
            &session,
            target,
            &CapabilityProfile::new([PresentationCapability::PortableCard]),
        )
        .unwrap()
    {
        PresentationResolution::Ready(resolved) => match resolved.content {
            ResolvedContent::PortableCard(card) => card
                .values
                .iter()
                .find(|value| value.label == "Request")
                .and_then(|value| uuid::Uuid::parse_str(&value.value).ok())
                .expect("pending card discloses its public request id"),
            other => panic!("expected pending portable card, got {other:?}"),
        },
        other => panic!("expected ready pending card, got {other:?}"),
    };
    let acknowledgement = client.acknowledgement(&session).unwrap();
    let intent = browser
        .request(&carrier_request(
            90,
            CarrierRequestBody::Intent(IntentInvocation {
                session: session.clone(),
                target,
                observed_epoch: acknowledgement.epoch,
                observed_revision: acknowledgement.revision,
                intent: SIGNING_APPROVE_ONCE_INTENT.to_string(),
                payload: serde_json::to_vec(&SigningDecisionIntentV1 { request_id }).unwrap(),
            }),
        ))
        .await
        .unwrap();
    assert!(matches!(
        intent.body,
        Ok(CarrierResponseBody::Intent(IntentResult::Accepted))
    ));
    assert_eq!(
        request_id, pending_id,
        "the card disclosed the pending request"
    );
    assert_eq!(
        host.intents.lock().unwrap().as_slice(),
        [SIGNING_APPROVE_ONCE_INTENT.to_string()],
        "the approval reached the resident authority"
    );

    let closed = browser
        .request(&carrier_request(100, CarrierRequestBody::Close))
        .await
        .unwrap();
    assert!(matches!(closed.body, Ok(CarrierResponseBody::Closed)));
    assert_eq!(server_task.await.unwrap().end, SessionEnd::Closed);
}
