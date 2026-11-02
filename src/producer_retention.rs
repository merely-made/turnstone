// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Captured source offers over the existing Knot/Redshank owner contracts.
use crate::{
    action::{Action, Effect},
    app::{App, research_note_arms::NoteChoice},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotSelection {
    pub source_hash: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProducerSelection {
    Knot(KnotSelection),
    Redshank {
        id: redshank_model::AnnotationId,
        digest: String,
    },
}

struct HashWriter(blake3::Hasher);
impl std::io::Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn note_digest(note: &redshank_model::Annotation) -> Result<String, String> {
    let mut writer = HashWriter(blake3::Hasher::new());
    serde_json::to_writer(&mut writer, note).map_err(|e| e.to_string())?;
    Ok(writer.0.finalize().to_hex().to_string())
}

fn checked_bound(
    parts: &[usize],
    multiplier: usize,
    overhead: usize,
    max: usize,
) -> Result<(), String> {
    let total = parts
        .iter()
        .try_fold(0usize, |sum, part| sum.checked_add(*part))
        .and_then(|sum| sum.checked_mul(multiplier))
        .and_then(|sum| sum.checked_add(overhead))
        .ok_or("Producer selection size overflow")?;
    if total > max {
        return Err("Producer selection exceeds the available byte limit".into());
    }
    Ok(())
}

struct CountWriter {
    count: usize,
    max: usize,
}
impl std::io::Write for CountWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.count = self
            .count
            .checked_add(bytes.len())
            .filter(|n| *n <= self.max)
            .ok_or_else(|| {
                std::io::Error::other("Producer selection exceeds the available byte limit")
            })?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Bound the borrowed complete note and possible source before the owner copies either.
/// The serialized annotation includes representation and fingerprint; the complete
/// library item includes every source variant and its metadata. The 18x multiplier covers at most 3x file-URL percent expansion followed by
/// up to 6x JSON escaping. It bounds named serialized payload, not allocator
/// overhead, RSS, or exact wire size.
pub(crate) fn export_redshank_selection(
    model: &redshank_model::RedshankModel,
    id: &redshank_model::AnnotationId,
    digest: &str,
    max_bytes: usize,
) -> Result<serde_json::Value, String> {
    let note = model
        .annotations
        .get(id)
        .ok_or("The selected Redshank note is no longer available")?;
    if note.privacy != redshank_model::NotePrivacy::Shareable {
        return Err("The selected Redshank note is private".into());
    }
    let mut count = CountWriter {
        count: 0,
        max: max_bytes,
    };
    serde_json::to_writer(&mut count, note).map_err(|e| e.to_string())?;
    if let Some(item) = model.library.get(&note.target.item_id) {
        serde_json::to_writer(&mut count, item).map_err(|e| e.to_string())?;
    }
    checked_bound(&[count.count], 18, 1024, max_bytes)?;
    if note_digest(note)? != digest {
        return Err("The selected Redshank note changed".into());
    }
    model
        .export_selected_note(id)
        .map_err(|e| format!("Redshank export refused: {e:?}"))
}

/// Produce the native owner's selection record; the complete source stays backing.
pub(crate) fn export_knot_selection(
    address: &str,
    source: &str,
    expected: &KnotSelection,
    max_bytes: usize,
) -> Result<(serde_json::Value, Vec<u8>), String> {
    use knot_composition::{CollectionItem, DocumentAnchor, ItemKind};
    if blake3::hash(source.as_bytes()).to_hex().as_str() != expected.source_hash {
        return Err("The observed Knot source changed".into());
    }
    let quote = source
        .get(expected.start..expected.end)
        .ok_or("Knot selection range is invalid")?;
    // Export repeats the quote and may JSON-escape address and text; backing
    // copies the entire source. Refuse before constructing the owner anchor.
    let export_inputs = address
        .len()
        .checked_add(quote.len())
        .ok_or("Producer selection size overflow")?;
    let export_bound = export_inputs
        .checked_mul(12)
        .ok_or("Producer selection size overflow")?;
    checked_bound(&[source.len(), export_bound], 1, 2048, max_bytes)?;
    let anchor = DocumentAnchor::capture(address, source, expected.start..expected.end)
        .map_err(|e| format!("Knot selection refused: {e:?}"))?;
    let mut item = CollectionItem::new(
        ItemKind::Passage,
        "Knot selection",
        anchor.exact_quote.clone(),
    )
    .with_document_source(anchor);
    let identity = serde_json::to_vec(&(address, expected)).map_err(|e| e.to_string())?;
    item.id = format!("selection:{}", blake3::hash(&identity).to_hex());
    let export = knot_composition::selected_export::export_selected_content(&item)
        .map_err(|e| e.to_string())?;
    Ok((export, source.as_bytes().to_vec()))
}

impl App {
    pub(crate) fn producer_retention_action(&self) -> Option<(String, Action)> {
        self.clip_destination().ok()?;
        let (member, address) = self.browser_command_target()?;
        let choice = self.note_choice(member)?;
        let selection = if let Some(knot) = self
            .knot_documents()
            .iter()
            .find(|doc| doc.address == address && doc.status == "local")
        {
            ProducerSelection::Knot(knot.retained_selection.clone()?)
        } else {
            let id = redshank_model::AnnotationId(
                address.strip_prefix("mere://redshank/note/")?.to_string(),
            );
            let note = self.redshank.model().annotations.get(&id)?;
            if note.privacy != redshank_model::NotePrivacy::Shareable {
                return None;
            }
            ProducerSelection::Redshank {
                id,
                digest: note_digest(note).ok()?,
            }
        };
        let label = match &selection {
            ProducerSelection::Knot(_) => "Retain selected Knot text",
            ProducerSelection::Redshank { .. } => "Retain selected Redshank note",
        };
        Some((
            label.into(),
            Action::RetainProducerSelection { choice, selection },
        ))
    }

    pub(crate) fn retain_producer_selection(
        &mut self,
        choice: NoteChoice,
        selection: ProducerSelection,
    ) -> Vec<Effect> {
        if self.note_choice(choice.member).as_ref() != Some(&choice)
            || self.clip_destination().is_err()
            || self.browser_command_target().map(|(member, _)| member) != Some(choice.member)
        {
            self.note_retained_clip_refusal(
                "The retained-content destination or source selection changed".into(),
            );
            return vec![Effect::Redraw];
        }
        vec![Effect::RetainProducerSelection { choice, selection }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_selection_refuses_changed_source_and_split_unicode_without_guessing() {
        let source = "private café tail";
        let selected = KnotSelection {
            source_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
            start: 8,
            end: 13,
        };
        let (export, backing) =
            export_knot_selection("mere://knot/source", source, &selected, 100_000).unwrap();
        assert_eq!(export["text"], "café");
        assert_eq!(backing, source.as_bytes());
        assert!(
            export_knot_selection(
                "mere://knot/source",
                "changed café tail",
                &selected,
                100_000
            )
            .is_err()
        );
        assert!(
            export_knot_selection(
                "mere://knot/source",
                source,
                &KnotSelection {
                    end: 12,
                    ..selected.clone()
                },
                100_000
            )
            .is_err()
        );
        let (other, _) =
            export_knot_selection("mere://knot/other", source, &selected, 100_000).unwrap();
        assert_ne!(
            export["id"], other["id"],
            "identical source bytes do not identify their owner document"
        );
    }

    #[test]
    fn extraction_limits_cover_backing_metadata_and_fingerprint() {
        use redshank_model::{
            Annotation, AnnotationId, AudioFingerprint, ItemId, NoteBody, NotePrivacy,
            RedshankModel, RepresentationReceipt, TimedTarget,
        };
        let source = "private café tail";
        let selection = KnotSelection {
            source_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
            start: 8,
            end: 13,
        };
        assert!(export_knot_selection("mere://knot/source", source, &selection, 8).is_err());
        let mut model = RedshankModel::default();
        let id = AnnotationId("note".into());
        let note = Annotation {
            id: id.clone(),
            target: TimedTarget {
                item_id: ItemId("item".into()),
                offset_ms: 1,
                end_offset_ms: None,
                pressed_offset_ms: None,
                representation: RepresentationReceipt::default(),
                fingerprint: None,
            },
            body: NoteBody::Text {
                plain_text: "small".into(),
            },
            created_at_ms: 1,
            privacy: NotePrivacy::Shareable,
        };
        let digest = note_digest(&note).unwrap();
        assert_eq!(
            digest,
            blake3::hash(&serde_json::to_vec(&note).unwrap())
                .to_hex()
                .to_string()
        );
        model.annotations.insert(id.clone(), note.clone());
        assert!(export_redshank_selection(&model, &id, &digest, 16_384).is_ok());
        let mut metadata = note.clone();
        metadata.target.representation.etag = Some("x".repeat(4096));
        let mut fingerprint = note;
        fingerprint.target.fingerprint = Some(AudioFingerprint {
            version: 1,
            frame_ms: 100,
            anchor_offset_ms: 3000,
            frames: vec![[255; 16]; 30],
        });
        for large in [metadata, fingerprint] {
            let digest = note_digest(&large).unwrap();
            model.annotations.insert(id.clone(), large);
            assert!(export_redshank_selection(&model, &id, &digest, 16_384).is_err());
            assert_eq!(
                export_redshank_selection(&model, &id, &digest, 1_000_000).unwrap(),
                model.export_selected_note(&id).unwrap()
            );
        }
        assert!(checked_bound(&[usize::MAX, 1], 1, 0, usize::MAX).is_err());
    }

    #[test]
    fn referenced_source_is_bounded_before_owner_export() {
        use redshank_model::{
            Annotation, AnnotationId, ItemId, LibraryItem, MediaSource, NoteBody, NotePrivacy,
            RedshankModel, RepresentationReceipt, TimedTarget,
        };
        for source in [
            MediaSource::Local {
                path: format!("/tmp/{}", "café #".repeat(1000)),
            },
            MediaSource::Enclosure {
                url: format!("https://example.invalid/{}", "x".repeat(6000)),
            },
        ] {
            let mut model = RedshankModel::default();
            let item = ItemId("item".into());
            model.library.insert(
                item.clone(),
                LibraryItem::LocalAudio {
                    id: item.clone(),
                    title: "small".into(),
                    source: MediaSource::Local {
                        path: "/tmp/small".into(),
                    },
                },
            );
            let id = AnnotationId("note".into());
            let note = Annotation {
                id: id.clone(),
                target: TimedTarget {
                    item_id: item,
                    offset_ms: 1,
                    end_offset_ms: None,
                    pressed_offset_ms: None,
                    representation: RepresentationReceipt::default(),
                    fingerprint: None,
                },
                body: NoteBody::Text {
                    plain_text: "small".into(),
                },
                created_at_ms: 1,
                privacy: NotePrivacy::Shareable,
            };
            let digest = note_digest(&note).unwrap();
            model.annotations.insert(id.clone(), note);
            assert!(export_redshank_selection(&model, &id, &digest, 16_384).is_ok());
            model
                .library
                .get_mut(&ItemId("item".into()))
                .unwrap()
                .replace_source(source);
            assert!(export_redshank_selection(&model, &id, &digest, 16_384).is_err());
            assert_eq!(
                export_redshank_selection(&model, &id, &digest, 1_000_000).unwrap(),
                model.export_selected_note(&id).unwrap()
            );
        }
    }

    #[test]
    fn a_retention_action_cannot_follow_a_replaced_or_foreign_member() {
        let root = tempfile::tempdir().unwrap();
        let mut app = App::test_stub_at(root.path().into());
        app.update(Action::OpenAddress("https://example.invalid/source".into()));
        let (member, _) = app.browser_command_target().unwrap();
        let choice = app.note_choice(member).unwrap();
        let selection = ProducerSelection::Knot(KnotSelection {
            source_hash: "hash".into(),
            start: 1,
            end: 2,
        });
        let action = Action::RetainProducerSelection {
            choice: choice.clone(),
            selection: selection.clone(),
        };
        assert_eq!(crate::ring::ring_of(&action), crate::ring::Ring::HostOnly);
        assert!(
            app.retain_producer_selection(choice.clone(), selection.clone())
                .iter()
                .any(|e| matches!(e, Effect::RetainProducerSelection { .. }))
        );
        app.update(Action::OpenAddress(
            "https://example.invalid/replacement".into(),
        ));
        assert!(
            !app.retain_producer_selection(choice, selection)
                .iter()
                .any(|e| matches!(e, Effect::RetainProducerSelection { .. }))
        );
    }
}
