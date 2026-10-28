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
        if !self.resident_publication {
            return Err(athanor::workshop::Error::Unauthorized);
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
