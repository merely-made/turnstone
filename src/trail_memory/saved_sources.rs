// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Exact-id backend for the app-owned saved-source projection.

use crate::saved_sources::{Admission, Content, Entry, Target};
use eidetic::ManifestId;
use muniment::FjallStore;

/// Load each supplied live-facet admission independently. Repeated references
/// remain separate entries with their original occurrence positions; the
/// search projection decides when identities are deduplicated.
pub(super) fn load(store: &mut FjallStore, admissions: &[Admission]) -> Vec<Entry> {
    admissions
        .iter()
        .map(|admission| match load_content(store, admission) {
            Ok(content) => Entry {
                admission: admission.clone(),
                content: Some(content),
                error: None,
            },
            Err(error) => Entry {
                admission: admission.clone(),
                content: None,
                error: Some(error),
            },
        })
        .collect()
}

/// Route tagged captures only through the separately owned sealed store.
pub(super) fn load_with_sealed(
    store: &mut FjallStore,
    mut sealed: Option<&mut FjallStore>,
    sealer: Option<&dyn eidetic::PayloadSealer>,
    admissions: &[Admission],
) -> Vec<Entry> {
    admissions
        .iter()
        .map(|admission| {
            let result = if admission.key.annotation_manifest.starts_with("sealed-v1:") {
                match (sealed.as_deref_mut(), sealer) {
                    (Some(store), Some(sealer)) => {
                        super::sealed_capture::content(store, sealer, admission)
                    }
                    _ => Err("Unlock the profile to read this sealed source".into()),
                }
            } else {
                load_content(store, admission)
            };
            match result {
                Ok(content) => Entry {
                    admission: admission.clone(),
                    content: Some(content),
                    error: None,
                },
                Err(error) => Entry {
                    admission: admission.clone(),
                    content: None,
                    error: Some(error),
                },
            }
        })
        .collect()
}

pub(super) fn resolve_with_sealed(
    store: &mut FjallStore,
    sealed: Option<&mut FjallStore>,
    sealer: Option<&dyn eidetic::PayloadSealer>,
    admission: &Admission,
    target: &Target,
) -> Result<Content, String> {
    if target.key != admission.key {
        return Err("saved source target does not match the admitted node reference".into());
    }
    let content = if admission.key.annotation_manifest.starts_with("sealed-v1:") {
        let store = sealed.ok_or("sealed source store unavailable")?;
        let sealer = sealer.ok_or("unlock the profile to read this sealed source")?;
        super::sealed_capture::content(store, sealer, admission)?
    } else {
        load_content(store, admission)?
    };
    crate::saved_sources::validate_selector(&content, target)?;
    Ok(content)
}

/// Resolve one result against its exact admission and canonical-text version.
/// A stale/wrong target is refused before it can produce a reader document.
pub(super) fn resolve(
    store: &mut FjallStore,
    admission: &Admission,
    target: &Target,
) -> Result<Content, String> {
    if target.key != admission.key {
        return Err("saved source target does not match the admitted node reference".into());
    }
    let content = load_content(store, admission)?;
    crate::saved_sources::validate_selector(&content, target)?;
    Ok(content)
}

fn load_content(store: &mut FjallStore, admission: &Admission) -> Result<Content, String> {
    if let Some(error) = admission.error.as_ref() {
        return Err(error.clone());
    }
    let annotation_id = parse_manifest(&admission.key.annotation_manifest)?;
    let record = pollster::block_on(fleece::load_fleece_annotation(store, annotation_id))
        .map_err(|error| format!("saved source annotation validation: {error}"))?
        .ok_or_else(|| "saved source annotation is unavailable".to_string())?;
    let evidence = record
        .extraction
        .capture
        .evidence
        .as_ref()
        .ok_or_else(|| "saved source has no authoritative acquisition evidence".to_string())?;
    let raw_id = evidence
        .raw_manifest
        .ok_or_else(|| "saved source has no raw-response manifest".to_string())?;
    let captured_at_ms = evidence.captured_at.0;
    let stored = crate::action::StoredSourceDocument {
        raw_manifest: raw_id.to_string(),
        annotation_manifest: annotation_id.to_string(),
    };
    crate::trail_memory::verify_source_document(store, &stored)?;

    let extraction = record.extraction;
    let canonical = extraction.canonical_text_record;
    let text = canonical.canonical_text;
    let text_hash = extraction.canonical_text_hash.to_string();
    let recomputed = eidetic::Hash::of(text.as_bytes()).to_string();
    if recomputed != text_hash {
        return Err("saved source canonical-text hash mismatch".into());
    }
    let capture = extraction.capture;
    Ok(Content {
        title: admission.title.clone(),
        origin: capture.canonical_source,
        captured_at_ms,
        text,
        text_hash,
        raw_manifest: raw_id.to_string(),
    })
}

