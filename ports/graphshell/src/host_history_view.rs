// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A host history in the viewer (mer3ly site canvas plan, S3; Rulings
//! 144-148).
//!
//! A `scenomise.host-dataset/v2` history carries a dataset per revision; a
//! v1 envelope is its one revision. The viewer shows one checkpoint at a
//! time: [`revision_envelope`] selects that revision as a one-dataset
//! envelope, and the shared constructor,
//! [`host_dataset_view`](crate::host_dataset_view::host_dataset_view),
//! compiles it, so whatever that view carries (and any reading built over
//! the same envelope, such as folds) applies to the selected revision.
//!
//! With more than one revision, the checkpoint is classified against its
//! predecessor by `scenomise::history`, comparing the fields and
//! relationship parts the history declares. [`mark_changes`] puts each
//! change's mark on its node's title, and returns every change as text,
//! removed items included, for a reader. Nothing here interprets a revision:
//! it is shown as the host wrote it.

use mere::kernel::graph::apply::{GraphDelta, apply_graph_delta};
pub use scenomise::history::Change;
use scenomise::history::RevisionChanges;
use scenomise::host_dataset::{
    HOST_DATASET_SCHEMA_V1, HostDatasetRevision, HostDatasetRevisionV2, HostDatasetV1,
    HostDatasetV2,
};

use crate::host_dataset_view::{HostDatasetView, host_dataset_view};
use crate::projection_compile::ProjectionDataset;

/// How a change is said.
pub fn change_word(change: Change) -> &'static str {
    match change {
        Change::Added => "added",
        Change::Updated => "updated",
        Change::Stable => "stable",
        Change::Removed => "removed",
    }
}

/// The mark a node's title carries for its change: the sandbox's glyphs.
pub fn change_mark(change: Change) -> &'static str {
    match change {
        Change::Added => "+",
        Change::Updated => "~",
        Change::Stable => "\u{b7}",
        Change::Removed => "\u{2212}",
    }
}

/// How one occurrence changed since the previous checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewedChange {
    pub occurrence_id: String,
    pub label: String,
    pub change: Change,
    /// For an update, the compared fields that differ.
    pub fields: Vec<String>,
    /// For an update, whether its incident relationships differ.
    pub relationships_differ: bool,
}

impl ViewedChange {
    /// The label, the change, and for an update what differs.
    pub fn spoken(&self) -> String {
        let mut parts = self.fields.clone();
        if self.relationships_differ {
            parts.push("relations".into());
        }
        let text = format!("{}: {}", self.label, change_word(self.change));
        if parts.is_empty() {
            text
        } else {
            format!("{text} ({})", parts.join(", "))
        }
    }
}

/// How one relationship changed since the previous checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewedRelationChange {
    pub id: String,
    pub label: String,
    /// The labels of the two occurrences it joins.
    pub from: String,
    pub to: String,
    pub change: Change,
}

impl ViewedRelationChange {
    /// As the viewer speaks a relation, with its change.
    pub fn spoken(&self) -> String {
        format!(
            "{}: {} to {}, {}",
            self.label,
            self.from,
            self.to,
            change_word(self.change)
        )
    }
}

/// Everything that changed from the previous checkpoint to the shown one.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ViewedChanges {
    /// Shown occurrences in the revision's order, then removed ones.
    pub occurrences: Vec<ViewedChange>,
    /// Every relationship, shown ones first, then those removed while both
    /// their endpoints remain (Ruling 147).
    pub relationships: Vec<ViewedRelationChange>,
}

impl ViewedChanges {
    /// How many occurrences changed `change`'s way.
    pub fn count(&self, change: Change) -> usize {
        self.occurrences
            .iter()
            .filter(|entry| entry.change == change)
            .count()
    }

    /// The counts in the evaluator's order: "2 added, 1 updated, ...".
    pub fn summary(&self) -> String {
        [
            Change::Added,
            Change::Updated,
            Change::Stable,
            Change::Removed,
        ]
        .into_iter()
        .map(|change| format!("{} {}", self.count(change), change_word(change)))
        .collect::<Vec<_>>()
        .join(", ")
    }

    /// What a reader is told: every occurrence's change, then every
    /// relationship that did not stay as it was.
    pub fn spoken(&self) -> Vec<String> {
        self.occurrences
            .iter()
            .map(ViewedChange::spoken)
            .chain(
                self.relationships
                    .iter()
                    .filter(|entry| entry.change != Change::Stable)
                    .map(ViewedRelationChange::spoken),
            )
            .collect()
    }
}

/// A checkpoint compiled for the viewer, with its changes when the history
/// has more than one.
pub struct HistoryView {
    pub view: HostDatasetView,
    pub changes: Option<ViewedChanges>,
}

/// The text that names checkpoint `index`: its place and its revision.
pub fn checkpoint_text(history: &HostDatasetV2, index: usize) -> String {
    let revision = history
        .revisions
        .get(index)
        .map_or("", |entry| entry.revision.as_str());
    format!(
        "Checkpoint {} of {}: {revision}",
        index + 1,
        history.revisions.len()
    )
}

