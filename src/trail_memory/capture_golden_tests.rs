// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's real Fleece annotator against the golden fixtures Eidetic
//! froze from this repository's own capture core at `cc9cc6d2`
//! (`tests/fixtures/capture_golden/`). Eidetic re-writes them with a stand-in
//! annotator; this proves Fleece's side is byte-identical too.

use super::capture_core::{FleeceAnnotator, neutral_document};
use super::{capture_source_document_in, sealed_capture, verify_source_document};
use crate::action::StoredSourceDocument;
use crate::content::FetchedDocument;
use crate::source_capture::{CaptureRequest, DepositReceipt, SourceIdentity};
use eidetic::capture::SourceAnnotator as _;
use eidetic::{Backend as _, Hash, MemoryBackend, PayloadSealer};
use serde_json::Value;

/// The fixture sealer Turnstone's tests used: XOR behind the hash hex.
struct FixtureSealer;

impl PayloadSealer for FixtureSealer {
    fn seal(
        &self,
        hash: &Hash,
        clear: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        let mut ciphertext = hash.to_hex().into_bytes();
        ciphertext.extend(clear.iter().map(|b| b ^ 0x5a));
        Ok((
            ciphertext,
            eidetic::SealedBlobRef {
                epoch: eidetic::SealEpochId([7; 16]),
                format: "fixture".into(),
            },
        ))
    }
    fn unseal(
        &self,
        hash: &Hash,
        marker: &eidetic::SealedBlobRef,
        cipher: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        if marker.format != "fixture" || !cipher.starts_with(hash.to_hex().as_bytes()) {
            return Err(eidetic::Error::new("refused"));
        }
        let clear: Vec<u8> = cipher[64..].iter().map(|b| b ^ 0x5a).collect();
        if Hash::of(&clear) != *hash {
            return Err(eidetic::Error::new("tampered"));
        }
        Ok(clear)
    }
}

type Kv = Vec<(String, Vec<u8>)>;

fn json(name: &str) -> Value {
    let path = format!(
        "{}/tests/fixtures/capture_golden/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn kv(name: &str) -> Kv {
    let Value::Array(entries) = json(name) else {
        panic!("{name} is not a dump");
    };
    entries
        .iter()
        .map(|entry| {
            let hex = entry["hex"].as_str().unwrap();
            let bytes = (0..hex.len() / 2)
                .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
                .collect();
            (entry["key"].as_str().unwrap().to_owned(), bytes)
        })
        .collect()
}

fn load(kv: &Kv) -> MemoryBackend {
    let store = MemoryBackend::new();
    for (key, value) in kv {
        pollster::block_on(store.put(key, value)).unwrap();
    }
    store
}

fn dump(store: &MemoryBackend) -> Kv {
    let mut keys = pollster::block_on(store.list("")).unwrap();
    keys.sort();
    keys.into_iter()
        .map(|key| {
            let value = pollster::block_on(store.get(&key)).unwrap().unwrap();
            (key, value)
        })
        .collect()
}

fn golden() -> (FetchedDocument, CaptureRequest, StoredSourceDocument) {
    let meta = json("sealed_meta.json");
    let doc = &meta["document"];
    let body = doc["body"].as_str().unwrap().to_owned();
    (
        FetchedDocument {
            bytes: body.as_bytes().to_vec(),
            body,
            effective_url: doc["effective_url"].as_str().map(str::to_owned),
            content_type: doc["content_type"].as_str().map(str::to_owned),
            acquired_at_ms: doc["acquired_at_ms"].as_u64().unwrap(),
        },
        serde_json::from_value(meta["request"].clone()).unwrap(),
        serde_json::from_value(meta["stored"].clone()).unwrap(),
    )
}

#[test]
fn turnstone_types_keep_the_stored_json_shapes() {
    let meta = json("sealed_meta.json");
    let (document, request, stored) = golden();
    assert_eq!(serde_json::to_value(&request).unwrap(), meta["request"]);
    assert_eq!(serde_json::to_value(&stored).unwrap(), meta["stored"]);
    assert_eq!(request.source, Some(SourceIdentity::of(&document)));
    // The neutral conversions are lossless in both directions.
    let neutral: eidetic::capture::CaptureRequest = (&request).into();
    assert_eq!(CaptureRequest::from(neutral.clone()), request);
    assert_eq!(
        neutral.source,
        Some(eidetic::capture::SourceIdentity::of(&neutral_document(
            &document
        )))
    );
    let receipt = DepositReceipt {
        saved: true,
        request,
        stored: Some(stored),
    };
    let round: DepositReceipt = eidetic::capture::DepositReceipt::from(&receipt).into();
    assert_eq!(round, receipt);
}

#[test]
fn fleece_annotator_builds_the_annotation_turnstone_stored() {
    let (document, _, _) = golden();
    let meta = json("source_document_meta.json");
    let record = FleeceAnnotator.annotate(&neutral_document(&document)).unwrap();
    FleeceAnnotator.validate(&record).unwrap();
    let bytes = eidetic::TypedPayload::serialize_to_bytes(&record).unwrap();
    assert_eq!(
        eidetic::ManifestId::of_blob(&bytes).to_string(),
        meta["stored"]["annotation_manifest"].as_str().unwrap()
    );
    let evidence = FleeceAnnotator.evidence(&record).unwrap();
    assert!(evidence.authoritative && evidence.source_dom && evidence.is_html());
    assert!(!evidence.has_replay_manifest);
}

#[test]
fn unsealed_source_document_writes_and_reads_the_stored_bytes() {
    let (document, _, _) = golden();
    let stored: StoredSourceDocument =
        serde_json::from_value(json("source_document_meta.json")["stored"].clone()).unwrap();
    let mut fresh = MemoryBackend::new();
    assert_eq!(
        capture_source_document_in(&mut fresh, &document).unwrap(),
        stored
    );
    assert_eq!(dump(&fresh), kv("source_document.kv.json"));

    let mut loaded = load(&kv("source_document.kv.json"));
    verify_source_document(&mut loaded, &stored).unwrap();
    let before = dump(&loaded);
    assert_eq!(
        capture_source_document_in(&mut loaded, &document).unwrap(),
        stored
    );
    assert_eq!(dump(&loaded), before, "re-capture is idempotent");
}

#[test]
fn sealed_capture_writes_reads_and_recovers_the_stored_bytes() {
    let (document, request, stored) = golden();
    let mut fresh = MemoryBackend::new();
    assert_eq!(
        sealed_capture::capture_request(&mut fresh, &FixtureSealer, &request, Some(&document))
            .unwrap(),
        stored
    );
    assert_eq!(dump(&fresh), kv("sealed_deposited.kv.json"));
    sealed_capture::acknowledge(&mut fresh, &FixtureSealer, &request).unwrap();
    assert_eq!(dump(&fresh), kv("sealed_acknowledged.kv.json"));

    let mut loaded = load(&kv("sealed_acknowledged.kv.json"));
    assert_eq!(
        sealed_capture::receipts(&mut loaded, &FixtureSealer).unwrap(),
        vec![DepositReceipt {
            saved: true,
            request: request.clone(),
            stored: Some(stored.clone()),
        }]
    );
    sealed_capture::verify(&mut loaded, &FixtureSealer, &request, &stored).unwrap();
    // Recovery with no live document re-verifies the stored pair.
    assert_eq!(
        sealed_capture::capture_request(&mut loaded, &FixtureSealer, &request, None).unwrap(),
        stored
    );
    let mut changed = request;
    changed.url = "https://other.example/".into();
    assert!(sealed_capture::capture_request(&mut loaded, &FixtureSealer, &changed, None).is_err());
}
