// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Small, host-neutral state for a disclosed practice workspace.
//!
//! A product supplies the current source identity, occurrence identifiers,
//! and a comparison disclosure. This module records navigation and
//! validates snapshots, but does not calculate, reinterpret, or edit product
//! material. In particular, [`ComparisonRecord::result`] is preserved as the
//! product's JSON disclosure rather than translated into generic arithmetic.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// The serialized workspace schema supported by this reducer.
pub const PRACTICE_WORKSPACE_VERSION: u16 = 1;

/// A product authority and resource binding, without an authority handle.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct PracticeSourceBinding {
    pub authority: String,
    pub domain: String,
    pub resource: String,
}

/// The current product disclosure against which workspace state is checked.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PracticeSource {
    pub binding: PracticeSourceBinding,
    pub revision: String,
    pub occurrence_ids: BTreeSet<String>,
}

impl PracticeSource {
    pub fn contains(&self, occurrence: &str) -> bool {
        self.occurrence_ids.contains(occurrence)
    }

    fn fingerprint(&self) -> PracticeSourceFingerprint {
        PracticeSourceFingerprint {
            binding: self.binding.clone(),
            revision: self.revision.clone(),
        }
    }
}

/// The source identity retained by a saved workspace.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PracticeSourceFingerprint {
    pub binding: PracticeSourceBinding,
    pub revision: String,
}

/// The product source identity of an occurrence in a comparison disclosure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComparisonSourceRef {
    pub adapter: String,
    pub id: String,
}

/// One side of a product-provided comparison.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComparisonSubject {
    pub occurrence_id: String,
    pub source: ComparisonSourceRef,
    pub label: String,
    /// Product fields such as Woodshed's `pitch_set`, retained verbatim for a
    /// host that can present the original comparison evidence.
    #[serde(flatten)]
    pub disclosed: BTreeMap<String, serde_json::Value>,
}

/// The product's declared comparison method.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComparisonMethod {
    pub id: String,
    pub version: u32,
}

/// An externally computed comparison record.
///
/// `result` retains product-specific evidence exactly as disclosed. The
/// reducer uses only the source, revision, method, and occurrence pair to
/// establish the stable relation identity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComparisonRecord {
    pub source: PracticeSourceBinding,
    pub revision: String,
    pub method: ComparisonMethod,
    pub left: ComparisonSubject,
    pub right: ComparisonSubject,
    pub result: serde_json::Value,
    /// Additional product disclosure retained for persistence validation and
    /// product-owned presentation, rather than silently discarded by serde.
    #[serde(flatten)]
    pub disclosed: BTreeMap<String, serde_json::Value>,
}

impl ComparisonRecord {
    /// A stable relation identity derived from disclosed, non-arithmetic facts.
    ///
    /// The pair is deliberately ordered. Woodshed's `left_only`, `right_only`,
    /// and singleton motion describe a directional comparison, so reversing
    /// the subjects must not reuse the same graph source identity.
    pub fn relation(&self) -> ComparisonRelation {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for value in [
            &self.source.authority,
            &self.source.domain,
            &self.source.resource,
            &self.revision,
            &self.method.id,
            &self.left.occurrence_id,
            &self.right.occurrence_id,
        ] {
            for byte in value.bytes().chain([0]) {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        for byte in self.method.version.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        ComparisonRelation {
            id: format!("comparison:{hash:016x}"),
            source: PracticeSourceFingerprint {
                binding: self.source.clone(),
                revision: self.revision.clone(),
            },
            method: self.method.clone(),
            left_occurrence: self.left.occurrence_id.clone(),
            right_occurrence: self.right.occurrence_id.clone(),
        }
    }

    /// Canonical JSON for the complete product disclosure.
    ///
    /// This includes `result` and flattened side evidence, unlike the stable
    /// relation ID, so reopening cannot silently attach old notes to changed
    /// product evidence at the same source revision.
    pub fn canonical_evidence(&self) -> String {
        let value = serde_json::to_value(self).expect("comparison record is serializable");
        let mut output = String::new();
        write_canonical_json(&mut output, &value);
        output
    }

    fn validate_against(&self, source: &PracticeSource) -> Result<(), ComparisonError> {
        if self.source != source.binding {
            return Err(ComparisonError::SourceBindingMismatch);
        }
        if self.revision != source.revision {
            return Err(ComparisonError::SourceRevisionMismatch);
        }
        if self.method.id.trim().is_empty() {
            return Err(ComparisonError::MissingMethod);
        }
        for subject in [&self.left, &self.right] {
            if subject.occurrence_id.trim().is_empty() || !source.contains(&subject.occurrence_id) {
                return Err(ComparisonError::UnknownOccurrence(
                    subject.occurrence_id.clone(),
                ));
            }
        }
        if self.left.occurrence_id == self.right.occurrence_id {
            return Err(ComparisonError::RepeatedOccurrence);
        }
        Ok(())
    }
}

/// A stable relation derived from one disclosed comparison record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComparisonRelation {
    pub id: String,
    pub source: PracticeSourceFingerprint,
    pub method: ComparisonMethod,
    pub left_occurrence: String,
    pub right_occurrence: String,
}

/// A view the host may realize.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PracticeView {
    #[default]
    Relations,
    Compare,
    History,
}

