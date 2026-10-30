// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::redshank_episode_surface::{NoteCommand, NoteProjection, NoteScope};

impl App {
    pub(crate) fn redshank_control_scope(&self, item: &ItemId) -> Option<NoteScope> {
        if !self.redshank.model().library.contains_key(item) {
            return None;
        }
        let custody_epoch = self
            .research
            .clip_access()
            .ok()
            .and_then(|access| (access.epoch)());
        let selected = self.redshank.selected().cloned();
        let selected_capture_generation = selected
            .as_ref()
            .and_then(|id| self.redshank.pending_capture_generation(id));
        Some(NoteScope {
            session: self.session_id,
            generation: self.source_capture_generation,
            custody_epoch,
            item: item.clone(),
            selected,
            selected_capture_generation,
            control_generation: self.redshank.control_generation(),
        })
    }

    #[cfg(test)]
    pub(crate) fn redshank_note_scope(&self, item: &ItemId) -> Option<NoteScope> {
        self.redshank_control_scope(item)
            .filter(|scope| scope.custody_epoch.is_some() && scope.selected.as_ref() == Some(item))
    }

    pub(crate) fn redshank_note_projection(&self, item: &ItemId) -> NoteProjection {
        use redshank_model::{NoteBody, NotePrivacy};
        use redshank_surfaces::{NoteSummary, NoteSummaryBody, VoiceNotePreview};
        let scope = self.redshank_control_scope(item);
        let note_available = scope.as_ref().is_some_and(|scope| {
            scope.custody_epoch.is_some() && scope.selected.as_ref() == Some(item)
        });
        let mut compact = self.redshank.project(item);
        let capture = note_available
            .then(|| {
                Some((
                    self.redshank.pending_capture_generation(item)?,
                    self.redshank.pending_anchor(item)?.clone(),
                ))
            })
            .flatten();
        compact.text_capture_available = Some(note_available && capture.is_none());
        let notes = if note_available {
            self.redshank
                .model()
                .annotations_for_item(item)
                .into_iter()
                .map(|note| NoteSummary {
                    id: note.id.clone(),
                    item_id: note.target.item_id.clone(),
                    offset_ms: note.target.offset_ms,
                    end_offset_ms: note.target.end_offset_ms,
                    body: match &note.body {
                        NoteBody::Text { plain_text } => NoteSummaryBody::Text(plain_text.clone()),
                        NoteBody::Audio { duration_ms, .. } => NoteSummaryBody::Voice {
                            duration_ms: *duration_ms,
                            preview: VoiceNotePreview::Idle,
                        },
                    },
                    private: note.privacy == NotePrivacy::Private,
                })
                .collect()
        } else {
            Vec::new()
        };
        NoteProjection {
            compact,
            scope,
            capture,
            notes,
        }
    }

    /// Reject a queued action unless its rendered owner and control context still match.
    pub(crate) fn apply_redshank_note_command(
        &mut self,
        item: &ItemId,
        issued: NoteCommand,
    ) -> Vec<Effect> {
        let write = matches!(
            &issued.command,
            CompactCommand::AddTextNote
                | CompactCommand::BeginTextNote
                | CompactCommand::SaveTextNote { .. }
                | CompactCommand::CancelTextNote
                | CompactCommand::SetNotePrivacy { .. }
                | CompactCommand::SetRate(_)
                | CompactCommand::SetVolume(_)
                | CompactCommand::Replay
        );
        let forbidden = matches!(
            &issued.command,
            CompactCommand::BeginEditNote(_)
                | CompactCommand::EditNote { .. }
                | CompactCommand::DeleteNote(_)
                | CompactCommand::SaveSpanNote { .. }
        );
        if forbidden {
            return Vec::new();
        }
        if !write
            && !matches!(
                &issued.command,
                CompactCommand::Play
                    | CompactCommand::Pause
                    | CompactCommand::Seek(_)
                    | CompactCommand::SkipBackward(_)
                    | CompactCommand::SkipForward(_)
                    | CompactCommand::OpenNote(_)
                    | CompactCommand::OpenNoteApproximately(_)
                    | CompactCommand::OpenAlignedNote(_)
                    | CompactCommand::PlaySpan(_)
                    | CompactCommand::OpenVoiceNote(_)
                    | CompactCommand::PlayVoiceNote(_)
                    | CompactCommand::StopVoiceNote(_)
            )
        {
            return Vec::new();
        }
        let Some(scope) = issued.scope.as_ref() else {
            return Vec::new();
        };
        if self.redshank_control_scope(item).as_ref() != Some(scope) {
            return Vec::new();
        }
        if write && scope.custody_epoch.is_none() {
            return Vec::new();
        }
        let note_action = matches!(
            &issued.command,
            CompactCommand::AddTextNote
                | CompactCommand::BeginTextNote
                | CompactCommand::SaveTextNote { .. }
                | CompactCommand::CancelTextNote
                | CompactCommand::SetNotePrivacy { .. }
        );
        if note_action && scope.selected.as_ref() != Some(item) {
            return Vec::new();
        }
        let selected_transport = matches!(
            &issued.command,
            CompactCommand::Pause
                | CompactCommand::Replay
                | CompactCommand::Seek(_)
                | CompactCommand::SkipBackward(_)
                | CompactCommand::SkipForward(_)
                | CompactCommand::OpenNote(_)
                | CompactCommand::OpenNoteApproximately(_)
                | CompactCommand::OpenAlignedNote(_)
                | CompactCommand::PlaySpan(_)
                | CompactCommand::OpenVoiceNote(_)
                | CompactCommand::PlayVoiceNote(_)
                | CompactCommand::StopVoiceNote(_)
        );
        if selected_transport && scope.selected.as_ref() != Some(item) {
            return Vec::new();
        }
        if write {
            match &issued.command {
                CompactCommand::SaveTextNote { .. } | CompactCommand::CancelTextNote => {
                    if issued.capture_generation.is_none()
                        || issued.capture_generation
                            != self.redshank.pending_capture_generation(item)
                    {
                        return Vec::new();
                    }
                },
                CompactCommand::BeginTextNote | CompactCommand::AddTextNote => {
                    if issued.capture_generation.is_some()
                        || self.redshank.pending_capture_generation(item).is_some()
                    {
                        return Vec::new();
                    }
                },
                _ => {},
            }
        }
        if let CompactCommand::OpenNote(id)
        | CompactCommand::OpenNoteApproximately(id)
        | CompactCommand::OpenAlignedNote(id)
        | CompactCommand::PlaySpan(id)
        | CompactCommand::OpenVoiceNote(id)
        | CompactCommand::PlayVoiceNote(id)
        | CompactCommand::StopVoiceNote(id) = &issued.command
        {
            if scope.custody_epoch.is_none()
                || !self
                    .redshank
                    .annotation(id)
                    .is_some_and(|note| &note.target.item_id == item)
            {
                return Vec::new();
            }
        }
        self.apply_redshank_commands(vec![(item.clone(), issued.command)])
    }

    pub(crate) fn invalidate_redshank_capture(&mut self, item: &ItemId, old: &NoteProjection) {
        if old
            .scope
            .as_ref()
            .is_some_and(|scope| scope.session == self.session_id)
            && old.capture.as_ref().is_some_and(|(generation, _)| {
                Some(*generation) == self.redshank.pending_capture_generation(item)
            })
        {
            let _ = self.redshank.command(
                item,
                CompactCommand::CancelTextNote,
                self.now_ms.unwrap_or_else(crate::denizen::now_ms),
            );
        }
    }
}

#[cfg(test)]
#[path = "redshank_notes_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "redshank_transport_tests.rs"]
mod transport_tests;
