// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! The product composition of Graphshell's existing admitted carrier and host.
use super::*;
use djinn::door::custody_identity::CustodyIdentity;
use djinn::door::route::ResidentEndpointRoute;
use graphshell_host::{
    admission::open_session_for,
    carrier::{projection_alpn, projection_policy},
    endpoint_catalog::ResidentEndpointCatalog,
    network_carrier::{dial_projection_session, projection_binding},
    projection_host::{ResidentProjectionHost, SelectedRoute},
};
use notochord::{NetworkId, ProfileRef, RequestedAction, TrafficClass, TrustedRoot};
use std::{path::Path, time::Duration};
use transport::{P2pandaTransport, Transport};

pub fn profile() -> ProfileRef {
    ProfileRef {
        id: "mere.base".into(),
        revision: 1,
    }
}
pub fn identity() -> Result<Arc<CustodyIdentity>, String> {
    CustodyIdentity::connect("graphshell")
        .map(Arc::new)
        .map_err(|e| format!("identity pending: {e}"))
}
pub async fn bind(identity: &dyn IdentityProvider) -> Result<Arc<P2pandaTransport>, String> {
    bind_with_relays(identity, &[]).await
}

/// Bind with relays explicitly selected by the holder. Reachability never
/// changes graph admission, and no public relay is selected implicitly.
pub async fn bind_with_relays(
    identity: &dyn IdentityProvider,
    relays: &[transport::p2panda_transport::RelayUrl],
) -> Result<Arc<P2pandaTransport>, String> {
    let key = identity
        .derived_keypair(TRANSPORT_SALT)
        .map_err(|e| e.to_string())?;
    let mut builder = P2pandaTransport::builder_for(&key).alpns(vec![projection_alpn()]);
    for relay in relays {
        builder = builder.relay_url(relay.clone());
    }
    let transport = builder
        .bind()
        .await
        .map(Arc::new)
        .map_err(|e| e.to_string())?;
    if !relays.is_empty() {
        // The contact is kept for this activity's lifetime. Do not freeze a
        // LAN-only ticket while its explicitly configured relay is connecting.
        let ready = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let address = transport.endpoint_addr().await.map_err(|e| e.to_string())?;
                if address.relay_urls().any(|url| relays.contains(url)) {
                    return Ok::<_, String>(());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| "configured co-op relay did not become reachable".to_string())
        .and_then(|result| result);
        if let Err(error) = ready {
            let _ = transport.close().await;
            return Err(error);
        }
    }
    Ok(transport)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Card {
    pub version: u16,
    pub graph: SessionId,
    pub owner: Contact,
    pub ticket: String,
}
pub async fn card(
    identity: &dyn IdentityProvider,
    graph: SessionId,
    transport: &P2pandaTransport,
) -> Result<Card, String> {
    Ok(Card {
        version: 1,
        graph,
        owner: Contact::create(identity)?,
        ticket: transport.ticket().await.map_err(|e| e.to_string())?,
    })
}
/// Small, public handoff artifacts use replacement, never partial writes.
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    pandect::atomic_file::write_bytes_with_backup(path, &bytes).map_err(|e| e.to_string())
}
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("handoff artifact is too large".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

pub fn host<B: pandect::graph_session::LegacyTransactionBackend + Clone + Send + 'static>(
    owner: SharedOwner<B>,
) -> Result<ResidentProjectionHost, String> {
    let state = owner.lock().map_err(|_| "owner lock")?;
    let graph = state.graph_id();
    let root = state.owner_root();
    let revoked = state.revocation().cloned();
    drop(state);
    let mut catalog = ResidentEndpointCatalog::new();
    let product = owner.clone();
    catalog
        .register_resumable_notifying("coop", "Shared graph", move |context| {
            Endpoint::new(product.clone(), context.clone())
        })
        .map_err(|e| e.to_string())?;
    let policy = projection_policy(
        NetworkId(network(root, graph)),
        vec![TrustedRoot {
            authority: root,
            issuer: root,
        }],
        vec![profile()],
        Some(4),
    );
    // The base policy owns connect beneath projection; this selector narrows
    // every admission to the exact issued certificate, subject and graph path.
    let selector = Arc::new(
        move |principal: &notochord::AdmittedPrincipal, chain: &[SignedDelegationCertificate]| {
            owner
                .lock()
                .map_err(|_| "owner lock")?
                .admits(principal, chain)?;
            Ok(SelectedRoute {
                route: ResidentEndpointRoute::new("coop", Duration::from_millis(100))
                    .map_err(|e| e.to_string())?,
                application: Some("graphshell.coop".into()),
            })
        },
    );
    let host = ResidentProjectionHost::selecting(policy, catalog, selector);
    if let Some(revoked) = revoked {
        host.revocations()
            .write()
            .map_err(|_| "ledger lock")?
            .fold(revoked.check().map_err(|e| format!("{e:?}"))?);
    }
    Ok(host)
}

