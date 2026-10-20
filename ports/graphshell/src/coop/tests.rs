// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
use super::*;
use muniment::Backend;
use personae::{InMemoryProvider, delegation::Issue};
impl Signer for InMemoryProvider {
    fn certificate(&self, c: DelegationCertificate) -> Result<SignedDelegationCertificate, String> {
        SignedDelegationCertificate::issue(self, c).map_err(|e| e.to_string())
    }
    fn revocation(&self, c: DelegationRevocation) -> Result<SignedDelegationRevocation, String> {
        SignedDelegationRevocation::issue(self, c).map_err(|e| e.to_string())
    }
}
#[test]
fn one_graph_guest_attribution_revoke_and_reopen() {
    pollster::block_on(async {
        let backend = muniment::MemoryBackend::new();
        let a = InMemoryProvider::from_seed([1; 32]);
        let b = InMemoryProvider::from_seed([2; 32]);
        let sessions = pandect::MereSessions::new(backend.clone());
        let mut graph =
            sessions.begin(Author::person(hex(&a.master_public_key().to_bytes())), None);
        graph.flush(SystemTime::now()).await.unwrap();
        let id = graph.id();
        let mut owner = Owner::open(backend.clone(), id, a.master_public_key().to_bytes())
            .await
            .unwrap();
        let invitation = owner
            .invite(&a, Contact::create(&b).unwrap(), true, now_ms() + 60000)
            .await
            .unwrap();
        let data = owner.owner_data();
        let operation = Operation::Add {
            id: Uuid::new_v4(),
            address: "https://example.test/shared".into(),
            title: "Guest source".into(),
        };
        owner
            .edit(b.master_public_key().to_bytes(), data.edit(operation))
            .await
            .unwrap();
        assert_eq!(
            owner.graph().graph().nodes().next().unwrap().1.title,
            "Guest source"
        );
        assert_eq!(
            owner.graph().changes().last().unwrap().author.id,
            format!("personae:{}", hex(&b.master_public_key().to_bytes()))
        );
        owner.revoke(&a).await.unwrap();
        assert!(
            owner
                .guest_data(invitation.guest.peer())
                .unwrap_err()
                .contains("revoked")
        );
        let saved_revision = owner.revision();
        drop(owner);
        let owner = Owner::open(backend, id, a.master_public_key().to_bytes())
            .await
            .unwrap();
        assert_eq!(owner.owner_data().nodes.len(), 1);
        assert_eq!(
            owner.revision(),
            saved_revision,
            "the co-op position follows the stored journal across reopen"
        );
        assert!(owner.guest_data(invitation.guest.peer()).is_err());
        assert!(owner.epoch().0 > data.epoch.0);
    })
}