/// Checkpoint `index` as a one-dataset envelope: its dataset and
/// relationships, and the revisions published up to it.
pub fn revision_envelope(history: &HostDatasetV2, index: usize) -> Option<HostDatasetV1> {
    let entry = history.revisions.get(index)?;
    Some(HostDatasetV1 {
        schema: HOST_DATASET_SCHEMA_V1.to_owned(),
        dataset: entry.dataset.clone(),
        relationships: entry.relationships.clone(),
        revisions: history.revisions[..=index]
            .iter()
            .map(|entry| HostDatasetRevision {
                sequence: entry.sequence,
                revision: entry.revision.clone(),
            })
            .collect(),
    })
}

/// Compile checkpoint `index` of `history` for the viewer, marked with its
/// changes from the checkpoint before it.
pub fn host_history_view(history: &HostDatasetV2, index: usize) -> Result<HistoryView, String> {
    let envelope = revision_envelope(history, index)
        .ok_or_else(|| format!("the history has no checkpoint {}", index + 1))?;
    history_view(history, index, host_dataset_view(&envelope)?)
}

/// Apply checkpoint presentation to a freshly compiled view of that revision,
/// including a folded view. The complete changes list remains independent of
/// which occurrences the view currently discloses.
pub fn history_view(
    history: &HostDatasetV2,
    index: usize,
    mut view: HostDatasetView,
) -> Result<HistoryView, String> {
    let envelope = revision_envelope(history, index)
        .ok_or_else(|| format!("the history has no checkpoint {}", index + 1))?;
    if view.revision != envelope.dataset.revision.as_str() {
        return Err("The view does not represent the selected checkpoint".into());
    }
    let changes = if history.revisions.len() > 1 {
        history.changes_at(index)
    } else {
        None
    };
    let previous = index
        .checked_sub(1)
        .and_then(|previous| history.revisions.get(previous));
    let changes = changes.map(|changes| {
        mark_changes(&mut view, &changes);
        viewed_changes(&changes, &envelope, previous)
    });
    Ok(HistoryView { view, changes })
}

/// Put each shown occurrence's change mark on its node's title. A node is
/// found by its host-dataset address, so this marks any view built over the
/// same checkpoint, folded or not.
pub fn mark_changes(view: &mut HostDatasetView, changes: &RevisionChanges) {
    for entry in &changes.occurrences {
        let url = format!("urn:host-dataset:{}", entry.occurrence_id);
        let Some((key, _)) = view.graph.get_node_by_url(&url) else {
            continue;
        };
        let title = format!(
            "{} {}",
            change_mark(entry.change),
            view.graph.node_display_label(key)
        );
        apply_graph_delta(&mut view.graph, GraphDelta::SetNodeTitle { key, title });
    }
}

/// An occurrence's `label`, or its id when it has none.
fn label_of(dataset: &ProjectionDataset, occurrence: &str) -> String {
    dataset
        .occurrences
        .iter()
        .find(|entry| entry.occurrence_id == occurrence)
        .and_then(|entry| entry.values.get("label"))
        .and_then(|value| value.text())
        .unwrap_or(occurrence)
        .to_owned()
}

/// The evaluator's changes, labelled: a removed item is named from the
/// revision it was last in.
fn viewed_changes(
    changes: &RevisionChanges,
    current: &HostDatasetV1,
    previous: Option<&HostDatasetRevisionV2>,
) -> ViewedChanges {
    let dataset = &current.dataset;
    let named_in = |change: Change| match (change, previous) {
        (Change::Removed, Some(previous)) => &previous.dataset,
        _ => dataset,
    };
    let occurrences = changes
        .occurrences
        .iter()
        .map(|entry| ViewedChange {
            occurrence_id: entry.occurrence_id.clone(),
            label: label_of(named_in(entry.change), &entry.occurrence_id),
            change: entry.change,
            fields: entry.fields.clone(),
            relationships_differ: entry.relationships_differ,
        })
        .collect();
    // A relationship is named as it was disclosed: now, or when last seen.
    let relationship_label = |id: &str| {
        current
            .relationships
            .iter()
            .chain(previous.map_or(&[][..], |previous| &previous.relationships))
            .find(|relationship| relationship.id == id)
            .map(|relationship| relationship.label.clone())
    };
    let relationships = changes
        .relationships
        .iter()
        .map(|entry| ViewedRelationChange {
            id: entry.id.clone(),
            label: relationship_label(&entry.id).unwrap_or_else(|| entry.id.clone()),
            // Both endpoints of a reported relationship are in this revision.
            from: label_of(dataset, &entry.from_occurrence),
            to: label_of(dataset, &entry.to_occurrence),
            change: entry.change,
        })
        .collect();
    ViewedChanges {
        occurrences,
        relationships,
    }
}

#[cfg(test)]
#[path = "host_history_view_tests.rs"]
mod tests;
