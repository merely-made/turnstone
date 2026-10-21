// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Current owner actions on a locally compiled projection.
//!
//! A host that already knows the owner supplies an exact occurrence-to-target
//! mapping. Source names and imported datasets cannot discover or authenticate
//! an owner. This adapter borrows that endpoint's protocol core for its entire
//! lifetime; recipes, snapshots and the compiler gain no action authority.
//! The view supplies its existing dataset and compiled selection directly, so
//! browser and native controls need no additional editor/session wrapper.

use std::collections::BTreeMap;

use chirograph::{AdvertisedAction, ProjectionSession as OwnerSession, Revision, SceneEpoch};
use graphshell_client::{ActionDraft, ActionDraftTarget, Outcome, Progress, SessionCore};
use sceno::InstanceId;
use scenograph::RuntimeSourceBinding;

use scenomise::projection::{CompiledProjection, ProjectionDataset};

/// An explicitly supplied runtime association, scoped to one endpoint and one
/// observed projection. No serialization, source-name lookup or first-match
/// source lookup is provided. Repeated appearances must be mapped separately.
pub struct BoundOwnerActions<'a> {
    binding: RuntimeSourceBinding<BTreeMap<String, InstanceId>>,
    owner: &'a mut SessionCore,
    session: OwnerSession,
    epoch: SceneEpoch,
    revision: Revision,
}

impl<'a> BoundOwnerActions<'a> {
    /// The integrating host must obtain `binding` from its known owner, never
    /// from an imported recipe or a matching authority/domain/resource string.
    /// The mapping uses the owner's item IDs, not the compiler's item IDs.
    pub fn new(
        binding: RuntimeSourceBinding<BTreeMap<String, InstanceId>>,
        owner: &'a mut SessionCore,
        session: &OwnerSession,
    ) -> Result<Self, String> {
        let ack = owner
            .client()
            .acknowledgement(session)
            .ok_or("owner projection is unavailable")?;
        Ok(Self {
            binding,
            owner,
            session: session.clone(),
            epoch: ack.epoch,
            revision: ack.revision,
        })
    }

    fn selected_target(
        &self,
        dataset: &ProjectionDataset,
        compiled: &CompiledProjection,
    ) -> Result<ActionDraftTarget, String> {
        if self.binding.source() != &dataset.source {
            return Err("view is bound to a different source".into());
        }
        let selected = compiled
            .selected
            .and_then(|instance| compiled.occurrence_by_instance.get(&instance))
            .ok_or("select a resolved item to see owner actions")?;
        let target = self
            .binding
            .with_witness(|targets| targets.get(selected).copied())
            .ok_or("owner has not supplied an exact action target for this occurrence")?;
        let occurrence = dataset
            .occurrences
            .iter()
            .find(|occurrence| occurrence.occurrence_id == *selected)
            .ok_or("selected occurrence is unavailable")?;
        let shown_source = compiled
            .selected
            .and_then(|instance| compiled.scene.items.get(instance.0 as usize))
            .and_then(|item| compiled.scene.sources.get(item.source.0 as usize));
        if shown_source != Some(&occurrence.source) {
            return Err("compiled selection and disclosed source no longer agree".into());
        }
        let mounted = self
            .owner
            .client()
            .mounted(&self.session)
            .ok_or("owner projection is unavailable")?;
        // A host mapping is scoped to the observation at which it was issued.
        // Do not silently attach an old mapping to a new/reused owner item.
        if mounted.scene.epoch != self.epoch || mounted.scene.revision != self.revision {
            return Err("owner projection changed; obtain a fresh action binding".into());
        }
        let source = mounted
            .scene
            .active_item(target)
            .and_then(|item| mounted.scene.tables.sources.get(item.source.0 as usize))
            .and_then(Option::as_ref);
        if source != Some(&occurrence.source) {
            return Err("owner action target does not name the selected source".into());
        }
        Ok(ActionDraftTarget {
            session: self.session.clone(),
            target,
            observed_epoch: self.epoch,
            observed_revision: self.revision,
        })
    }

    /// Only the exact selected owner's supported, current offers. No
    /// viewer-local actions or other occurrences' same-named actions enter it.
    pub fn actions(
        &self,
        dataset: &ProjectionDataset,
        compiled: &CompiledProjection,
    ) -> Result<Vec<AdvertisedAction>, String> {
        let target = self.selected_target(dataset, compiled)?;
        self.owner.current_actions(&target.session, target.target)
    }

    /// Use the existing bounded form and opaque values unchanged, including
    /// ordinary actions without a form. Opening a draft invokes nothing.
    pub fn open_action(
        &self,
        dataset: &ProjectionDataset,
        compiled: &CompiledProjection,
        intent: &str,
    ) -> Result<(ActionDraftTarget, ActionDraft), String> {
        let target = self.selected_target(dataset, compiled)?;
        let action = self
            .actions(dataset, compiled)?
            .into_iter()
            .find(|action| action.intent.0 == intent)
            .ok_or("owner does not currently offer that action on the selected occurrence")?;
        Ok((target, ActionDraft::new(action)))
    }

    /// Recheck selection and the current offer, then use the same endpoint's
    /// existing command/wire path. The endpoint still decides authorization
    /// and freshness; an accepted dispatch is not terminal completion.
    pub fn submit_action(
        &mut self,
        dataset: &ProjectionDataset,
        compiled: &CompiledProjection,
        target: &ActionDraftTarget,
        draft: &mut ActionDraft,
    ) -> Result<Progress<Outcome>, String> {
        if &self.selected_target(dataset, compiled)? != target {
            return Err("action target changed; reopen the action for the selected item".into());
        }
        self.owner.submit_action_draft(target, draft)
    }
}
