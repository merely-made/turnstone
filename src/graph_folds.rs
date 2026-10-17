// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Session-local native fold records, separate from graph truth and layout.
use std::path::Path;

use curation::FoldRecord;
use serde_json::{Value, json};

use crate::panes::GraphId;

pub(crate) fn scope(graph: GraphId) -> String {
    format!("turnstone:graph:{}", graph.as_uuid())
}

#[derive(Default)]
pub(crate) struct FoldSidecar {
    /// Unreadable/future envelope: ordinary saves must leave its bytes intact.
    pub(crate) read_only: bool,
    /// Stale, foreign, overlapping or future records survive ordinary saves.
    retained: Vec<Value>,
}

impl FoldSidecar {
    pub(crate) fn restore(dir: &Path, graph: GraphId, canvas: &mut mere::canvas::Canvas) -> Self {
        canvas.restore_folds(vec![]);
        let path = dir.join("folds.json");
        if let Err(error) = crate::session_persistence::require_complete_file(&path) {
            tracing::warn!(%error, "incomplete folds replacement preserved");
            return Self {
                read_only: true,
                retained: vec![],
            };
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(error) => {
                tracing::warn!(%error, "cannot read folds; preserving sidecar");
                return Self {
                    read_only: true,
                    retained: vec![],
                };
            },
        };
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
            return Self {
                read_only: true,
                retained: vec![],
            };
        };
        if value.get("version") != Some(&json!(1))
            || !value.get("records").is_some_and(Value::is_array)
        {
            return Self {
                read_only: true,
                retained: vec![],
            };
        }
        let mut state = Self::default();
        let mut records = Vec::new();
        for raw in value["records"].as_array().expect("checked array") {
            match serde_json::from_value::<FoldRecord>(raw.clone()) {
                Ok(record) if record.source_scope == scope(graph) && record.validate().is_ok() => {
                    records.push(record)
                },
                _ => state.retained.push(raw.clone()),
            }
        }
        state
            .retained
            .extend(canvas.restore_folds(records).into_iter().map(|r| json!(r)));
        if !state.retained.is_empty() {
            tracing::warn!(
                count = state.retained.len(),
                "saved folds refused; records retained intact"
            );
        }
        state
    }

    pub(crate) fn save(&self, dir: &Path, records: &[FoldRecord]) -> Result<(), String> {
        if self.read_only {
            return Err("unreadable fold sidecar preserved".into());
        }
        let mut records: Vec<Value> = records.iter().map(|record| json!(record)).collect();
        records.extend(self.retained.iter().cloned());
        let bytes = serde_json::to_vec_pretty(&json!({"version": 1, "records": records}))
            .map_err(|e| e.to_string())?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let tmp = dir.join("folds.json.tmp");
        std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(tmp, dir.join("folds.json")).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, GraphId, mere::canvas::Canvas) {
        let dir = tempfile::tempdir().unwrap();
        let graph = GraphId::from_uuid(uuid::Uuid::new_v4());
        let mut canvas = mere::canvas::Canvas::new();
        canvas.visit("mere://fold-a");
        canvas.visit("mere://fold-b");
        let members: Vec<_> = canvas.graph().nodes().map(|(_, n)| n.id).collect();
        canvas.set_selected_members(&members);
        assert!(canvas.fold_selected(scope(graph)).is_some());
        (dir, graph, canvas)
    }
    #[test]
    fn native_records_roundtrip_and_expand_stays_view_only() {
        let (dir, graph, mut canvas) = fixture();
        let expected = canvas.fold_records().to_vec();
        FoldSidecar::default().save(dir.path(), &expected).unwrap();
        canvas.restore_folds(vec![]);
        let state = FoldSidecar::restore(dir.path(), graph, &mut canvas);
        assert_eq!(canvas.fold_records(), expected);
        let before = canvas.graph().nodes().count();
        assert!(canvas.expand_selected_fold());
        state.save(dir.path(), canvas.fold_records()).unwrap();
        assert_eq!(canvas.graph().nodes().count(), before);
        assert!(
            FoldSidecar::restore(dir.path(), graph, &mut canvas)
                .retained
                .is_empty()
        );
        assert!(canvas.fold_records().is_empty());
    }
    #[test]
    fn corrupt_and_future_envelopes_survive_saves_byte_for_byte() {
        let (dir, graph, mut canvas) = fixture();
        for raw in [b"not json".as_slice(), br#"{"version":99,"records":[]}"#] {
            std::fs::write(dir.path().join("folds.json"), raw).unwrap();
            let state = FoldSidecar::restore(dir.path(), graph, &mut canvas);
            assert!(state.read_only);
            assert!(state.save(dir.path(), &[]).is_err());
            assert_eq!(std::fs::read(dir.path().join("folds.json")).unwrap(), raw);
        }
    }
    #[test]
    fn interrupted_replacement_is_preserved() {
        let (dir, graph, mut canvas) = fixture();
        std::fs::write(dir.path().join("folds.json.tmp"), b"interrupted evidence").unwrap();
        let state = FoldSidecar::restore(dir.path(), graph, &mut canvas);
        assert!(state.read_only);
        assert!(state.save(dir.path(), &[]).is_err());
        assert!(!dir.path().join("folds.json").exists());
        assert_eq!(
            std::fs::read(dir.path().join("folds.json.tmp")).unwrap(),
            b"interrupted evidence"
        );
    }

    #[test]
    fn stale_foreign_future_and_overlapping_records_are_retained() {
        let (dir, graph, mut canvas) = fixture();
        let valid = json!(canvas.fold_records()[0]);
        let mut foreign = valid.clone();
        foreign["source_scope"] = json!("another graph");
        let mut future = valid.clone();
        future["version"] = json!(100);
        let mut stale = valid.clone();
        stale["members"] = json!([
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string()
        ]);
        let rejected = vec![valid.clone(), foreign, future, stale];
        let mut raw = vec![valid.clone()];
        raw.extend(rejected.clone());
        std::fs::write(
            dir.path().join("folds.json"),
            serde_json::to_vec(&json!({"version":1,"records":raw})).unwrap(),
        )
        .unwrap();
        let state = FoldSidecar::restore(dir.path(), graph, &mut canvas);
        assert_eq!(canvas.fold_records().len(), 1);
        assert_eq!(state.retained.len(), 4);
        state.save(dir.path(), canvas.fold_records()).unwrap();
        let saved: Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("folds.json")).unwrap()).unwrap();
        for raw in rejected {
            assert!(saved["records"].as_array().unwrap().contains(&raw));
        }
    }
}