pub struct Guest {
    pub invitation: Invitation,
    pump: graphshell_client::pump::LinePump<<P2pandaTransport as Transport>::Stream>,
    runtime: tokio::runtime::Handle,
    next_id: u64,
    dirty: bool,
    request: ProjectionRequest,
    pub data: Option<Data>,
    _transport: Arc<P2pandaTransport>,
}
impl Guest {
    /// Current transport observations, including which route carries traffic.
    /// This reads the existing carrier; it never establishes another session.
    pub fn peer_paths(&self) -> Result<Vec<transport::PeerPath>, String> {
        let peer = transport::PeerID::from_bytes(&self.invitation.owner.peer())
            .map_err(|e| e.to_string())?;
        self.runtime
            .block_on(self._transport.peer_paths(peer))
            .map_err(|e| e.to_string())
    }

    /// The native product worker drives the shared line pump on this runtime.
    /// Transport and host accept tasks remain asynchronous.
    pub fn connect(
        runtime: &tokio::runtime::Runtime,
        identity: &dyn IdentityProvider,
        invitation: Invitation,
        card: Card,
    ) -> Result<Self, String> {
        invitation.check()?;
        card.owner.check()?;
        if card.version != 1
            || card.graph != invitation.graph
            || card.owner.binding != invitation.owner.binding
            || identity.master_public_key().to_bytes() != invitation.guest.root()
        {
            return Err("contact or invitation belongs to another graph or person".into());
        }
        let key = identity
            .derive_keypair(TRANSPORT_SALT)
            .map_err(|e| e.to_string())?;
        let signing = personae::InMemoryProvider::from_seed(key.to_seed());
        let transport = runtime.block_on(bind(identity))?;
        let peer = runtime
            .block_on(transport.add_peer_ticket(&card.ticket))
            .map_err(|e| e.to_string())?;
        if peer.to_bytes() != card.owner.peer() {
            return Err("contact ticket names another holder".into());
        }
        let mut nonce = [0; 32];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut nonce);
        let hello = open_session_for(
            &signing,
            NetworkId(network(invitation.owner.root(), invitation.graph)),
            profile(),
            RequestedAction {
                domain: DOMAIN.into(),
                path: path(invitation.graph),
                action: "connect".into(),
            },
            TrafficClass::Interactive,
            nonce,
            &projection_binding(transport.local_peer_id()),
            vec![invitation.grant.clone()],
        )
        .map_err(|e| e.to_string())?;
        let stream = runtime
            .block_on(async {
                tokio::time::timeout(
                    Duration::from_secs(15),
                    dial_projection_session(&*transport, peer, &hello, &Default::default()),
                )
                .await
            })
            .map_err(|_| "connection timed out")?
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("admission refused: {e:?}"))?;
        let mut pump = graphshell_client::pump::LinePump::new(stream);
        let opened = runtime
            .block_on(pump.open(CapabilityProfile::new([
                PresentationCapability::PortableCard,
            ])))
            .map_err(|e| e.to_string())?;
        if opened.descriptor.projections.len() != 1 {
            return Err("holder did not disclose exactly one graph".into());
        }
        let request = opened.descriptor.projections[0].request.clone();
        let mut guest = Self {
            invitation,
            pump,
            runtime: runtime.handle().clone(),
            next_id: 1,
            dirty: false,
            request,
            data: None,
            _transport: transport,
        };
        guest.refresh()?;
        Ok(guest)
    }
    fn request(&mut self, body: CarrierRequestBody) -> Result<CarrierResponseBody, String> {
        let id = self.next_id;
        self.next_id += 1;
        let runtime = self.runtime.clone();
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(15), async {
                let line = chirograph::line::encode(&CarrierRequest { id, body })
                    .map_err(|e| e.to_string())?;
                self.pump.send(&line).await.map_err(|e| e.to_string())?;
                loop {
                    let line = self
                        .pump
                        .recv()
                        .await
                        .map_err(|e| e.to_string())?
                        .ok_or("holder disconnected")?;
                    match chirograph::line::decode::<CarrierOutput>(&line)
                        .map_err(|e| e.to_string())?
                    {
                        Some(CarrierOutput::Notice(_)) => self.dirty = true,
                        Some(CarrierOutput::Response(response)) if response.id == id => {
                            return response.body.map_err(|failure| failure.message);
                        },
                        Some(CarrierOutput::Response(_)) => {
                            return Err("holder answered another request".into());
                        },
                        None => {},
                    }
                }
            })
            .await
            .map_err(|_| "holder response timed out".to_string())?
        })
    }
    /// Consume the host's pushed revision bell; an idle view sends no polls.
    pub fn poll(&mut self) -> Result<(), String> {
        let outcome = (|| {
            if !self.dirty {
                let runtime = self.runtime.clone();
                let line = runtime.block_on(async {
                    tokio::time::timeout(Duration::from_millis(100), self.pump.recv()).await
                });
                match line {
                    Err(_) => return Ok(()),
                    Ok(Err(error)) => return Err(error.to_string()),
                    Ok(Ok(None)) => return Err("holder disconnected".into()),
                    Ok(Ok(Some(line))) => match chirograph::line::decode::<CarrierOutput>(&line)
                        .map_err(|e| e.to_string())?
                    {
                        Some(CarrierOutput::Notice(notice))
                            if notice.session == self.request.session =>
                        {
                            self.dirty = true
                        },
                        None => return Ok(()),
                        _ => return Err("unexpected idle session response".into()),
                    },
                }
            }
            self.refresh()
        })();
        if outcome.is_err() {
            self.data = None;
        }
        outcome
    }
    pub fn refresh(&mut self) -> Result<(), String> {
        self.dirty = false;
        let outcome = (|| {
            let response = self
                .request(CarrierRequestBody::Snapshot(self.request.clone()))
                .map_err(|e| e.to_string())?;
            let CarrierResponseBody::Snapshot(snapshot) = response else {
                return Err("holder did not return the graph".into());
            };
            let offer = snapshot
                .presentation
                .offers
                .values()
                .next()
                .and_then(|o| o.first())
                .ok_or("graph has no presentation")?;
            let response = self
                .request(CarrierRequestBody::Resource(ResourceRequest {
                    session: self.request.session.clone(),
                    resource: offer.resource,
                }))
                .map_err(|e| e.to_string())?;
            let CarrierResponseBody::Resource(resource) = response else {
                return Err("holder did not return graph content".into());
            };
            if ContentHash::of(&resource.bytes) != offer.resource {
                return Err("graph content hash mismatch".into());
            }
            let card: PortableCardV1 =
                serde_json::from_slice(&resource.bytes).map_err(|e| e.to_string())?;
            let value = card
                .values
                .iter()
                .find(|v| v.label == "graphshell.coop/state/v1")
                .ok_or("unknown graph presentation")?;
            let data: Data = serde_json::from_str(&value.value).map_err(|e| e.to_string())?;
            if data.graph != self.invitation.graph
                || data.owner != hex(&self.invitation.owner.root())
                || data.epoch != snapshot.scene.epoch
                || data.revision != snapshot.scene.revision
            {
                return Err("holder returned a different graph or revision".into());
            }
            Ok(data)
        })();
        match outcome {
            Ok(data) => {
                self.data = Some(data);
                Ok(())
            },
            Err(reason) => {
                self.data = None;
                Err(reason)
            },
        }
    }
    pub fn edit(&mut self, edit: Edit) -> Result<(), String> {
        let invocation = IntentInvocation {
            session: self.request.session.clone(),
            target: sceno::InstanceId(0),
            intent: EDIT_INTENT.into(),
            observed_epoch: edit.epoch,
            observed_revision: edit.revision,
            payload: serde_json::to_vec(&edit).map_err(|e| e.to_string())?,
        };
        let response = self
            .request(CarrierRequestBody::Intent(invocation))
            .map_err(|e| e.to_string())?;
        match response {
            CarrierResponseBody::Intent(IntentResult::Accepted) => self.refresh(),
            CarrierResponseBody::Intent(other) => Err(format!("save refused: {other:?}")),
            other => Err(format!("unexpected save response: {other:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_unreachable_configured_relay_cannot_produce_a_lan_only_contact() {
        let identity = personae::InMemoryProvider::from_seed([0x71; 32]);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        // A bound, non-serving listener keeps the test's port from being
        // reused by an actual relay while exercising readiness failure.
        let relay = format!("http://{address}/").parse().unwrap();
        let result = bind_with_relays(&identity, &[relay]).await;
        assert!(matches!(result, Err(error) if error.contains("configured co-op relay")));
        drop(listener);
    }
    #[test]
    fn real_carrier_binds_the_guest_scope_and_revocation() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (mut owner, a, b, _) =
            runtime.block_on(crate::coop::tests::fixture(muniment::MemoryBackend::new()));
        let invitation = runtime
            .block_on(owner.invite(&a, Contact::create(&b).unwrap(), true, now_ms() + 120000))
            .unwrap();
        let shared = Arc::new(Mutex::new(owner));
        let transport = runtime.block_on(bind(&a)).unwrap();
        let contact = runtime
            .block_on(card(&a, invitation.graph, &transport))
            .unwrap();
        let mut host = host(shared.clone()).unwrap();
        let ledger = host.revocations().clone();
        let served = runtime.spawn(async move {
            loop {
                match host.accept_one(&*transport, now_ms).await {
                    Ok(Ok(session)) => {
                        tokio::spawn(async move {
                            let _ = session.finished().await;
                        });
                    },
                    Ok(Err(_)) => {},
                    Err(_) => break,
                }
            }
        });
        let mut guest = Guest::connect(&runtime, &b, invitation.clone(), contact.clone()).unwrap();
        assert_eq!(guest.data.as_ref().unwrap().nodes.len(), 0);
        let edit = guest.data.as_ref().unwrap().edit(Operation::Add {
            id: Uuid::new_v4(),
            address: "https://network.test/shared".into(),
            title: "Guest over admitted carrier".into(),
        });
        guest.edit(edit).unwrap();
        assert_eq!(guest.data.as_ref().unwrap().nodes.len(), 1);
        assert_eq!(
            shared
                .lock()
                .unwrap()
                .graph()
                .unwrap()
                .changes()
                .last()
                .unwrap()
                .author
                .id,
            format!("personae:{}", hex(&b.master_public_key().to_bytes()))
        );
        let edit = shared.lock().unwrap().owner_data().edit(Operation::Add {
            id: Uuid::new_v4(),
            address: "https://network.test/owner-live".into(),
            title: "Pushed holder edit".into(),
        });
        runtime
            .block_on(shared.lock().unwrap().edit_owner(edit))
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while guest.data.as_ref().unwrap().nodes.len() != 2 && std::time::Instant::now() < deadline
        {
            guest.poll().unwrap();
        }
        assert_eq!(
            guest.data.as_ref().unwrap().nodes.len(),
            2,
            "idle guest consumes the pushed revision without a snapshot poll"
        );
        let mut foreign = guest.request.clone();
        foreign.session = ProjectionSession("another-graph".into());
        assert!(
            guest
                .request(CarrierRequestBody::Snapshot(foreign))
                .is_err()
        );
        // A malicious peer bypassing Guest::connect's helpful prechecks still
        // cannot use the invited guest's certificate on its own carrier key.
        let wrong = personae::InMemoryProvider::from_seed([91; 32]);
        let wrong_transport = runtime.block_on(bind(&wrong)).unwrap();
        let peer = runtime
            .block_on(wrong_transport.add_peer_ticket(&contact.ticket))
            .unwrap();
        let key = wrong.derive_keypair(TRANSPORT_SALT).unwrap();
        let signing = personae::InMemoryProvider::from_seed(key.to_seed());
        let hello = open_session_for(
            &signing,
            NetworkId(network(invitation.owner.root(), invitation.graph)),
            profile(),
            RequestedAction {
                domain: DOMAIN.into(),
                path: path(invitation.graph),
                action: "connect".into(),
            },
            TrafficClass::Interactive,
            [71; 32],
            &projection_binding(wrong_transport.local_peer_id()),
            vec![invitation.grant.clone()],
        )
        .unwrap();
        assert!(
            runtime
                .block_on(dial_projection_session(
                    &*wrong_transport,
                    peer,
                    &hello,
                    &Default::default()
                ))
                .unwrap()
                .is_err()
        );
        let key = b.derive_keypair(TRANSPORT_SALT).unwrap();
        let signing = personae::InMemoryProvider::from_seed(key.to_seed());
        let hello = open_session_for(
            &signing,
            NetworkId(network(invitation.owner.root(), invitation.graph)),
            profile(),
            RequestedAction {
                domain: DOMAIN.into(),
                path: path(SessionId::new()),
                action: "connect".into(),
            },
            TrafficClass::Interactive,
            [72; 32],
            &projection_binding(guest._transport.local_peer_id()),
            vec![invitation.grant.clone()],
        )
        .unwrap();
        assert!(
            runtime
                .block_on(dial_projection_session(
                    &*guest._transport,
                    peer,
                    &hello,
                    &Default::default()
                ))
                .unwrap()
                .is_err(),
            "the invited person cannot widen the graph route"
        );
        runtime.block_on(shared.lock().unwrap().revoke(&a)).unwrap();
        let revoked = shared.lock().unwrap().revocation().cloned().unwrap();
        ledger.write().unwrap().fold(revoked.check().unwrap());
        assert!(guest.refresh().is_err());
        assert!(guest.data.is_none());
        drop(guest);
        assert!(Guest::connect(&runtime, &b, invitation, contact).is_err());
        served.abort();
    }
}
