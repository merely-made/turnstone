// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Workshop proposals enter the existing Graphshell owner, never another writer.

use super::*;
use alembic::workshop::{Acceptance, Owner, Passage, Proposal, Receipt};
use athanor::workshop_destination as destination;
use servitor::AuthorityProvider;

impl<B: pandect::graph_session::LegacyTransactionBackend + Clone> MereHost<B> {
    /// The workshop creates a recorded graph only when the store has never
    /// held a session. Replace the unpersisted empty placeholder without
    /// retiring it: saving two fresh sessions would make latest-live ambiguous.
    pub async fn workshop_initialize_recorded(&mut self) -> Result<(), MereHostError> {
        if self.reopened {
            return Ok(());
        }
        if !self.sessions.list().await?.is_empty()
            || !self.retired.is_empty()
            || self.graph().nodes().next().is_some()
        {
            return Err(MereHostError::OwnershipUnproven);
        }
        self.graph_session = self.sessions.begin_recorded(self.author(), None);
        Ok(())
    }
    pub fn workshop_owner(&self) -> Owner {
        Owner {
            host: "graphshell".into(),
            workspace: self
                .graph_session
                .manifest()
                .root_graph_id
                .as_uuid()
                .to_string(),
            session: self.graph_session.id().as_uuid().to_string(),
        }
    }
    pub fn workshop_revision(&self) -> Result<String, athanor::workshop::Error> {
        destination::revision(&self.graph_session)
    }

    pub fn workshop_capture_public(
        &self,
        surface: uuid::Uuid,
        start: u64,
        end: u64,
        public: impl FnOnce(uuid::Uuid, uuid::Uuid) -> bool,
    ) -> Result<Passage, athanor::workshop::Error> {
        destination::capture_public_source(
            &self.graph_session,
            &self.workshop_owner(),
            surface,
            start,
            end,
            public,
        )
    }
    pub async fn workshop_query(
        &self,
        acceptance: &Acceptance,
        proposed: &Proposal,
    ) -> Result<Option<Receipt>, athanor::workshop::Error> {
        destination::query(
            &self.graph_session,
            &self.workshop_owner(),
            acceptance,
            proposed,
        )
        .await
    }
    /// Recover both the receipt and the canonical graph after a lost storage
    /// reply. The caller retains its sole-writer lease across this operation.
    pub async fn workshop_reconcile(
        &mut self,
        acceptance: &Acceptance,
        proposed: &Proposal,
    ) -> Result<Option<Receipt>, athanor::workshop::Error> {
        let Some(expected) = self.workshop_query(acceptance, proposed).await? else {
            return Ok(None);
        };
        let Some((_, current_generation)) = self
            .graph_session
            .reconcile_operation(acceptance.operation)
            .await
            .map_err(|error| athanor::workshop::Error::Store(error.to_string()))?
        else {
            return Err(athanor::workshop::Error::Conflict);
        };
        let recovered = self.workshop_query(acceptance, proposed).await?;
        if recovered.as_ref() != Some(&expected) {
            return Err(athanor::workshop::Error::Conflict);
        }
        self.resident_generation = Some(current_generation);
        self.projection_revision = self.projection_revision.wrapping_add(1);
        Ok(recovered)
    }

