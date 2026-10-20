// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Compare-and-swap persistence for an editable document declaration.

use muniment::{Backend, WriteOp};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(not(target_arch = "wasm32"))]
pub trait DocumentStore: Backend + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<B: Backend + Sync> DocumentStore for B {}
#[cfg(target_arch = "wasm32")]
pub trait DocumentStore: Backend {}
#[cfg(target_arch = "wasm32")]
impl<B: Backend> DocumentStore for B {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredDocument {
    pub version: u16,
    pub id: String,
    pub source: String,
}

impl StoredDocument {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("This document uses an unsupported stored format".into());
        }
        if self.id.trim().is_empty() || self.id.len() > 256 {
            return Err("A stable document id of at most 256 bytes is required".into());
        }
        if self.source.len() > super::MAX_DOCUMENT_BYTES {
            return Err("Document source exceeds one MiB".into());
        }
        Ok(())
    }
}

/// Load the current declaration without changing bytes on any refusal.
pub async fn load<B: DocumentStore>(
    backend: &B,
    key: &str,
) -> Result<Option<(StoredDocument, Vec<u8>)>, String> {
    let Some(bytes) = backend.get(key).await.map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    if bytes.len() > super::MAX_DOCUMENT_BYTES {
        return Err("Stored document exceeds one MiB; stored bytes are unchanged".into());
    }
    let document: StoredDocument = serde_json::from_slice(&bytes).map_err(|error| {
        format!("Stored document cannot be read; stored bytes are unchanged: {error}")
    })?;
    document
        .validate()
        .map_err(|error| format!("{error}; stored bytes are unchanged"))?;
    Ok(Some((document, bytes)))
}

/// Save only if the bytes still match the version the caller loaded.
///
/// The transaction must acknowledge before the new bytes are returned. A read
/// error inside the transaction is reported separately from a stale value.
pub async fn save<B: DocumentStore>(
    backend: &B,
    key: &str,
    expected: Option<Vec<u8>>,
    document: StoredDocument,
) -> Result<Vec<u8>, String> {
    document.validate()?;
    if key.trim().is_empty() {
        return Err("A nonblank storage key is required".into());
    }
    let bytes = serde_json::to_vec(&document).map_err(|error| error.to_string())?;
    if bytes.len() > super::MAX_DOCUMENT_BYTES {
        return Err("Serialized document exceeds one MiB".into());
    }

    let accepted = Arc::new(AtomicBool::new(false));
    let accepted_in_tx = accepted.clone();
    let read_error = Arc::new(Mutex::new(None));
    let read_error_in_tx = read_error.clone();
    let value = bytes.clone();
    let key = key.to_owned();
    let transaction = backend
        .transact(Box::new(move |reader| match reader.get(&key) {
            Err(error) => {
                *read_error_in_tx
                    .lock()
                    .expect("transaction read error lock") = Some(error.to_string());
                Vec::new()
            },
            Ok(current) if current == expected => {
                accepted_in_tx.store(true, Ordering::Release);
                vec![WriteOp::Put { key, value }]
            },
            Ok(_) => Vec::new(),
        }))
        .await;

    if let Some(error) = read_error
        .lock()
        .expect("transaction read error lock")
        .take()
    {
        return Err(format!("Cannot inspect stored document: {error}"));
    }
    transaction.map_err(|error| error.to_string())?;
    if !accepted.load(Ordering::Acquire) {
        return Err("The stored document changed; reopen it before saving".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muniment::MemoryBackend;

    fn document(id: &str, source: &str) -> StoredDocument {
        StoredDocument {
            version: 1,
            id: id.into(),
            source: source.into(),
        }
    }

    #[test]
    fn unicode_document_roundtrips_with_exact_bytes() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let original = document("文書-α", "urn:knot:詩/épisode-七");
            let bytes = save(&backend, "document/test", None, original.clone())
                .await
                .unwrap();
            let loaded = load(&backend, "document/test").await.unwrap().unwrap();
            assert_eq!(loaded.0, original);
            assert_eq!(loaded.1, bytes);
            assert_eq!(backend.get("document/test").await.unwrap(), Some(bytes));
        });
    }

    #[test]
    fn empty_source_document_roundtrips() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let original = document("empty-composition", "");
            save(&backend, "document/empty", None, original.clone())
                .await
                .unwrap();
            let (loaded, _) = load(&backend, "document/empty").await.unwrap().unwrap();
            assert_eq!(loaded, original);
        });
    }

    #[test]
    fn stale_writer_is_refused_without_overwriting_current_value() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let first = save(
                &backend,
                "document/test",
                None,
                document("poem", "urn:source:first"),
            )
            .await
            .unwrap();
            let second = save(
                &backend,
                "document/test",
                Some(first.clone()),
                document("poem", "urn:source:second"),
            )
            .await
            .unwrap();
            let stale = save(
                &backend,
                "document/test",
                Some(first),
                document("poem", "urn:source:stale"),
            )
            .await;
            assert!(stale.unwrap_err().contains("changed"));
            assert_eq!(backend.get("document/test").await.unwrap(), Some(second));
        });
    }

    #[test]
    fn malformed_declaration_is_rejected_and_kept_byte_for_byte() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let malformed = br#"{"version":1,"id":"poem","source":"urn:source","extension":{"kept":"unknown"}}"#.to_vec();
            backend.put("document/test", &malformed).await.unwrap();
            let error = load(&backend, "document/test").await.unwrap_err();
            assert!(error.contains("unchanged"));
            assert_eq!(backend.get("document/test").await.unwrap(), Some(malformed));
        });
    }
}
