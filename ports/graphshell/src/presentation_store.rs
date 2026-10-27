// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Graph-scoped Muniment record and compare-and-replace adapter for the pure
//! Scenograph presentation rule workspace.
use std::sync::{Arc, Mutex};

use muniment::{Backend, WriteOp};
use scenograph::presentation::editor::{
    OperationId, PreparedOperation, RuleWorkspace, SaveKind, SavedState, parse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const RECORD_LIMIT: usize = 1024 * 1024;
const HISTORY_LIMIT: usize = 32;

#[cfg(not(target_arch = "wasm32"))]
pub trait RuleStore: Backend + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<B: Backend + Sync> RuleStore for B {}
#[cfg(target_arch = "wasm32")]
pub trait RuleStore: Backend {}
#[cfg(target_arch = "wasm32")]
impl<B: Backend> RuleStore for B {}

pub fn slot(graph_id: Uuid) -> String {
    format!("graphshell/presentation-rules/v1/{graph_id}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerObservation {
    NoProduct,
    TemporarilyUnavailable,
    Graph(Uuid),
}
pub fn binding_changed(current: Option<Uuid>, observed: OwnerObservation) -> bool {
    match observed {
        OwnerObservation::NoProduct => current.is_some(),
        OwnerObservation::TemporarilyUnavailable => false,
        OwnerObservation::Graph(graph_id) => current != Some(graph_id),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u16,
    graph_id: Uuid,
    generation: u64,
    rules: scenograph::presentation::RuleSet,
    undo: Vec<scenograph::presentation::RuleSet>,
    redo: Vec<scenograph::presentation::RuleSet>,
}
impl Record {
    fn from_state(graph_id: Uuid, state: &SavedState) -> Self {
        Self {
            version: 1,
            graph_id,
            generation: state.generation,
            rules: state.rules.clone(),
            undo: state.undo.clone(),
            redo: state.redo.clone(),
        }
    }
    fn into_state(self, graph_id: Uuid) -> Result<SavedState, String> {
        if self.version != 1
            || self.graph_id != graph_id
            || self.undo.len() > HISTORY_LIMIT
            || self.redo.len() > HISTORY_LIMIT
        {
            return Err(
                "invalid presentation workspace identity, version or history length".into(),
            );
        }
        let state = SavedState {
            generation: self.generation,
            rules: self.rules,
            undo: self.undo,
            redo: self.redo,
        };
        state.validate()?;
        Ok(state)
    }
}

pub struct Loaded {
    pub graph_id: Uuid,
    pub baseline: Option<Vec<u8>>,
    pub workspace: RuleWorkspace,
}
pub async fn load<B: Backend>(backend: B, graph_id: Uuid) -> Result<Loaded, String> {
    let baseline = backend
        .get(&slot(graph_id))
        .await
        .map_err(|e| e.to_string())?;
    let state = match &baseline {
        None => SavedState::empty(),
        Some(bytes) => {
            if bytes.len() > RECORD_LIMIT {
                return Err("presentation workspace exceeds 1 MiB".into());
            }
            let text = std::str::from_utf8(bytes)
                .map_err(|e| format!("Invalid presentation workspace: {e}"))?;
            let record: Record = parse(text, "workspace")
                .map_err(|e| format!("Invalid presentation workspace: {e:?}"))?;
            record.into_state(graph_id)?
        },
    };
    let workspace = RuleWorkspace::new(state)?;
    Ok(Loaded {
        graph_id,
        baseline,
        workspace,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveFailure {
    Conflict(String),
    Failed(String),
}

#[derive(Clone)]
pub struct PreparedSave {
    graph_id: Uuid,
    id: OperationId,
    expected: Option<Vec<u8>>,
    bytes: Vec<u8>,
}
impl PreparedSave {
    pub fn id(&self) -> OperationId {
        self.id
    }
    pub fn graph_id(&self) -> Uuid {
        self.graph_id
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub async fn save<B: RuleStore>(&self, backend: B) -> Result<Vec<u8>, SaveFailure> {
        let key = slot(self.graph_id);
        let expected = self.expected.clone();
        let bytes = self.bytes.clone();
        let outcome = Arc::new(Mutex::new(None));
        let answer = outcome.clone();
        backend.transact(Box::new(move |reader| {
            let result = reader.get(&key).map_err(|e| SaveFailure::Failed(e.to_string())).and_then(|held| {
                if held == expected { Ok(()) } else { Err(SaveFailure::Conflict("Saved rules changed elsewhere; draft retained. Reload latest rules and review before applying.".into())) }
            });
            let accepted = result.is_ok();
            *answer.lock().expect("transaction result") = Some(result);
            if accepted { vec![WriteOp::Put { key, value: bytes }] } else { Vec::new() }
        })).await.map_err(|e| SaveFailure::Failed(e.to_string()))?;
        outcome
            .lock()
            .expect("transaction result")
            .take()
            .ok_or_else(|| SaveFailure::Failed("backend did not run the transaction".into()))?
            .map(|()| self.bytes.clone())
    }
}

/// Prepare a core transition and its graph-scoped bytes as one adapter step.
/// If encoding refuses the record, the core pending transition is failed too.
pub fn prepare_save(
    workspace: &mut RuleWorkspace,
    graph_id: Uuid,
    expected: Option<Vec<u8>>,
    kind: SaveKind,
) -> Result<PreparedSave, String> {
    let operation: PreparedOperation = workspace.prepare(kind)?;
    let result = serde_json::to_vec(&Record::from_state(graph_id, &operation.state))
        .map_err(|e| e.to_string())
        .and_then(|bytes| {
            if bytes.len() > RECORD_LIMIT {
                Err("saved rule history exceeds 1 MiB; draft retained".into())
            } else {
                Ok(bytes)
            }
        });
    match result {
        Ok(bytes) => Ok(PreparedSave {
            graph_id,
            id: operation.id,
            expected,
            bytes,
        }),
        Err(error) => {
            let _ = workspace.finish(operation.id, Err(error.clone()));
            Err(error)
        },
    }
}

#[cfg(test)]
#[path = "presentation_store_tests.rs"]
mod tests;