/// A stable selection target. `Comparison` is the disclosed pair itself.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Selection {
    Occurrence(String),
    Comparison,
}

/// The restorable navigation position.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PracticeLocation {
    pub view: PracticeView,
    pub selection: Selection,
}

/// An immutable, monotonically identified entry in chronological history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: u64,
    pub location: PracticeLocation,
}

/// Host-owned runtime options retained with a snapshot.
///
/// This reducer does not change these settings. A host resolves `layout_id`
/// and applies physics according to its own registered implementations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PracticeRuntimeConfig {
    pub layout_id: String,
    pub physics_enabled: bool,
}

/// The complete reducer state. The source is immutable for its lifetime.
#[derive(Clone, Debug, PartialEq)]
pub struct PracticeWorkspace {
    source: PracticeSource,
    comparison: ComparisonRecord,
    location: PracticeLocation,
    history: Vec<HistoryEntry>,
    next_history_id: u64,
    history_limit: usize,
    runtime: PracticeRuntimeConfig,
}

/// A durable workspace payload. A host marks its own storage success after
/// writing this value; creating a snapshot has no persistence side effect.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PracticeWorkspaceSnapshot {
    pub version: u16,
    pub source: PracticeSourceFingerprint,
    /// Canonical copy of every disclosed comparison field retained at save
    /// time. It is compared exactly during reopen.
    pub comparison_evidence: String,
    pub location: PracticeLocation,
    pub history: Vec<HistoryEntry>,
    pub next_history_id: u64,
    pub runtime: PracticeRuntimeConfig,
}

/// A host gesture accepted by the reducer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PracticeAction {
    Select(Selection),
    SelectView(PracticeView),
    /// Restore the exact retained entry. IDs are monotonic, and old IDs may
    /// become unavailable once the bounded buffer evicts them.
    ReturnTo(u64),
}

