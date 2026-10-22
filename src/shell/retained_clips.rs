// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::{NodeSessions, Shell};
use crate::retained_clip::{ClipRequest, ENGINE_ID, request};

impl Shell {
    /// Mirror decoder/output observations into the retained document view.
    /// This changes presentation only; it never edits the retained source.
    pub(super) fn refresh_retained_clip_playback(&mut self) {
        let preview = self.app.redshank.retained_voice_preview();
        let members: Vec<_> = self
            .app
            .content
            .live_nodes()
            .filter(|member| {
                self.app
                    .content
                    .facts(*member)
                    .is_some_and(|facts| facts.engine == ENGINE_ID)
            })
            .collect();
        for member in members {
            let Some(session) = self.content_sessions.session_mut(&member) else {
                continue;
            };
            let Some(session) = session
                .as_any()
                .downcast_mut::<mere_document_lanes::SmolwebDocumentSession>()
            else {
                continue;
            };
            let document = session.document_mut().document();
            let Ok(Some(ClipRequest::Read(id))) = request(&document.address) else {
                continue;
            };
            let value = preview
                .as_ref()
                .filter(|p| p.id == format!("clip:{}", id.0.to_hex()))
                .map(|p| match &p.state {
                    redshank_playback::PreviewState::Loading => "Loading".to_string(),
                    redshank_playback::PreviewState::Playing => {
                        format!("Playing {} ms", p.position_ms)
                    },
                    redshank_playback::PreviewState::Ended => format!("Ended {} ms", p.position_ms),
                    redshank_playback::PreviewState::Unavailable(reason) => {
                        format!("Unavailable: {reason}")
                    },
                })
                .unwrap_or_else(|| "Ready".into());
            if !document.blocks.iter().any(|b| matches!(b, inker::Block::MetadataRow { label, value: held } if label == "Voice note playback" && *held != value)) { continue; }
            let mut document = document.clone();
            for block in &mut document.blocks {
                if let inker::Block::MetadataRow { label, value: held } = block
                    && label == "Voice note playback"
                {
                    *held = value.clone();
                }
            }
            session.replace_document(document);
        }
    }
}

impl Shell {
    pub(super) fn retain_producer_selection(
        &mut self,
        choice: crate::app::research_note_arms::NoteChoice,
        selection: crate::producer_retention::ProducerSelection,
    ) {
        let result = (|| {
            if self.app.note_choice(choice.member).as_ref() != Some(&choice)
                || self.app.browser_command_target().map(|(member, _)| member)
                    != Some(choice.member)
            {
                return Err("The displayed source or retention destination changed".into());
            }
            let root = self.app.clip_destination()?;
            let policy = crate::retained_clip::limits()?;
            let (export, backing, voice, capture_id) = match &selection {
                crate::producer_retention::ProducerSelection::Knot(expected) => {
                    let session = self
                        .content_sessions
                        .session_mut(&choice.member)
                        .and_then(|session| {
                            session
                                .as_any()
                                .downcast_mut::<crate::knot_authoring::KnotDocumentSession>()
                        })
                        .ok_or("The selected Knot editor is no longer open")?;
                    if session.address() != choice.address {
                        return Err("The selected Knot document changed".into());
                    }
                    let (export, backing) = session.selected_retained_export(expected)?;
                    let id = export
                        .get("id")
                        .and_then(|id| id.as_str())
                        .ok_or("Knot export has no native identity")?
                        .to_owned();
                    (export, Some(backing), None, id)
                },
                crate::producer_retention::ProducerSelection::Redshank { id, digest } => {
                    let model = self.app.redshank.model();
                    let note = model
                        .annotations
                        .get(id)
                        .ok_or("The selected Redshank note is no longer available")?;
                    if crate::producer_retention::note_digest(note)? != *digest
                        || choice.address != format!("mere://redshank/note/{}", id.0)
                    {
                        return Err("The selected Redshank note changed".into());
                    }
                    let export = model
                        .export_selected_note(id)
                        .map_err(|e| format!("Redshank export refused: {e:?}"))?;
                    if matches!(note.body, redshank_model::NoteBody::Audio { .. }) {
                        return Err("This voice note needs body bytes from its source owner before retention".into());
                    }
                    (export, None, None, format!("redshank:{digest}"))
                },
            };
            let context = fleece::portable_clip::CaptureContext {
                source: eidetic::clip_profile::ClipSourceV1 {
                    owner: None,
                    entity: choice.address.clone(),
                    revision: None,
                },
                capture_id,
                provenance: eidetic::ProvenanceRecord::self_imported(
                    "explicit owner-selected content",
                ),
                trust: eidetic::TrustEnvelope::self_asserted(),
                captured_at: eidetic::Timestamp(crate::denizen::now_ms()),
            };
            let source = match &selection {
                crate::producer_retention::ProducerSelection::Knot(_) => {
                    crate::retained_clip::NativeClip::Knot {
                        export: &export,
                        backing: backing.as_deref().ok_or("Knot source backing missing")?,
                    }
                },
                crate::producer_retention::ProducerSelection::Redshank { .. } => {
                    crate::retained_clip::NativeClip::Redshank {
                        export: &export,
                        voice_body: voice,
                    }
                },
            };
            crate::retained_clip::retain_native(
                &root,
                source,
                &context,
                policy,
                self.app.clip_sealer()?,
            )
        })();
        match result {
            Ok(id) => {
                self.act(crate::action::Action::OpenAddress(
                    crate::retained_clip::address(id),
                ));
                self.run_effects(vec![crate::action::Effect::SaveSession]);
            },
            Err(error) => self.app.note_retained_clip_refusal(error),
        }
    }
}