    pub async fn workshop_commit(
        &mut self,
        acceptance: &Acceptance,
        proposed: &Proposal,
        human: [u8; 32],
        authority: &impl AuthorityProvider,
        now_ms: u64,
        custody_open: bool,
    ) -> Result<Receipt, athanor::workshop::Error> {
        if !self.resident_publication || human != acceptance.human {
            return Err(athanor::workshop::Error::Unauthorized);
        }
        // A prior target write may have committed even when its acknowledgement
        // was lost. Recover the canonical owner before returning its receipt;
        // otherwise a queued duplicate could acknowledge a note absent from the
        // live graph. Reconciliation keeps the current publication generation.
        if self.workshop_query(acceptance, proposed).await?.is_some() {
            return self
                .workshop_reconcile(acceptance, proposed)
                .await?
                .ok_or(athanor::workshop::Error::Conflict);
        }
        let before = self
            .graph_session
            .operation_revision()
            .map_err(|error| athanor::workshop::Error::Store(error.to_string()))?;
        let owner = self.workshop_owner();
        let receipt = destination::commit(
            &mut self.graph_session,
            &owner,
            acceptance,
            proposed,
            human,
            authority,
            now_ms,
            custody_open,
        )
        .await?;
        if self
            .graph_session
            .operation_revision()
            .map_err(|error| athanor::workshop::Error::Store(error.to_string()))?
            != before
        {
            self.resident_generation = Some(receipt.storage_witness.clone());
            self.projection_revision = self.projection_revision.wrapping_add(1);
        }
        Ok(receipt)
    }