fn parse_manifest(value: &str) -> Result<ManifestId, String> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .map_err(|error| format!("saved source manifest identity is malformed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::FetchedDocument;
    use crate::panes::{GraphId, SessionId};
    use crate::saved_sources::{Key, Selector};
    use eidetic::Backend;

    struct FixtureSealer;
    impl eidetic::PayloadSealer for FixtureSealer {
        fn seal(
            &self,
            _: &eidetic::Hash,
            clear: &[u8],
        ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
            Ok((
                clear.to_vec(),
                eidetic::SealedBlobRef {
                    epoch: eidetic::SealEpochId([1; 16]),
                    format: "actor-fixture".into(),
                },
            ))
        }

        fn unseal(
            &self,
            _: &eidetic::Hash,
            _: &eidetic::SealedBlobRef,
            ciphertext: &[u8],
        ) -> eidetic::Result<Vec<u8>> {
            Ok(ciphertext.to_vec())
        }
    }

    fn fixture_custody() -> crate::trail_memory::SourceCustody {
        let sealer: std::sync::Arc<dyn eidetic::PayloadSealer + Send + Sync> =
            std::sync::Arc::new(FixtureSealer);
        crate::trail_memory::SourceCustody {
            sealer,
            epoch: std::sync::Arc::new(|| Some(1)),
        }
    }

    fn fixture(label: &str) -> FetchedDocument {
        let body = format!(
            "<html><main><h1>{label}</h1><p>first 🐦 exact saved source passage</p></main></html>"
        );
        FetchedDocument {
            bytes: body.as_bytes().to_vec(),
            body,
            content_type: Some("text/html; charset=utf-8".into()),
            effective_url: Some(format!("https://saved.test/{label}")),
            acquired_at_ms: 1_700_000_123_456,
        }
    }

    fn admission(stored: &crate::action::StoredSourceDocument, occurrence: usize) -> Admission {
        Admission {
            key: Key {
                session: SessionId::new(),
                graph: GraphId::new(),
                node: uuid::Uuid::new_v4(),
                annotation_manifest: stored.annotation_manifest.clone(),
            },
            occurrence,
            title: "Saved source fixture".into(),
            url: "https://saved.test/fixture".into(),
            error: None,
        }
    }

    #[test]
    fn exact_live_admissions_reopen_offline_and_reject_wrong_or_missing_ids() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = FjallStore::open(directory.path()).unwrap();
        let stored =
            crate::trail_memory::capture_source_document_in(&mut store, &fixture("A")).unwrap();
        let admitted = admission(&stored, 0);
        let entries = load(&mut store, std::slice::from_ref(&admitted));
        let content = entries[0]
            .content
            .as_ref()
            .expect("local saved source loads");
        assert_eq!(content.origin, "https://saved.test/A");
        assert!(content.text.contains("first 🐦 exact saved source passage"));
        assert_eq!(content.captured_at_ms, 1_700_000_123_456);
        assert_eq!(
            eidetic::Hash::of(content.text.as_bytes()).to_string(),
            content.text_hash
        );

        drop(store);
        let mut reopened = FjallStore::open(directory.path()).unwrap();
        let reopened_entry = load(&mut reopened, std::slice::from_ref(&admitted));
        let reopened_content = reopened_entry[0]
            .content
            .as_ref()
            .expect("the exact annotation is available after closing and reopening");
        assert_eq!(reopened_content.text, content.text);

        let mut wrong = admitted.clone();
        wrong.key.node = uuid::Uuid::new_v4();
        let wrong_target = Target {
            key: wrong.key.clone(),
            text_hash: reopened_content.text_hash.clone(),
            selector: None,
        };
        assert!(resolve(&mut reopened, &admitted, &wrong_target).is_err());

        let mut missing = admitted.clone();
        missing.key.annotation_manifest = eidetic::ManifestId::of_blob(b"missing").to_string();
        assert!(load(&mut reopened, &[missing])[0].content.is_none());
    }

    #[test]
    fn canonical_text_hash_and_unicode_scalar_range_are_enforced() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = FjallStore::open(directory.path()).unwrap();
        let stored =
            crate::trail_memory::capture_source_document_in(&mut store, &fixture("B")).unwrap();
        let admitted = admission(&stored, 1);
        let content = load(&mut store, std::slice::from_ref(&admitted))[0]
            .content
            .clone()
            .unwrap();
        let start = content.text.find('🐦').unwrap();
        let scalar_start = content.text[..start].chars().count();
        let target = Target {
            key: admitted.key.clone(),
            text_hash: content.text_hash.clone(),
            selector: Some(Selector {
                start: scalar_start,
                end: scalar_start + 1,
            }),
        };
        assert_eq!(
            crate::saved_sources::validate_selector(&content, &target).unwrap(),
            Some("🐦".into())
        );
        let bad = Target {
            text_hash: eidetic::Hash::of(b"substituted").to_string(),
            ..target.clone()
        };
        assert!(resolve(&mut store, &admitted, &bad).is_err());
        let bad_range = Target {
            selector: Some(Selector {
                start: 999,
                end: 1000,
            }),
            ..target
        };
        assert!(crate::saved_sources::validate_selector(&content, &bad_range).is_err());
    }

    #[test]
    fn corrupt_typed_annotation_becomes_unavailable_without_origin_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = FjallStore::open(directory.path()).unwrap();
        let stored =
            crate::trail_memory::capture_source_document_in(&mut store, &fixture("C")).unwrap();
        let admitted = admission(&stored, 0);
        let id = parse_manifest(&admitted.key.annotation_manifest).unwrap();
        pollster::block_on(store.put(
            &format!("blob:{}", id.0.to_hex()),
            b"corrupt typed annotation bytes",
        ))
        .unwrap();
        let result = load(&mut store, std::slice::from_ref(&admitted));
        assert!(result[0].content.is_none());
        assert!(result[0].error.as_deref().is_some_and(|error| {
            error.contains("validation") || error.contains("hash") || error.contains("decode")
        }));
        let target = Target {
            key: admitted.key.clone(),
            text_hash: eidetic::Hash::of(b"not from origin").to_string(),
            selector: None,
        };
        assert!(resolve(&mut store, &admitted, &target).is_err());
    }

    #[test]
    fn actor_load_and_read_require_current_owner_generation_and_exact_store() {
        use crate::trail_memory::{TrailCommand, spawn_trail_for_session};
        use std::time::Duration;

        let root = tempfile::tempdir().unwrap();
        let session = SessionId::new();
        let directory = root
            .path()
            .join("sessions")
            .join(session.as_uuid().to_string())
            .join("memory");
        std::fs::create_dir_all(&directory).unwrap();
        let stored = {
            let mut store = FjallStore::open(&directory).unwrap();
            crate::trail_memory::capture_source_document_in(&mut store, &fixture("actor")).unwrap()
        };
        let admitted = Admission {
            key: Key {
                session,
                graph: GraphId::new(),
                node: uuid::Uuid::new_v4(),
                annotation_manifest: stored.annotation_manifest.clone(),
            },
            occurrence: 0,
            title: "Actor saved source".into(),
            url: "https://saved.test/actor".into(),
            error: None,
        };
        let generation = uuid::Uuid::new_v4();
        let old_generation = uuid::Uuid::new_v4();
        let (actor, updates) = spawn_trail_for_session(
            std::sync::Arc::new(|| {}),
            Some(directory.clone()),
            Default::default(),
            generation,
        );

        let locked_request = uuid::Uuid::new_v4();
        assert!(actor.command(TrailCommand::LoadSavedSources {
            generation,
            request: locked_request,
            session,
            admissions: vec![admitted.clone()],
            directory: directory.clone(),
        }));
        match updates.recv_timeout(Duration::from_secs(5)).unwrap() {
            crate::action::Update::SavedSourcesLoaded { result, .. } => {
                assert!(result.unwrap_err().contains("Unlock the profile"));
            }
            _ => panic!("expected locked legacy-source refusal"),
        }
        assert!(actor.command(TrailCommand::SetCustody(Some(fixture_custody()))));

        let load_request = uuid::Uuid::new_v4();
        assert!(actor.command(TrailCommand::LoadSavedSources {
            generation,
            request: load_request,
            session,
            admissions: vec![admitted.clone()],
            directory: directory.clone(),
        }));
        let entries = match updates.recv_timeout(Duration::from_secs(5)).unwrap() {
            crate::action::Update::SavedSourcesLoaded {
                generation: actual_generation,
                request,
                session: actual_session,
                admissions,
                result,
            } => {
                assert_eq!(actual_generation, generation);
                assert_eq!(request, load_request);
                assert_eq!(actual_session, session);
                assert_eq!(admissions, vec![admitted.clone()]);
                result.unwrap()
            }
            _ => panic!("expected saved-source load result"),
        };
        let content = entries[0]
            .content
            .clone()
            .expect("exact local admission loads");
        assert_eq!(content.origin, "https://saved.test/actor");

        let passage_start = content.text.find("exact saved source passage").unwrap();
        let scalar_start = content.text[..passage_start].chars().count();
        let target = Target {
            key: admitted.key.clone(),
            text_hash: content.text_hash.clone(),
            selector: Some(Selector {
                start: scalar_start,
                end: scalar_start + "exact saved source passage".chars().count(),
            }),
        };
        let read_request = uuid::Uuid::new_v4();
        let reader_node = uuid::Uuid::new_v4();
        let reader_url = crate::saved_sources::address(&target).unwrap();
        assert!(actor.command(TrailCommand::ReadSavedSource {
            generation,
            request: read_request,
            node: reader_node,
            url: reader_url.clone(),
            target: target.clone(),
            admission: admitted.clone(),
            directory: directory.clone(),
        }));
        match updates.recv_timeout(Duration::from_secs(5)).unwrap() {
            crate::action::Update::SavedSourceRead {
                generation: actual_generation,
                request,
                node,
                url,
                target: actual_target,
                result,
            } => {
                assert_eq!(actual_generation, generation);
                assert_eq!(request, read_request);
                assert_eq!(node, reader_node);
                assert_eq!(url, reader_url);
                assert_eq!(actual_target, target);
                assert_eq!(result.unwrap(), content);
            }
            _ => panic!("expected saved-source read result"),
        }

        for (request_generation, request_session, request_directory, message) in [
            (old_generation, session, directory.clone(), "generation"),
            (generation, SessionId::new(), directory.clone(), "owner"),
            (
                generation,
                session,
                directory.with_file_name("other-memory"),
                "directory",
            ),
        ] {
            let request = uuid::Uuid::new_v4();
            assert!(actor.command(TrailCommand::LoadSavedSources {
                generation: request_generation,
                request,
                session: request_session,
                admissions: vec![admitted.clone()],
                directory: request_directory,
            }));
            match updates.recv_timeout(Duration::from_secs(5)).unwrap() {
                crate::action::Update::SavedSourcesLoaded { result, .. } => {
                    assert!(
                        result
                            .unwrap_err()
                            .contains("another session opening or store"),
                        "wrong {message} is refused"
                    );
                }
                _ => panic!("expected refused saved-source load"),
            }
        }

        for (request_generation, request_directory, message) in [
            (generation, directory.clone(), "owner"),
            (old_generation, directory.clone(), "generation"),
            (
                generation,
                directory.with_file_name("other-memory"),
                "directory",
            ),
        ] {
            let target = if message == "owner" {
                Target {
                    key: Key {
                        session: SessionId::new(),
                        ..target.key.clone()
                    },
                    ..target.clone()
                }
            } else {
                target.clone()
            };
            assert!(actor.command(TrailCommand::ReadSavedSource {
                generation: request_generation,
                request: uuid::Uuid::new_v4(),
                node: reader_node,
                url: reader_url.clone(),
                target,
                admission: admitted.clone(),
                directory: request_directory,
            }));
            match updates.recv_timeout(Duration::from_secs(5)).unwrap() {
                crate::action::Update::SavedSourceRead { result, .. } => {
                    assert!(
                        result
                            .unwrap_err()
                            .contains("another session opening or attachment"),
                        "wrong read {message} is refused"
                    );
                }
                _ => panic!("expected refused saved-source read"),
            }
        }

        let (ack, receive) = std::sync::mpsc::sync_channel(1);
        assert!(actor.command(TrailCommand::Release(ack)));
        receive.recv_timeout(Duration::from_secs(5)).unwrap();
        actor.join().unwrap();
    }
}
