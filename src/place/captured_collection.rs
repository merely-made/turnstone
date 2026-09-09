// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's composition seam between local Fleece custody and Gemot's
//! effective fauna.
//!
//! The trail actor is the sole owner of the session Eidetic store. It
//! publishes an immutable in-memory library here; the place worker reads that
//! library and remints an app-owned view. Neither side acquires the other's
//! store handle.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use mere_document_lanes::FleeceAnnotationRecord;
use moot_port::captured_web::{CapturedCollectionProjection, RejectedCaptureReason};
use servitor::AuthorityProvider;

use crate::place::{
    CapturedCollectionCache, CapturedContentGroup, CapturedContribution,
    CapturedContributionStatus, CapturedRejectionReason,
};

#[derive(Clone, Debug, Default)]
struct ReadyCaptureLibrary {
    records: BTreeMap<[u8; 32], FleeceAnnotationRecord>,
    invalid: BTreeMap<[u8; 32], String>,
}

#[derive(Clone, Debug)]
enum CaptureLibraryState {
    Ready {
        directory: PathBuf,
        library: ReadyCaptureLibrary,
    },
    Unavailable(String),
}

impl Default for CaptureLibraryState {
    fn default() -> Self {
        Self::Unavailable("the session capture store has not opened".to_string())
    }
}

/// Read-only bridge shared by the trail and place workers. The lock guards
/// plain cloned records, never a database handle.
#[derive(Clone, Debug, Default)]
pub(crate) struct LocalCaptureLibrary(Arc<RwLock<CaptureLibraryState>>);

impl LocalCaptureLibrary {
    pub(crate) fn unavailable(&self, reason: impl Into<String>) {
        let mut state = self
            .0
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *state = CaptureLibraryState::Unavailable(reason.into());
    }

    fn replace(&self, directory: impl Into<PathBuf>, library: ReadyCaptureLibrary) {
        let mut state = self
            .0
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *state = CaptureLibraryState::Ready {
            directory: directory.into(),
            library,
        };
    }

    fn snapshot_for(&self, directory: &Path) -> CaptureLibraryState {
        let state = self
            .0
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        match state {
            CaptureLibraryState::Ready {
                directory: current,
                library,
            } if current == directory => CaptureLibraryState::Ready {
                directory: current,
                library,
            },
            CaptureLibraryState::Ready {
                directory: current, ..
            } => CaptureLibraryState::Unavailable(format!(
                "capture library belongs to {}, requested {}",
                current.display(),
                directory.display()
            )),
            unavailable => unavailable,
        }
    }
}

/// Rebuild the read-only library while the trail actor holds the one mutable
/// Fjall handle. A bad payload is retained as an explicit status keyed by the
/// manifest that failed; a failure to enumerate the store makes the whole
/// library unavailable rather than pretending it is empty.
pub(crate) fn refresh_local_capture_library(
    store: &mut dyn eidetic::Store,
    directory: &Path,
    library: &LocalCaptureLibrary,
) -> Result<(), String> {
    let manifests = pollster::block_on(eidetic::list_typed::<FleeceAnnotationRecord>(store))
        .map_err(|error| format!("list local Fleece annotations: {error}"))?;
    let mut ready = ReadyCaptureLibrary::default();
    for manifest in manifests {
        let id = manifest.id;
        let key = *id.0.as_bytes();
        match pollster::block_on(mere_document_lanes::load_fleece_annotation(store, id)) {
            Ok(Some(record)) => {
                ready.records.insert(key, record);
            },
            Ok(None) => {
                ready.invalid.insert(
                    key,
                    "the listed annotation manifest disappeared during resolution".to_string(),
                );
            },
            Err(error) => {
                ready.invalid.insert(key, error.to_string());
            },
        }
    }
    library.replace(directory, ready);
    Ok(())
}

fn status(
    share: &gemot::moot::FaunaEntry,
    reason: CapturedRejectionReason,
) -> CapturedContributionStatus {
    CapturedContributionStatus {
        share_operation: share.op_hash,
        annotation_manifest: share.manifest_id,
        title: share.title.clone(),
        reason,
    }
}

