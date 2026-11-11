// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! What a projection save acknowledgement names.
//!
//! The host's completion says only that a write landed. The save recorded what
//! it sent, so the acknowledgement marks clean that editor and nothing else: a
//! draft that replaced it during the save stays dirty.

use scenograph::{PublicSourceRevision, SourceBinding};
use scenomise::projection::ProjectionDataset;

/// What identifies a resolved dataset: its source binding and public revision.
#[derive(Clone, Debug, Eq, PartialEq)]
struct DatasetId {
    source: SourceBinding,
    revision: PublicSourceRevision,
}

impl DatasetId {
    fn of(dataset: &ProjectionDataset) -> Self {
        Self {
            source: dataset.source.clone(),
            revision: dataset.revision.clone(),
        }
    }
}

/// The dataset and editor generation a save was sent from. The generation
/// counts editor replacements; the page bumps it whenever it installs a
/// different editor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingSave {
    dataset: Option<DatasetId>,
    generation: u64,
}

/// Whether the editor a save acknowledges is the one it was sent from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveAck {
    /// The same editor over the same dataset: it is saved.
    Current,
    /// A different dataset is loaded now; the saved projection was for the earlier one.
    EarlierDataset,
    /// The same dataset, but a different editor replaced the one that was sent.
    EarlierDraft,
}

impl PendingSave {
    pub fn sent(dataset: Option<&ProjectionDataset>, generation: u64) -> Self {
        Self {
            dataset: dataset.map(DatasetId::of),
            generation,
        }
    }

    /// Compare the save with the dataset and editor generation now current.
    pub fn acknowledge(&self, dataset: Option<&ProjectionDataset>, generation: u64) -> SaveAck {
        if self.dataset != dataset.map(DatasetId::of) {
            SaveAck::EarlierDataset
        } else if self.generation != generation {
            SaveAck::EarlierDraft
        } else {
            SaveAck::Current
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn dataset(resource: &str, revision: &str) -> ProjectionDataset {
        ProjectionDataset {
            source: SourceBinding {
                authority: "local-device".into(),
                domain: "notes".into(),
                resource: resource.into(),
            },
            revision: revision.into(),
            fields: BTreeMap::new(),
            occurrences: Vec::new(),
        }
    }

    #[test]
    fn a_save_acknowledges_only_the_editor_it_was_sent_from() {
        let notes = dataset("graph:notes", "rev-1");
        let sent = PendingSave::sent(Some(&notes), 3);
        assert_eq!(sent.acknowledge(Some(&notes), 3), SaveAck::Current);
        assert_eq!(
            sent.acknowledge(Some(&notes), 4),
            SaveAck::EarlierDraft,
            "the same dataset reloaded is another draft"
        );
        assert_eq!(
            sent.acknowledge(Some(&dataset("graph:tasks", "rev-1")), 4),
            SaveAck::EarlierDataset
        );
        assert_eq!(
            sent.acknowledge(Some(&dataset("graph:notes", "rev-2")), 3),
            SaveAck::EarlierDataset,
            "a new revision is another dataset"
        );
        assert_eq!(sent.acknowledge(None, 3), SaveAck::EarlierDataset);
    }

    #[test]
    fn a_save_without_a_dataset_is_current_until_one_loads() {
        let sent = PendingSave::sent(None, 0);
        assert_eq!(sent.acknowledge(None, 0), SaveAck::Current);
        assert_eq!(
            sent.acknowledge(Some(&dataset("graph:notes", "rev-1")), 1),
            SaveAck::EarlierDataset
        );
    }
}
