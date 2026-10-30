// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use muniment::{Backend, StoreError, StoreLimitKind, WriteOp};

struct RefusingStore;
#[async_trait::async_trait]
impl Backend for RefusingStore {
    async fn get(&self, _: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Err(StoreError::LimitExceeded {
            kind: StoreLimitKind::ValueBytes,
            limit: 7,
        })
    }
    async fn put(&self, _: &str, _: &[u8]) -> Result<(), StoreError> {
        unreachable!()
    }
    async fn delete(&self, _: &str) -> Result<(), StoreError> {
        unreachable!()
    }
    async fn list(&self, _: &str) -> Result<Vec<String>, StoreError> {
        unreachable!()
    }
    async fn scan(&self, _: &str, _: &str) -> Result<Vec<String>, StoreError> {
        unreachable!()
    }
    async fn apply(&self, _: &[WriteOp]) -> Result<(), StoreError> {
        unreachable!()
    }
}
#[test]
fn model_free_capture_storage_limit_refuses_entire_snapshot() {
    let admission = Admission {
        key: crate::saved_sources::Key {
            session: crate::panes::SessionId::new(),
            graph: crate::panes::GraphId::new(),
            node: uuid::Uuid::new_v4(),
            annotation_manifest: eidetic::ManifestId::of_blob(b"fixture").to_string(),
        },
        occurrence: 0,
        title: "fixture".into(),
        url: "https://example.test".into(),
        error: None,
    };
    let error = load_with_sealed_bounded(
        &mut RefusingStore,
        None,
        None,
        &[admission],
        Some(100),
        Some(100),
    )
    .unwrap_err();
    assert!(error.contains("ValueBytes"));
    assert!(error.contains("7"));
}
#[test]
fn model_free_evidence_open_shares_one_operation_budget() {
    let root = tempfile::tempdir().unwrap();
    let mut initial = FjallStore::open(root.path()).unwrap();
    let html = "<html><main>Captured evidence</main></html>";
    let stored = crate::trail_memory::capture_source_document_in(
        &mut initial,
        &crate::content::FetchedDocument {
            bytes: html.as_bytes().to_vec(),
            body: html.into(),
            content_type: Some("text/html".into()),
            effective_url: Some("https://example.test/captured".into()),
            acquired_at_ms: 1,
        },
    )
    .unwrap();
    let admission = Admission {
        key: crate::saved_sources::Key {
            session: crate::panes::SessionId::new(),
            graph: crate::panes::GraphId::new(),
            node: uuid::Uuid::new_v4(),
            annotation_manifest: stored.annotation_manifest,
        },
        occurrence: 0,
        title: "captured".into(),
        url: "https://example.test/captured".into(),
        error: None,
    };
    let content = load_content(&mut initial, &admission).unwrap();
    drop(initial);
    let limits = crate::research_storage::StorageLimits::default();
    let owner = FjallStore::open_with_limits(root.path(), limits.owner()).unwrap();
    let target = crate::saved_sources::Target {
        key: admission.key.clone(),
        text_hash: content.text_hash,
        selector: None,
    };
    let mut tight = limits.clone();
    tight.operation_read.max_visited_items = 0;
    let error =
        resolve_with_budget(&owner, None, None, &admission, &target, tight.budget()).unwrap_err();
    assert!(error.contains("VisitedItems"));
    assert!(resolve_with_budget(&owner, None, None, &admission, &target, limits.budget()).is_ok());
}