    pub async fn workshop_seed(&mut self, deltas: Vec<GraphDelta>) -> Result<(), MereHostError> {
        if !self.resident_publication {
            return Err(MereHostError::OwnershipUnproven);
        }
        let author = author_of(&self.selected_persona, "alembic-public-example");
        let (_, generation) = self
            .graph_session
            .transact_deltas_published(author, deltas, wall_clock_now())
            .await?;
        self.resident_generation = Some(generation);
        self.projection_revision = self.projection_revision.wrapping_add(1);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alembic::workshop::{Destination, Owner, Passage, Review};
    use athanor::workshop::{Workshop, evaluate};
    use muniment::{MemoryBackend, TransactFn, WriteOp};
    use servitor::{Cap, Mode, Subject};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use uuid::Uuid;

    #[derive(Clone)]
    struct LostAckBackend {
        store: MemoryBackend,
        fail_after_commit: Arc<AtomicBool>,
    }
    #[async_trait::async_trait]
    impl Backend for LostAckBackend {
        async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
            self.store.get(key).await
        }
        async fn put(&self, key: &str, value: &[u8]) -> Result<(), StoreError> {
            self.store.put(key, value).await
        }
        async fn delete(&self, key: &str) -> Result<(), StoreError> {
            self.store.delete(key).await
        }
        async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
            self.store.list(prefix).await
        }
        async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
            self.store.scan(start, end).await
        }
        async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
            self.store.apply(ops).await
        }
        async fn transact(&self, transaction: TransactFn) -> Result<(), StoreError> {
            self.store.transact(transaction).await?;
            if self.fail_after_commit.swap(false, Ordering::SeqCst) {
                return Err(StoreError::NotTransactional);
            }
            Ok(())
        }
    }
    struct Granted;
    impl AuthorityProvider for Granted {
        fn covers(&self, _: Subject, _: &Cap, _: Mode) -> bool {
            true
        }
    }

    #[test]
    fn duplicate_accept_after_committed_but_lost_ack_reloads_live_owner() {
        pollster::block_on(async {
            let backend = LostAckBackend {
                store: MemoryBackend::new(),
                fail_after_commit: Arc::new(AtomicBool::new(false)),
            };
            let mut host = MereHost::open(
                backend.clone(),
                SelectedPersonaRef {
                    persona: "public".into(),
                    profile: "test".into(),
                },
                fixture_handlers(),
                AccessContext {
                    persona: "public".into(),
                    device: "test".into(),
                    at_ms: 0,
                },
            )
            .await
            .unwrap();
            host.workshop_initialize_recorded().await.unwrap();
            let mut host = host.with_resident_publication();
            host.persist(1).await.unwrap();
            let first = Uuid::from_u128(401);
            let second = Uuid::from_u128(402);
            let mut deltas = Vec::new();
            for (id, title, text) in [
                (first, "First", "first passage"),
                (second, "Second", "second passage"),
            ] {
                deltas.extend([
                    GraphDelta::AddNode {
                        id: Some(id),
                        url: format!("https://public.example/{id}"),
                        position: (0.0, 0.0).into(),
                    },
                    GraphDelta::ReplaySetNodeTitleById {
                        node_id: id,
                        title: title.into(),
                    },
                    GraphDelta::ReplaySetNodeBodyById {
                        node_id: id,
                        body: Some(text.into()),
                    },
                ]);
            }
            host.workshop_seed(deltas).await.unwrap();
            let owner: Owner = host.workshop_owner();
            let sources: Vec<Passage> = [(first, "first passage"), (second, "second passage")]
                .into_iter()
                .map(|(id, text)| {
                    host.workshop_capture_public(id, 0, text.len() as u64, |_, _| true)
                        .unwrap()
                })
                .collect();
            let workshop = Workshop::new(MemoryBackend::new(), owner.clone(), [3; 16]);
            workshop.install([7; 32]).await.unwrap();
            workshop.begin(1, sources, &Granted, 10).await.unwrap();
            for step in 0..3 {
                let dispatch = workshop.dispatch(1, &Granted, 11 + step * 2).await.unwrap();
                workshop
                    .observe(evaluate(dispatch).unwrap(), &Granted, 12 + step * 2)
                    .await
                    .unwrap();
            }
            let proposal = workshop.view(Some(1)).await.unwrap().proposal.unwrap();
            let destination = Destination {
                owner,
                graph: host.workshop_owner().workspace,
                revision: host.workshop_revision().unwrap(),
            };
            let acceptance = workshop
                .accept(1, destination.clone(), [8; 32], &Granted, 20)
                .await
                .unwrap();
            let original_count = host.graph().node_count();
            let before_generation = host.resident_generation.clone();
            backend.fail_after_commit.store(true, Ordering::SeqCst);
            assert!(
                host.workshop_commit(&acceptance, &proposal, [8; 32], &Granted, 21, true)
                    .await
                    .is_err()
            );
            assert_eq!(
                host.graph().node_count(),
                original_count,
                "lost acknowledgement leaves the original in-memory writer unchanged"
            );
            assert_eq!(host.resident_generation, before_generation);
            let durable = host
                .workshop_query(&acceptance, &proposal)
                .await
                .unwrap()
                .unwrap();
            workshop.uncertain(1).await.unwrap();
            assert!(matches!(
                workshop.view(Some(1)).await.unwrap().review,
                Some(Review::Reconciling(_))
            ));
            let queued_duplicate = workshop
                .accept(1, destination, [8; 32], &Granted, 22)
                .await
                .unwrap();
            assert_eq!(queued_duplicate, acceptance);
            let recovered = host
                .workshop_commit(&queued_duplicate, &proposal, [8; 32], &Granted, 22, true)
                .await
                .unwrap();
            assert_eq!(recovered, durable);
            assert_eq!(
                host.graph().node_count(),
                original_count + 1,
                "receipt recovery must refresh the live owner before acknowledging"
            );
            let surface = destination::result_surface(&acceptance);
            assert!(host.graph().get_node_by_id(surface).is_some());
            assert_eq!(
                host.resident_generation.as_deref(),
                Some(recovered.storage_witness.as_str())
            );
            workshop.acknowledge(1, recovered.clone()).await.unwrap();
            assert!(matches!(
                workshop.view(Some(1)).await.unwrap().review,
                Some(Review::Added { .. })
            ));

            // A later graph publication must not be rolled back to the older
            // operation's generation by another duplicate acceptance.
            host.workshop_seed(vec![GraphDelta::AddNode {
                id: Some(Uuid::from_u128(403)),
                url: "https://public.example/later".into(),
                position: (0.0, 0.0).into(),
            }])
            .await
            .unwrap();
            let current_generation = host.resident_generation.clone();
            let current_nodes = host.graph().node_count();
            let old = host
                .workshop_commit(&acceptance, &proposal, [8; 32], &Granted, u64::MAX, false)
                .await
                .unwrap();
            assert_eq!(old, recovered);
            assert_eq!(host.resident_generation, current_generation);
            assert_eq!(host.graph().node_count(), current_nodes);
        });
    }
}