/// The result of reducing one action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PracticeOutcome {
    Changed,
    Unchanged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComparisonError {
    SourceBindingMismatch,
    SourceRevisionMismatch,
    MissingMethod,
    UnknownOccurrence(String),
    RepeatedOccurrence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceError {
    ZeroHistoryLimit,
    EmptyLayoutId,
    Comparison(ComparisonError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReduceError {
    UnknownOccurrence(String),
    UnknownHistoryEntry(u64),
    HistoryIdExhausted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    UnsupportedVersion(u16),
    ZeroHistoryLimit,
    SourceBindingMismatch,
    SourceRevisionMismatch,
    TooManyHistoryEntries { limit: usize, actual: usize },
    InvalidHistoryOrder,
    InvalidNextHistoryId,
    MissingOccurrence(String),
    EmptyLayoutId,
    ComparisonEvidenceMismatch,
    Comparison(ComparisonError),
}

impl PracticeWorkspace {
    pub fn new(
        source: PracticeSource,
        comparison: ComparisonRecord,
        runtime: PracticeRuntimeConfig,
        history_limit: usize,
    ) -> Result<Self, WorkspaceError> {
        validate_config(&runtime, history_limit)?;
        comparison
            .validate_against(&source)
            .map_err(WorkspaceError::Comparison)?;
        Ok(Self {
            source,
            comparison,
            location: PracticeLocation {
                view: PracticeView::Relations,
                selection: Selection::Comparison,
            },
            history: Vec::new(),
            next_history_id: 0,
            history_limit,
            runtime,
        })
    }

    pub fn source(&self) -> &PracticeSource {
        &self.source
    }

    pub fn comparison(&self) -> &ComparisonRecord {
        &self.comparison
    }

    pub fn relation(&self) -> ComparisonRelation {
        self.comparison.relation()
    }

    /// The other comparison subject when an occurrence is selected.
    pub fn relation_peer_for_selection(&self) -> Option<&ComparisonSubject> {
        let Selection::Occurrence(selected) = &self.location.selection else {
            return None;
        };
        if selected == &self.comparison.left.occurrence_id {
            Some(&self.comparison.right)
        } else if selected == &self.comparison.right.occurrence_id {
            Some(&self.comparison.left)
        } else {
            None
        }
    }

    pub fn location(&self) -> &PracticeLocation {
        &self.location
    }

    pub fn history(&self) -> &[HistoryEntry] {
        &self.history
    }

    pub fn runtime(&self) -> &PracticeRuntimeConfig {
        &self.runtime
    }

    /// Replace host-owned runtime settings after the host has resolved them.
    /// This does not create a navigation entry or alter the immutable source.
    pub fn set_runtime(
        &mut self,
        runtime: PracticeRuntimeConfig,
    ) -> Result<PracticeOutcome, WorkspaceError> {
        validate_config(&runtime, self.history_limit)?;
        if self.runtime == runtime {
            return Ok(PracticeOutcome::Unchanged);
        }
        self.runtime = runtime;
        Ok(PracticeOutcome::Changed)
    }

    pub fn snapshot(&self) -> PracticeWorkspaceSnapshot {
        PracticeWorkspaceSnapshot {
            version: PRACTICE_WORKSPACE_VERSION,
            source: self.source.fingerprint(),
            comparison_evidence: self.comparison.canonical_evidence(),
            location: self.location.clone(),
            history: self.history.clone(),
            next_history_id: self.next_history_id,
            runtime: self.runtime.clone(),
        }
    }

    pub fn reopen(
        snapshot: PracticeWorkspaceSnapshot,
        source: PracticeSource,
        comparison: ComparisonRecord,
        history_limit: usize,
    ) -> Result<Self, SnapshotError> {
        if snapshot.version != PRACTICE_WORKSPACE_VERSION {
            return Err(SnapshotError::UnsupportedVersion(snapshot.version));
        }
        if history_limit == 0 {
            return Err(SnapshotError::ZeroHistoryLimit);
        }
        if snapshot.source.binding != source.binding {
            return Err(SnapshotError::SourceBindingMismatch);
        }
        if snapshot.source.revision != source.revision {
            return Err(SnapshotError::SourceRevisionMismatch);
        }
        if snapshot.history.len() > history_limit {
            return Err(SnapshotError::TooManyHistoryEntries {
                limit: history_limit,
                actual: snapshot.history.len(),
            });
        }
        if snapshot.runtime.layout_id.trim().is_empty() {
            return Err(SnapshotError::EmptyLayoutId);
        }
        comparison
            .validate_against(&source)
            .map_err(SnapshotError::Comparison)?;
        if snapshot.comparison_evidence != comparison.canonical_evidence() {
            return Err(SnapshotError::ComparisonEvidenceMismatch);
        }
        validate_location(&snapshot.location, &source)?;
        let mut previous = None;
        for entry in &snapshot.history {
            if previous.is_some_and(|id| entry.id <= id) {
                return Err(SnapshotError::InvalidHistoryOrder);
            }
            validate_location(&entry.location, &source)?;
            previous = Some(entry.id);
        }
        if previous.is_some_and(|id| snapshot.next_history_id <= id) {
            return Err(SnapshotError::InvalidNextHistoryId);
        }
        Ok(Self {
            source,
            comparison,
            location: snapshot.location,
            history: snapshot.history,
            next_history_id: snapshot.next_history_id,
            history_limit,
            runtime: snapshot.runtime,
        })
    }

    pub fn reduce(&mut self, action: PracticeAction) -> Result<PracticeOutcome, ReduceError> {
        match action {
            PracticeAction::Select(selection) => {
                if let Selection::Occurrence(id) = &selection
                    && !self.source.contains(id)
                {
                    return Err(ReduceError::UnknownOccurrence(id.clone()));
                }
                let next = PracticeLocation {
                    view: self.location.view,
                    selection,
                };
                self.move_to(next)
            },
            PracticeAction::SelectView(view) => self.move_to(PracticeLocation {
                view,
                selection: self.location.selection.clone(),
            }),
            PracticeAction::ReturnTo(id) => {
                let Some(entry) = self.history.iter().find(|entry| entry.id == id) else {
                    return Err(ReduceError::UnknownHistoryEntry(id));
                };
                if self.location == entry.location {
                    Ok(PracticeOutcome::Unchanged)
                } else {
                    self.location = entry.location.clone();
                    Ok(PracticeOutcome::Changed)
                }
            },
        }
    }

    fn move_to(&mut self, next: PracticeLocation) -> Result<PracticeOutcome, ReduceError> {
        if self.location == next {
            return Ok(PracticeOutcome::Unchanged);
        }
        let id = self.next_history_id;
        self.next_history_id = self
            .next_history_id
            .checked_add(1)
            .ok_or(ReduceError::HistoryIdExhausted)?;
        self.history.push(HistoryEntry {
            id,
            location: self.location.clone(),
        });
        if self.history.len() > self.history_limit {
            self.history.remove(0);
        }
        self.location = next;
        Ok(PracticeOutcome::Changed)
    }
}

fn validate_config(
    runtime: &PracticeRuntimeConfig,
    history_limit: usize,
) -> Result<(), WorkspaceError> {
    if history_limit == 0 {
        return Err(WorkspaceError::ZeroHistoryLimit);
    }
    if runtime.layout_id.trim().is_empty() {
        return Err(WorkspaceError::EmptyLayoutId);
    }
    Ok(())
}

fn validate_location(
    location: &PracticeLocation,
    source: &PracticeSource,
) -> Result<(), SnapshotError> {
    if let Selection::Occurrence(id) = &location.selection
        && !source.contains(id)
    {
        return Err(SnapshotError::MissingOccurrence(id.clone()));
    }
    Ok(())
}

fn write_canonical_json(output: &mut String, value: &serde_json::Value) {
    match value {
        serde_json::Value::Null => output.push_str("null"),
        serde_json::Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        serde_json::Value::Number(value) => {
            output.push_str(&value.to_string());
        },
        serde_json::Value::String(value) => {
            output.push_str(&serde_json::to_string(value).expect("string is serializable"));
        },
        serde_json::Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                write_canonical_json(output, value);
            }
            output.push(']');
        },
        serde_json::Value::Object(values) => {
            output.push('{');
            let mut entries: Vec<_> = values.iter().collect();
            entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                output.push_str(&serde_json::to_string(key).expect("key is serializable"));
                output.push(':');
                write_canonical_json(output, value);
            }
            output.push('}');
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> PracticeSource {
        PracticeSource {
            binding: PracticeSourceBinding {
                authority: "woodshed".into(),
                domain: "music.practice".into(),
                resource: "set:musical-comparison".into(),
            },
            revision: "musical-comparison-fixture-v1".into(),
            occurrence_ids: ["card:1".into(), "card:2".into(), "card:3".into()]
                .into_iter()
                .collect(),
        }
    }

    fn comparison() -> ComparisonRecord {
        serde_json::from_str(include_str!(
            "../../../../woodshed/scenarios/woodshed_musical_comparison.json"
        ))
        .expect("real Woodshed comparison export is valid")
    }

    fn runtime() -> PracticeRuntimeConfig {
        PracticeRuntimeConfig {
            layout_id: "grid.default".into(),
            physics_enabled: false,
        }
    }

    fn workspace(limit: usize) -> PracticeWorkspace {
        PracticeWorkspace::new(source(), comparison(), runtime(), limit).unwrap()
    }

    #[test]
    fn woodshed_export_preserves_disclosed_evidence_and_stable_relation() {
        let mut workspace = workspace(4);
        assert_eq!(workspace.location().selection, Selection::Comparison);
        assert_eq!(workspace.location().view, PracticeView::Relations);
        assert_eq!(workspace.comparison().result["shared"][0]["label"], "C");
        assert_eq!(
            workspace.comparison().left.disclosed["pitch_set"][0]["label"],
            "C"
        );
        let mut export = serde_json::to_value(workspace.comparison()).unwrap();
        export["product_note"] = serde_json::Value::String("kept as evidence".into());
        let extended: ComparisonRecord = serde_json::from_value(export).unwrap();
        assert_eq!(extended.disclosed["product_note"], "kept as evidence");
        let relation = workspace.relation();
        assert_eq!(relation.left_occurrence, "card:1");
        assert_eq!(relation.right_occurrence, "card:2");
        assert_eq!(relation, workspace.comparison().relation());
        let mut reversed = workspace.comparison().clone();
        std::mem::swap(&mut reversed.left, &mut reversed.right);
        assert_ne!(relation.id, reversed.relation().id);
        workspace
            .reduce(PracticeAction::Select(Selection::Occurrence(
                "card:1".into(),
            )))
            .unwrap();
        assert_eq!(
            workspace
                .relation_peer_for_selection()
                .unwrap()
                .occurrence_id,
            "card:2"
        );
    }

    #[test]
    fn rejects_comparison_when_current_source_identity_or_ids_do_not_match() {
        let mut stale = source();
        stale.revision = "newer".into();
        assert_eq!(
            PracticeWorkspace::new(stale, comparison(), runtime(), 4),
            Err(WorkspaceError::Comparison(
                ComparisonError::SourceRevisionMismatch
            ))
        );

        let mut missing = source();
        missing.occurrence_ids.remove("card:2");
        assert_eq!(
            PracticeWorkspace::new(missing, comparison(), runtime(), 4),
            Err(WorkspaceError::Comparison(
                ComparisonError::UnknownOccurrence("card:2".into())
            ))
        );
    }

    #[test]
    fn return_to_changes_location_without_editing_source() {
        let mut workspace = workspace(4);
        let source_before = workspace.source().clone();
        workspace
            .reduce(PracticeAction::Select(Selection::Occurrence(
                "card:1".into(),
            )))
            .unwrap();
        let comparison_entry = workspace.history()[0].id;
        workspace
            .reduce(PracticeAction::ReturnTo(comparison_entry))
            .unwrap();
        assert_eq!(workspace.location().selection, Selection::Comparison);
        assert_eq!(workspace.source(), &source_before);
    }

    #[test]
    fn history_is_bounded_and_ids_remain_monotonic_after_eviction() {
        let mut workspace = workspace(2);
        workspace
            .reduce(PracticeAction::Select(Selection::Occurrence(
                "card:1".into(),
            )))
            .unwrap();
        workspace
            .reduce(PracticeAction::SelectView(PracticeView::Compare))
            .unwrap();
        workspace
            .reduce(PracticeAction::Select(Selection::Occurrence(
                "card:2".into(),
            )))
            .unwrap();
        assert_eq!(workspace.history().len(), 2);
        assert_eq!(workspace.history()[0].id, 1);
        assert_eq!(workspace.history()[1].id, 2);
        assert_eq!(
            workspace.reduce(PracticeAction::ReturnTo(0)),
            Err(ReduceError::UnknownHistoryEntry(0))
        );
    }

    #[test]
    fn save_and_reopen_restores_history_location_and_runtime() {
        let mut workspace = workspace(4);
        workspace
            .reduce(PracticeAction::Select(Selection::Occurrence(
                "card:1".into(),
            )))
            .unwrap();
        workspace
            .reduce(PracticeAction::SelectView(PracticeView::Relations))
            .unwrap();
        let saved = workspace.snapshot();
        let reopened = PracticeWorkspace::reopen(saved, source(), comparison(), 4).unwrap();
        assert_eq!(reopened.location(), workspace.location());
        assert_eq!(reopened.history(), workspace.history());
        assert_eq!(reopened.runtime(), workspace.runtime());
    }

    #[test]
    fn reopen_rejects_changed_evidence_at_the_same_source_revision() {
        let snapshot = workspace(4).snapshot();
        let mut changed = comparison();
        changed.left.disclosed.get_mut("pitch_set").unwrap()[0]["label"] =
            serde_json::Value::String("C altered".into());
        assert_eq!(
            PracticeWorkspace::reopen(snapshot, source(), changed, 4),
            Err(SnapshotError::ComparisonEvidenceMismatch)
        );
    }

    #[test]
    fn runtime_changes_are_validated_without_creating_history() {
        let mut workspace = workspace(4);
        workspace
            .set_runtime(PracticeRuntimeConfig {
                layout_id: "scatter.default".into(),
                physics_enabled: true,
            })
            .unwrap();
        assert_eq!(workspace.history(), &[]);
        assert_eq!(workspace.runtime().layout_id, "scatter.default");
        assert_eq!(
            workspace.set_runtime(PracticeRuntimeConfig {
                layout_id: " ".into(),
                physics_enabled: true,
            }),
            Err(WorkspaceError::EmptyLayoutId)
        );
    }

    #[test]
    fn reopen_rejects_stale_revision_and_missing_saved_occurrence() {
        let mut workspace = workspace(4);
        workspace
            .reduce(PracticeAction::Select(Selection::Occurrence(
                "card:1".into(),
            )))
            .unwrap();
        let mut snapshot = workspace.snapshot();

        let mut stale = source();
        stale.revision = "newer".into();
        assert_eq!(
            PracticeWorkspace::reopen(snapshot.clone(), stale, comparison(), 4),
            Err(SnapshotError::SourceRevisionMismatch)
        );

        let mut missing = source();
        missing.occurrence_ids.remove("card:1");
        let mut current_comparison = comparison();
        current_comparison.left.occurrence_id = "card:2".into();
        current_comparison.right.occurrence_id = "card:3".into();
        snapshot.comparison_evidence = current_comparison.canonical_evidence();
        assert_eq!(
            PracticeWorkspace::reopen(snapshot, missing, current_comparison, 4),
            Err(SnapshotError::MissingOccurrence("card:1".into()))
        );
    }
}