/// Remint Turnstone's cache from an already authority-filtered roster and the
/// current local library. `roster` must contain only effective fauna entries;
/// its retained withdrawal facts remain useful to the Mere projection and do
/// not become app authority.
pub(crate) fn remint(
    roster: &gemot::moot::MootRoster,
    authority: &impl AuthorityProvider,
    capture_directory: &Path,
    library: &LocalCaptureLibrary,
) -> CapturedCollectionCache {
    let library = library.snapshot_for(capture_directory);
    let (records, invalid) = match library {
        CaptureLibraryState::Unavailable(error) => {
            let mut rejected = Vec::new();
            for share in roster.authorized_fauna(authority) {
                let reason = if share.schema_id == mere_document_lanes::FLEECE_ANNOTATION_SCHEMA_ID
                {
                    CapturedRejectionReason::LibraryUnavailable(error.clone())
                } else {
                    CapturedRejectionReason::UnsupportedSchema(share.schema_id.clone())
                };
                rejected.push(status(share, reason));
            }
            rejected.sort_by_key(|item| item.share_operation);
            return CapturedCollectionCache {
                groups: Vec::new(),
                rejected,
            };
        },
        CaptureLibraryState::Ready { library: ready, .. } => (ready.records, ready.invalid),
    };

    // The Mere projection owns grouping and integrity checks. Locally corrupt
    // records that could not be decoded are removed before that call and
    // reported with their real failure rather than downgraded to "missing".
    let mut projectable = roster.clone();
    let mut rejected = Vec::new();
    projectable.fauna.retain(|share| {
        if share.schema_id == mere_document_lanes::FLEECE_ANNOTATION_SCHEMA_ID
            && let Some(error) = invalid.get(&share.manifest_id)
        {
            rejected.push(status(
                share,
                CapturedRejectionReason::InvalidRecord(error.clone()),
            ));
            false
        } else {
            true
        }
    });
    let projection = CapturedCollectionProjection::from_roster(&projectable, authority, &records);
    let groups = projection
        .pages
        .into_iter()
        .map(|page| CapturedContentGroup {
            extraction_schema: page.id.extraction_schema,
            normalization: page.id.normalization,
            reader_profile: page.id.reader_profile,
            canonical_text_hash: page.id.canonical_text_hash,
            canonical_text_iri: page.canonical_text_iri,
            canonical_text: page.canonical_text,
            contributions: page
                .contributions
                .into_iter()
                .map(|contribution| CapturedContribution {
                    share_operation: contribution.share.op_hash,
                    annotation_manifest: contribution.share.manifest_id,
                    contributor: contribution.share.shared_by,
                    source: contribution.capture.canonical_source,
                    title: contribution.share.title,
                    shared_at_ms: contribution.share.at_ms,
                })
                .collect(),
        })
        .collect();
    rejected.extend(projection.rejected.into_iter().map(|item| {
        let reason = match item.reason {
            RejectedCaptureReason::UnsupportedSchema(schema) => {
                CapturedRejectionReason::UnsupportedSchema(schema)
            },
            RejectedCaptureReason::MissingRecord => CapturedRejectionReason::MissingRecord,
            RejectedCaptureReason::InvalidRecord(error) => {
                CapturedRejectionReason::InvalidRecord(error)
            },
        };
        status(&item.share, reason)
    }));
    rejected.sort_by_key(|item| item.share_operation);
    CapturedCollectionCache { groups, rejected }
}

/// Keep only Gemot entries the full service authority admitted, including
/// membership and withdrawal policy, before the generic captured-web
/// projection sees the roster.
pub(crate) fn effective_roster(
    roster: &gemot::moot::MootRoster,
    fauna: &[gemot::moot::FaunaEntry],
) -> gemot::moot::MootRoster {
    let operations: BTreeSet<_> = fauna.iter().map(|share| share.op_hash).collect();
    let mut effective = roster.clone();
    effective
        .fauna
        .retain(|share| operations.contains(&share.op_hash));
    effective
}

#[cfg(test)]
mod tests {
    use super::*;
    use eidetic::{ManifestId, TypedPayload};
    use eidetic_fjall::FjallStore;
    use fleece::{TextPositionSelector, anchor_for_range, extract_document};
    use genet_static_dom::StaticDocument;
    use mere_document_lanes::eidetic_bridge::{CaptureIdentity, FLEECE_ANNOTATION_SCHEMA_ID};
    use servitor::{Cap, Mode, Subject};

    struct Allowed;

    impl AuthorityProvider for Allowed {
        fn covers(&self, _: Subject, _: &Cap, mode: Mode) -> bool {
            mode == Mode::Write
        }
    }

    fn record(source: &str, html: &str, raw_tag: u8) -> FleeceAnnotationRecord {
        let document = extract_document(&StaticDocument::parse(html));
        let end = document.page.text.chars().count() as u64;
        let anchor = anchor_for_range(
            &document.page.text,
            TextPositionSelector { start: 0, end },
            document.contract.quote_context,
        )
        .unwrap();
        FleeceAnnotationRecord::from_fleece(
            CaptureIdentity::new(source, eidetic::Hash::of(&[raw_tag])).unwrap(),
            &document,
            &anchor,
        )
        .unwrap()
    }

    fn share(manifest: ManifestId, operation: u8, at_ms: u64) -> gemot::moot::FaunaEntry {
        gemot::moot::FaunaEntry {
            manifest_id: *manifest.0.as_bytes(),
            schema_id: FLEECE_ANNOTATION_SCHEMA_ID.to_string(),
            title: format!("capture {operation}"),
            shared_by: [7; 32],
            at_ms,
            op_hash: [operation; 32],
        }
    }

    fn empty_roster(fauna: Vec<gemot::moot::FaunaEntry>) -> gemot::moot::MootRoster {
        gemot::moot::MootRoster {
            declaration: None,
            members: BTreeMap::new(),
            membership_revision: [0; 32],
            fauna,
            withdrawals: Vec::new(),
        }
    }

