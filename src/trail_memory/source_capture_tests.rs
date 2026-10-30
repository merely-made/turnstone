// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use async_trait::async_trait;
use eidetic::{Backend, WriteOp};
use muniment::StoreError;
use std::sync::atomic::{AtomicUsize, Ordering};

fn document(label: &str) -> crate::content::FetchedDocument {
    let body = format!("<html><main><p>{label} naïve kestrel</p></main></html>");
    crate::content::FetchedDocument {
        bytes: body.as_bytes().to_vec(),
        body,
        content_type: Some("text/html; charset=utf-8".into()),
        effective_url: Some(format!("https://capture.test/{label}")),
        acquired_at_ms: 1_700_000_000_001,
    }
}

struct ActorFixtureSealer;
impl eidetic::PayloadSealer for ActorFixtureSealer {
    fn seal(
        &self,
        _: &eidetic::Hash,
        clear: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        Ok((
            clear.iter().map(|byte| byte ^ 0x5a).collect(),
            eidetic::SealedBlobRef {
                epoch: eidetic::SealEpochId([3; 16]),
                format: "actor-fixture".into(),
            },
        ))
    }

    fn unseal(
        &self,
        _: &eidetic::Hash,
        marker: &eidetic::SealedBlobRef,
        ciphertext: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        if marker.format != "actor-fixture" {
            return Err(eidetic::Error::new("wrong fixture marker"));
        }
        Ok(ciphertext.iter().map(|byte| byte ^ 0x5a).collect())
    }
}

fn actor_fixture_custody() -> crate::trail_memory::SourceCustody {
    crate::trail_memory::SourceCustody {
        sealer: std::sync::Arc::new(ActorFixtureSealer),
        epoch: std::sync::Arc::new(|| Some(1)),
    }
}

/// Fail immediately before or after one real write. The latter models a write
/// that committed before its acknowledgement was lost. Real Fjall deposits
/// remain behind for a fresh owner to reopen and retry.
struct FaultStore {
    inner: FjallStore,
    writes: AtomicUsize,
    fail_at: Option<usize>,
    after_write: bool,
}

impl FaultStore {
    fn new(inner: FjallStore, fail_at: Option<usize>, after_write: bool) -> Self {
        Self {
            inner,
            writes: AtomicUsize::new(0),
            fail_at,
            after_write,
        }
    }
}

#[async_trait]
impl Backend for FaultStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.inner.get(key).await
    }
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let write = self.writes.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_at == Some(write) && !self.after_write {
            return Err(StoreError::Backend(format!(
                "injected deposit write {write}: {key}"
            )));
        }
        self.inner.put(key, bytes).await?;
        if self.fail_at == Some(write) && self.after_write {
            return Err(StoreError::Backend(format!(
                "injected deposit write {write} acknowledged after commit: {key}"
            )));
        }
        Ok(())
    }
    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.inner.delete(key).await
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        self.inner.list(prefix).await
    }
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        self.inner.scan(start, end).await
    }
    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        // The capture path currently uses individual put acknowledgements.
        // Preserve the same prefix-failure instrument if it adopts a batch.
        for op in ops {
            match op {
                WriteOp::Put { key, value } => self.put(key, value).await?,
                WriteOp::Delete { key } => self.delete(key).await?,
            }
        }
        Ok(())
    }
}

fn id(value: &str) -> eidetic::ManifestId {
    serde_json::from_value(serde_json::json!(value)).unwrap()
}

fn request(document: &crate::content::FetchedDocument) -> crate::source_capture::CaptureRequest {
    crate::source_capture::CaptureRequest {
        id: uuid::Uuid::new_v4(),
        session: crate::panes::SessionId::new(),
        graph: crate::panes::GraphId::new(),
        node: uuid::Uuid::new_v4(),
        url: "https://capture.test/candidate".into(),
        source: Some(crate::source_capture::SourceIdentity::of(document)),
        keep: false,
    }
}

fn manifest_bytes(store: &mut dyn eidetic::Store, value: &str) -> Vec<u8> {
    pollster::block_on(store.get(&id(value).store_key()))
        .unwrap()
        .unwrap()
}

fn html_bytes(bytes: Vec<u8>, label: &str) -> crate::content::FetchedDocument {
    crate::content::FetchedDocument {
        body: String::from_utf8(bytes.clone()).unwrap(),
        bytes,
        content_type: Some("text/html; charset=utf-8".into()),
        effective_url: Some(format!("https://capture.test/{label}")),
        acquired_at_ms: 1_700_000_000_007,
    }
}

#[path = "source_capture_tests/deposit_failure_at_each_real_write_reopens_preserves_cases.rs"]
mod deposit_failure_at_each_real_write_reopens_preserves_cases;
#[path = "source_capture_tests/partial_receipt_recovers_original_non_utf8_html_decoding_cases.rs"]
mod partial_receipt_recovers_original_non_utf8_html_decoding_cases;
#[path = "source_capture_tests/same_directory_actor_reopen_rejects_old_opening_commands_cases.rs"]
mod same_directory_actor_reopen_rejects_old_opening_commands_cases;
