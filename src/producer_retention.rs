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

pub fn note_digest(note: &redshank_model::Annotation) -> Result<String, String> {
    serde_json::to_vec(note)
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
        .map_err(|e| e.to_string())
}

/// Produce the native owner's selection record; the complete source stays backing.
pub(crate) fn export_knot_selection(
    address: &str,
    source: &str,
    expected: &KnotSelection,
) -> Result<(serde_json::Value, Vec<u8>), String> {
    use knot_composition::{CollectionItem, DocumentAnchor, ItemKind};
    if blake3::hash(source.as_bytes()).to_hex().as_str() != expected.source_hash {
        return Err("The observed Knot source changed".into());
    }
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
            export_knot_selection("mere://knot/source", source, &selected).unwrap();
        assert_eq!(export["text"], "café");
        assert_eq!(backing, source.as_bytes());
        assert!(
            export_knot_selection("mere://knot/source", "changed café tail", &selected).is_err()
        );
        assert!(
            export_knot_selection(
                "mere://knot/source",
                source,
                &KnotSelection {
                    end: 12,
                    ..selected.clone()
                }
            )
            .is_err()
        );
        let (other, _) = export_knot_selection("mere://knot/other", source, &selected).unwrap();
        assert_ne!(
            export["id"], other["id"],
            "identical source bytes do not identify their owner document"
        );
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