pub(super) async fn fixture<B: pandect::graph_session::LegacyTransactionBackend + Clone>(
    backend: B,
) -> (Owner<B>, InMemoryProvider, InMemoryProvider, SessionId) {
    let a = InMemoryProvider::from_seed([11; 32]);
    let b = InMemoryProvider::from_seed([12; 32]);
    let mut graph =
        pandect::MereSessions::new(backend.clone()).begin(Author::person("owner"), None);
    graph.flush(SystemTime::now()).await.unwrap();
    let id = graph.id();
    (
        Owner::open(backend, id, a.master_public_key().to_bytes())
            .await
            .unwrap(),
        a,
        b,
        id,
    )
}
fn invocation(data: &Data, session: ProjectionSession, operation: Operation) -> IntentInvocation {
    IntentInvocation {
        session,
        target: sceno::InstanceId(0),
        observed_epoch: data.epoch,
        observed_revision: data.revision,
        intent: EDIT_INTENT.into(),
        payload: serde_json::to_vec(&data.edit(operation)).unwrap(),
    }
}
fn add() -> Operation {
    Operation::Add {
        id: Uuid::new_v4(),
        address: format!("https://shared.test/{}", Uuid::new_v4()),
        title: "Shared source".into(),
    }
}
#[test]
fn exact_graph_identity_payload_and_stale_requests_are_refused_without_writes() {
    pollster::block_on(async {
        let (mut owner, a, b, id) = fixture(muniment::MemoryBackend::new()).await;
        let invitation = owner
            .invite(&a, Contact::create(&b).unwrap(), true, now_ms() + 60000)
            .await
            .unwrap();
        let data = owner.owner_data();
        let shared = Arc::new(Mutex::new(owner));
        let session = ProjectionSession("admitted-guest".into());
        assert!(
            Endpoint::new(
                shared.clone(),
                AdmittedEndpointContext::new(session.clone(), [99; 32])
            )
            .is_err()
        );
        let mut endpoint = Endpoint::new(
            shared.clone(),
            AdmittedEndpointContext::new(session.clone(), invitation.guest.peer()),
        )
        .unwrap();
        let mut request = endpoint.request();
        request.session = ProjectionSession("other-graph".into());
        assert!(endpoint.snapshot(request).is_err());
        let mut edit = data.edit(add());
        edit.graph = SessionId::new();
        let mut call = invocation(&data, session.clone(), add());
        call.payload = serde_json::to_vec(&edit).unwrap();
        assert!(matches!(
            endpoint.invoke(call).unwrap(),
            IntentResult::Rejected { .. }
        ));
        let mut call = invocation(&data, session.clone(), add());
        let mut payload: serde_json::Value = serde_json::from_slice(&call.payload).unwrap();
        payload["author"] = serde_json::json!(hex(&a.master_public_key().to_bytes()));
        call.payload = serde_json::to_vec(&payload).unwrap();
        assert!(matches!(
            endpoint.invoke(call).unwrap(),
            IntentResult::Rejected { .. }
        ));
        let call = invocation(&data, session.clone(), add());
        assert_eq!(
            endpoint.invoke(call.clone()).unwrap(),
            IntentResult::Accepted
        );
        assert!(matches!(
            endpoint.invoke(call).unwrap(),
            IntentResult::Rejected { .. }
        ));
        let owner = shared.lock().unwrap();
        assert_eq!(owner.graph_id(), id);
        assert_eq!(owner.owner_data().nodes.len(), 1);
        assert_eq!(
            owner.graph().changes().last().unwrap().author.id,
            format!("personae:{}", hex(&b.master_public_key().to_bytes()))
        );
    })
}
#[test]
fn reader_and_revoked_guest_cannot_fetch_or_mutate() {
    pollster::block_on(async {
        let (mut owner, a, b, _) = fixture(muniment::MemoryBackend::new()).await;
        let invitation = owner
            .invite(&a, Contact::create(&b).unwrap(), false, now_ms() + 60000)
            .await
            .unwrap();
        let data = owner.owner_data();
        let shared = Arc::new(Mutex::new(owner));
        let session = ProjectionSession("reader".into());
        let mut endpoint = Endpoint::new(
            shared.clone(),
            AdmittedEndpointContext::new(session.clone(), invitation.guest.peer()),
        )
        .unwrap();
        let snapshot = endpoint.snapshot(endpoint.request()).unwrap();
        let hash = snapshot.presentation.offers.values().next().unwrap()[0].resource;
        assert!(
            matches!(endpoint.invoke(invocation(&data,session,add())).unwrap(),IntentResult::Rejected{reason} if reason.contains("read-only"))
        );
        shared.lock().unwrap().revoke(&a).await.unwrap();
        assert!(
            endpoint
                .snapshot(endpoint.request())
                .unwrap_err()
                .contains("revoked")
        );
        assert!(
            endpoint
                .resource(ResourceRequest {
                    session: endpoint.context.session().clone(),
                    resource: hash
                })
                .is_err()
        );
        assert!(endpoint.poll_notice().is_err());
        assert_eq!(shared.lock().unwrap().owner_data().nodes.len(), 0);
    })
}
#[derive(Clone)]
struct Refusing {
    inner: muniment::MemoryBackend,
    fail: Arc<std::sync::atomic::AtomicBool>,
}
#[async_trait::async_trait]
impl muniment::Backend for Refusing {
    async fn get(&self, k: &str) -> Result<Option<Vec<u8>>, muniment::StoreError> {
        self.inner.get(k).await
    }
    async fn put(&self, k: &str, b: &[u8]) -> Result<(), muniment::StoreError> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(muniment::StoreError::Backend(
                "injected storage refusal".into(),
            ));
        }
        self.inner.put(k, b).await
    }
    async fn delete(&self, k: &str) -> Result<(), muniment::StoreError> {
        self.inner.delete(k).await
    }
    async fn list(&self, k: &str) -> Result<Vec<String>, muniment::StoreError> {
        self.inner.list(k).await
    }
    async fn scan(&self, a: &str, b: &str) -> Result<Vec<String>, muniment::StoreError> {
        self.inner.scan(a, b).await
    }
    async fn apply(&self, ops: &[muniment::WriteOp]) -> Result<(), muniment::StoreError> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(muniment::StoreError::Backend(
                "injected storage refusal".into(),
            ));
        }
        self.inner.apply(ops).await
    }
    async fn transact(&self, f: muniment::TransactFn) -> Result<(), muniment::StoreError> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(muniment::StoreError::Backend(
                "injected storage refusal".into(),
            ));
        }
        self.inner.transact(f).await
    }
}
#[test]
fn storage_refusal_keeps_graph_authors_revision_and_revocation_then_retry() {
    pollster::block_on(async {
        use std::sync::atomic::Ordering::SeqCst;
        let backend = Refusing {
            inner: muniment::MemoryBackend::new(),
            fail: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        let (mut owner, a, b, id) = fixture(backend.clone()).await;
        let invitation = owner
            .invite(&a, Contact::create(&b).unwrap(), true, now_ms() + 60000)
            .await
            .unwrap();
        let data = owner.owner_data();
        let edit = data.edit(add());
        backend.fail.store(true, SeqCst);
        assert!(
            owner
                .edit_owner(edit.clone())
                .await
                .unwrap_err()
                .contains("injected")
        );
        assert_eq!(owner.revision(), data.revision);
        assert_eq!(owner.owner_data().nodes.len(), 0);
        assert_eq!(owner.owner_data().authors.len(), data.authors.len());
        assert!(owner.revoke(&a).await.is_err());
        assert!(owner.guest_data(invitation.guest.peer()).is_ok());
        backend.fail.store(false, SeqCst);
        owner.edit_owner(edit).await.unwrap();
        owner.revoke(&a).await.unwrap();
        drop(owner);
        let owner = Owner::open(backend, id, a.master_public_key().to_bytes())
            .await
            .unwrap();
        assert_eq!(owner.owner_data().nodes.len(), 1);
        assert!(owner.guest_data(invitation.guest.peer()).is_err());
    })
}
#[test]
fn private_sibling_graph_bytes_are_unchanged() {
    pollster::block_on(async {
        use muniment::Backend;
        let backend = muniment::MemoryBackend::new();
        let (mut owner, _, _, _) = fixture(backend.clone()).await;
        let mut private =
            pandect::MereSessions::new(backend.clone()).begin(Author::person("owner"), None);
        private.flush(SystemTime::now()).await.unwrap();
        let prefix = format!("sessions/{}/", private.id().as_uuid());
        let keys = backend.list(&prefix).await.unwrap();
        let mut before = vec![];
        for k in &keys {
            before.push(backend.get(k).await.unwrap());
        }
        owner
            .edit_owner(owner.owner_data().edit(add()))
            .await
            .unwrap();
        for (k, bytes) in keys.iter().zip(before) {
            assert_eq!(backend.get(k).await.unwrap(), bytes);
        }
        assert_eq!(
            GraphSession::open(backend, private.id())
                .await
                .unwrap()
                .graph()
                .node_count(),
            0
        );
    })
}
#[test]
fn contact_and_invitation_cannot_be_retargeted() {
    pollster::block_on(async {
        let (mut owner, a, b, _) = fixture(muniment::MemoryBackend::new()).await;
        let i = owner
            .invite(&a, Contact::create(&b).unwrap(), true, now_ms() + 60000)
            .await
            .unwrap();
        let mut tampered = i.clone();
        tampered.graph = SessionId::new();
        assert!(tampered.check().is_err());
        tampered = i.clone();
        tampered.guest = Contact::create(&InMemoryProvider::from_seed([13; 32])).unwrap();
        assert!(tampered.check().is_err());
        assert!(
            Owner::open(owner.backend.clone(), owner.graph_id(), [88; 32])
                .await
                .is_err()
        );
    })
}
#[test]
fn export_retry_reuses_exact_grant_and_write_permission_is_signed() {
    pollster::block_on(async {
        let (mut owner, a, b, _) = fixture(muniment::MemoryBackend::new()).await;
        let guest = Contact::create(&b).unwrap();
        let first = owner
            .invite(&a, guest.clone(), true, now_ms() + 60000)
            .await
            .unwrap();
        let second = owner.invite(&a, guest, false, now_ms() + 60000).await;
        assert!(second.is_err());
        let second = owner
            .invite(&a, first.guest.clone(), true, now_ms() + 60000)
            .await
            .unwrap();
        assert_eq!(first.grant, second.grant);
        let mut altered = first;
        altered.write = false;
        assert!(altered.check().is_err());
    })
}
