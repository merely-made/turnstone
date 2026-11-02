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
        let request = self
            .latest_clip_retention
            .as_ref()
            .filter(|status| {
                status.choice == choice
                    && status.result.is_none()
                    && !self.pending_clip_retains.contains_key(&status.request)
            })
            .map(|status| status.request)
            .unwrap_or_else(|| {
                let request = uuid::Uuid::new_v4();
                self.latest_clip_retention = Some(super::clip_retaining::RetainStatus {
                    request,
                    choice: choice.clone(),
                    result: None,
                });
                request
            });
        let result: Result<super::clip_retaining::PendingClipRetain, String> = (|| {
            if self.app.note_choice(choice.member).as_ref() != Some(&choice)
                || self.app.browser_command_target().map(|(member, _)| member)
                    != Some(choice.member)
            {
                return Err("The displayed source or retention destination changed".into());
            }
            let root = self.app.clip_destination()?;
            let identity = self.app.identity.clone().ok_or(crate::identity::PENDING)?;
            let binding = self
                .custody_binding
                .as_ref()
                .ok_or(crate::identity::PENDING)?;
            if !std::sync::Arc::ptr_eq(&identity, &binding.root) {
                return Err(crate::identity::PENDING.into());
            }
            let bound = binding.bound.clone();
            let epoch = bound.epoch().ok_or(crate::identity::PENDING)?;
            let policy = crate::retained_clip::limits()?;
            let limits = self.app.research.storage_limits()?;
            let input_limit = usize::try_from(policy.max_bytes)
                .map_err(|_| "Clip size limit is too large")?
                .checked_add(policy.max_record_bytes)
                .ok_or("Clip size limit overflow")?
                .min(limits.clip_queue.max_bytes);
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
                    let (export, backing) =
                        session.selected_retained_export(expected, input_limit)?;
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
                    if choice.address != format!("mere://redshank/note/{}", id.0) {
                        return Err("The selected Redshank note changed".into());
                    }
                    let export = crate::producer_retention::export_redshank_selection(
                        model,
                        id,
                        digest,
                        input_limit,
                    )?;
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
                    crate::retained_clip::owner_worker::OwnedNative::Knot {
                        export,
                        backing: backing.ok_or("Knot source backing missing")?,
                    }
                },
                crate::producer_retention::ProducerSelection::Redshank { .. } => {
                    crate::retained_clip::owner_worker::OwnedNative::Redshank {
                        export,
                        voice_body: voice,
                    }
                },
            };
            let claim = bound
                .claim()
                .ok_or("Profile custody changed before retention")?;
            if claim.epoch() != epoch {
                return Err("Profile custody changed before retention".into());
            }
            let access = crate::research::ClipAccess::from_bound(root.clone(), bound.clone());
            let response = self.clip_owner.retain(
                root.clone(),
                source,
                context,
                policy,
                access,
                epoch,
                limits,
            )?;
            Ok(super::clip_retaining::PendingClipRetain {
                choice: choice.clone(),
                selection: selection.clone(),
                root,
                identity,
                bound,
                epoch,
                response,
            })
        })();
        match result {
            Ok(pending) => {
                self.pending_clip_retains.insert(request, pending);
            },
            Err(error) => {
                if let Some(status) = self
                    .latest_clip_retention
                    .as_mut()
                    .filter(|status| status.request == request)
                {
                    status.record(request, Err(error.clone()));
                }
                self.app.note_retained_clip_refusal(error);
            },
        }
    }
}
