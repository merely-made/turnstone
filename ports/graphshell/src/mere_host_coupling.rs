// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Exact coupling writes use the graph journal, not a second field store.
use super::*;
use mere::coupling_edit::StrengthDraft;
use mere::kernel::graph::{Coupling, CouplingId};

/// The journal channel one coupling's saved strength edits and undo travel on.
pub(crate) fn channel(id: CouplingId) -> String {
    format!("graphshell.coupling/{}", id.as_uuid())
}

impl<B: pandect::graph_session::LegacyTransactionBackend + Clone> MereHost<B> {
    pub async fn save_coupling_strength(
        &mut self,
        draft: &StrengthDraft,
        saved_at_secs: u64,
    ) -> Result<Coupling, String> {
        if self.graph_session.id().0 != draft.graph_id {
            return Err(
                "The strength draft belongs to another graph session; draft retained".into(),
            );
        }
        if self.has_unstored() {
            return Err("Save pending graph edits first; strength draft retained".into());
        }
        let candidate = draft.candidate(self.graph())?;
        if candidate == draft.baseline {
            return Err("No strength change to apply".into());
        }
        let author = author_of(&self.selected_persona, &channel(candidate.id));
        self.graph_session
            .transact_deltas(
                author,
                vec![GraphDelta::AddCoupling {
                    coupling: candidate.clone(),
                }],
                saved_at(saved_at_secs),
            )
            .await
            .map_err(|e| e.to_string())?;
        self.projection_revision = self.projection_revision.wrapping_add(1);
        Ok(candidate)
    }

    pub async fn coupling_history(
        &mut self,
        id: CouplingId,
        redo: bool,
        saved_at_secs: u64,
    ) -> Result<bool, String> {
        if self.has_unstored() {
            return Err("Save pending graph edits before coupling history".into());
        }
        let author = author_of(&self.selected_persona, &channel(id));
        let result = self
            .graph_session
            .transact_history(author, redo, saved_at(saved_at_secs))
            .await
            .map_err(|e| e.to_string())?;
        self.bump_if(result.is_some());
        if let Some(result) = result {
            if !result.kept.is_empty() {
                return Err("History preserved a coupling changed by another author; inspect its current value".into());
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
#[path = "mere_host_coupling_tests.rs"]
mod tests;