    fn save(store: &mut FjallStore, record: &FleeceAnnotationRecord, at_ms: u64) -> ManifestId {
        pollster::block_on(mere_document_lanes::bootstrap_fleece_annotation_schema(
            store,
        ))
        .unwrap();
        pollster::block_on(mere_document_lanes::save_fleece_annotation(
            store, record, at_ms,
        ))
        .unwrap()
    }

    #[test]
    fn local_records_group_search_and_survive_cold_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let html = "<main><p>Mycelial mutual-aid field notes.</p></main>";
        let first = record("https://one.test/notes", html, 1);
        let second = record("https://two.test/copy", html, 1);
        let (first_id, second_id) = {
            let mut store = FjallStore::open(dir.path()).unwrap();
            (save(&mut store, &first, 10), save(&mut store, &second, 20))
        };
        let roster = empty_roster(vec![share(first_id, 1, 10), share(second_id, 2, 20)]);

        let library = LocalCaptureLibrary::default();
        {
            let mut reopened = FjallStore::open(dir.path()).unwrap();
            refresh_local_capture_library(&mut reopened, dir.path(), &library).unwrap();
        }
        let collection = remint(&roster, &Allowed, dir.path(), &library);
        assert_eq!(collection.groups.len(), 1);
        assert_eq!(collection.groups[0].contributions.len(), 2);
        assert_eq!(collection.search("mycelial", 5).len(), 1);
        assert_eq!(collection.search("mycelial", 5)[0].contribution_count, 2);
        assert!(collection.rejected.is_empty());
    }

    #[test]
    fn targeted_withdrawal_remints_only_the_selected_contribution_away() {
        let html = "<main><p>Withdrawal keeps a surviving contribution.</p></main>";
        let first = record("https://one.test", html, 1);
        let second = record("https://two.test", html, 1);
        let first_id = ManifestId::of_blob(&first.serialize_to_bytes().unwrap());
        let second_id = ManifestId::of_blob(&second.serialize_to_bytes().unwrap());
        let kept = share(first_id, 1, 10);
        let removed = share(second_id, 2, 20);
        let mut roster = empty_roster(vec![kept.clone(), removed.clone()]);
        roster.withdrawals.push(gemot::moot::FaunaWithdrawal {
            target_share: removed.op_hash,
            withdrawn_by: removed.shared_by,
            at_ms: 30,
            op_hash: [3; 32],
        });
        let library = LocalCaptureLibrary::default();
        let scope = Path::new("test-capture-library");
        library.replace(
            scope,
            ReadyCaptureLibrary {
                records: [(kept.manifest_id, first), (removed.manifest_id, second)]
                    .into_iter()
                    .collect(),
                invalid: BTreeMap::new(),
            },
        );

        let collection = remint(&roster, &Allowed, scope, &library);
        assert_eq!(collection.groups.len(), 1);
        assert_eq!(collection.groups[0].contributions.len(), 1);
        assert_eq!(
            collection.groups[0].contributions[0].share_operation,
            kept.op_hash
        );
    }

    #[test]
    fn missing_and_unavailable_manifest_statuses_never_become_text() {
        let missing = share(ManifestId::of_blob(b"missing"), 4, 40);
        let invalid = share(ManifestId::of_blob(b"invalid"), 5, 50);
        let roster = empty_roster(vec![missing.clone(), invalid.clone()]);
        let ready = LocalCaptureLibrary::default();
        let scope = Path::new("test-capture-library");
        ready.replace(
            scope,
            ReadyCaptureLibrary {
                records: BTreeMap::new(),
                invalid: [(invalid.manifest_id, "payload hash mismatch".to_string())]
                    .into_iter()
                    .collect(),
            },
        );
        let missing_view = remint(&roster, &Allowed, scope, &ready);
        assert!(missing_view.groups.is_empty());
        assert!(missing_view.rejected.iter().any(|item| {
            item.share_operation == missing.op_hash
                && item.reason == CapturedRejectionReason::MissingRecord
        }));
        assert!(missing_view.rejected.iter().any(|item| {
            item.share_operation == invalid.op_hash
                && matches!(item.reason, CapturedRejectionReason::InvalidRecord(_))
        }));
        assert!(missing_view.search(&missing.title, 5).is_empty());

        let wrong_session = remint(
            &roster,
            &Allowed,
            Path::new("another-session/memory"),
            &ready,
        );
        assert!(wrong_session.groups.is_empty());
        assert!(matches!(
            wrong_session.rejected[0].reason,
            CapturedRejectionReason::LibraryUnavailable(_)
        ));

        let unavailable = LocalCaptureLibrary::default();
        unavailable.unavailable("capture storage is offline");
        let unavailable_view = remint(&roster, &Allowed, scope, &unavailable);
        assert!(unavailable_view.groups.is_empty());
        assert!(matches!(
            &unavailable_view.rejected[0].reason,
            CapturedRejectionReason::LibraryUnavailable(error)
                if error == "capture storage is offline"
        ));
        assert!(unavailable_view.search(&missing.title, 5).is_empty());
    }
}
