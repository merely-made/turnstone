// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl RedshankHost {
    /// Apply one command from an episode tile's dock.
    ///
    /// Voice capture is deliberately absent this pass: Turnstone has no
    /// microphone adapter, the projection keeps `voice_capture_available`
    /// false, and a voice command that arrives anyway is refused in words.
    pub fn command(&mut self, item: &ItemId, command: CompactCommand, now_ms: u64) -> HostOutcome {
        if matches!(
            &command,
            CompactCommand::Play
                | CompactCommand::Pause
                | CompactCommand::Replay
                | CompactCommand::Seek(_)
                | CompactCommand::SkipBackward(_)
                | CompactCommand::SkipForward(_)
        ) {
            self.pending_note_open = None;
            // A new explicit transport action supersedes an unacknowledged
            // note seek. Delivery failure still leaves progress held.
            self.pending_note_seek = None;
            self.span_stop_ms = None;
            self.note_warning = None;
        }
        if matches!(
            &command,
            CompactCommand::BeginTextNote
                | CompactCommand::AddTextNote
                | CompactCommand::DeleteNote(_)
                | CompactCommand::BeginEditNote(_)
                | CompactCommand::EditNote { .. }
        ) {
            self.pending_note_open = None;
        }
        match command {
            CompactCommand::SelectItem(id) | CompactCommand::RetryItem(id) => {
                self.select(&id);
                HostOutcome::default()
            },
            CompactCommand::Play => {
                if self.selected.as_ref() != Some(item) {
                    self.select(item);
                }
                if self.snapshot().state == PlaybackState::Ended {
                    self.send_transport(PlaybackCommand::Seek(0));
                }
                self.send_transport(PlaybackCommand::Play);
                HostOutcome::default()
            },
            CompactCommand::Pause => {
                self.send_transport(PlaybackCommand::Pause);
                HostOutcome::default()
            },
            CompactCommand::Replay => {
                if !self.send_transport(PlaybackCommand::Seek(0)) {
                    return HostOutcome::notice(
                        "The audio runtime could not replay this recording",
                    );
                }
                self.send_transport(PlaybackCommand::Play);
                let _ = self.model.set_progress(
                    item,
                    Progress {
                        position_ms: 0,
                        completed: false,
                        updated_at_ms: now_ms,
                    },
                );
                HostOutcome::changed()
            },
            CompactCommand::Seek(position) => {
                self.send_transport(PlaybackCommand::Seek(position));
                HostOutcome::default()
            },
            CompactCommand::SkipBackward(ms) => {
                let position = self.snapshot().position_ms.saturating_sub(ms);
                self.send_transport(PlaybackCommand::Seek(position));
                HostOutcome::default()
            },
            CompactCommand::SkipForward(ms) => {
                let position = self.snapshot().position_ms.saturating_add(ms);
                self.send_transport(PlaybackCommand::Seek(position));
                HostOutcome::default()
            },
            CompactCommand::SetRate(percent) => {
                self.model.settings.playback_rate_percent = percent;
                self.send(PlaybackCommand::SetRate(percent));
                HostOutcome::changed()
            },
            CompactCommand::SetVolume(percent) => {
                self.model.settings.volume_percent = percent;
                self.send(PlaybackCommand::SetVolume(percent));
                HostOutcome::changed()
            },
            CompactCommand::BeginTextNote | CompactCommand::AddTextNote => {
                let anchor = self.anchor(item);
                self.anchors.insert(item.clone(), anchor);
                HostOutcome::default()
            },
            CompactCommand::CancelTextNote => {
                self.anchors.remove(item);
                HostOutcome::default()
            },
            CompactCommand::SaveTextNote { anchor, plain_text } => {
                self.save_text_note(item, anchor, plain_text, now_ms)
            },
            CompactCommand::DeleteNote(id) => match self.model.delete_annotation(&id) {
                Ok(()) => HostOutcome::changed(),
                Err(error) => HostOutcome::notice(format!("Could not delete that note: {error:?}")),
            },
            CompactCommand::EditNote { id, plain_text } => {
                match self.model.update_text_annotation(&id, plain_text) {
                    Ok(()) => HostOutcome::changed(),
                    Err(error) => {
                        HostOutcome::notice(format!("Could not edit that note: {error:?}"))
                    },
                }
            },
            CompactCommand::SetNotePrivacy { id, privacy } => {
                let Some(note) = self.model.annotations.get(&id) else {
                    return HostOutcome::notice("That note is no longer available");
                };
                if note.target.item_id != *item {
                    return HostOutcome::notice("That note belongs to another episode");
                }
                if note.privacy == privacy {
                    return HostOutcome::default();
                }
                match self.model.set_annotation_privacy(&id, privacy) {
                    Ok(()) => HostOutcome::changed(),
                    Err(error) => {
                        HostOutcome::notice(format!("Could not change note privacy: {error:?}"))
                    },
                }
            },
            CompactCommand::OpenNote(id) => self.open_note(id, false, false, false),
            CompactCommand::PlaySpan(id) => self.open_note(id, false, false, true),
            CompactCommand::OpenNoteApproximately(id) => self.open_note(id, true, false, false),
            CompactCommand::OpenAlignedNote(id) => self.open_note(id, false, true, false),
            CompactCommand::RealignNote(_) => HostOutcome::notice(
                "Realign downloaded notes in Redshank; this tile can open a saved aligned estimate",
            ),
            CompactCommand::BeginVoiceNote
            | CompactCommand::FinishVoiceNote
            | CompactCommand::CancelVoiceNote
            | CompactCommand::OpenVoiceNote(_)
            | CompactCommand::PlayVoiceNote(_)
            | CompactCommand::StopVoiceNote(_) => {
                HostOutcome::notice("Voice notes need a microphone Turnstone does not supply yet")
            },
            other => {
                tracing::debug!(
                    ?other,
                    "a dock command outside the episode tile's vocabulary"
                );
                HostOutcome::default()
            },
        }
    }

    pub(super) fn open_note(
        &mut self,
        id: AnnotationId,
        approximate: bool,
        aligned: bool,
        span: bool,
    ) -> HostOutcome {
        if self.ensure_runtime().is_none() {
            return HostOutcome::notice("No audio output in this session");
        }
        let snapshot = self.snapshot();
        match self.note_commands(id.clone(), approximate, aligned, span, &snapshot) {
            Ok(commands) => {
                if self.send_note_commands(&id, commands) {
                    HostOutcome::default()
                } else {
                    HostOutcome::notice("The audio runtime could not open this note")
                }
            },
            Err(message) => {
                self.note_warning = Some(NoteOpenWarning {
                    id,
                    message: message.clone(),
                });
                HostOutcome::notice(message)
            },
        }
    }

    pub(super) fn send_note_commands(
        &mut self,
        id: &AnnotationId,
        commands: Vec<PlaybackCommand>,
    ) -> bool {
        for command in commands {
            if !self.send(command) {
                self.note_warning = Some(NoteOpenWarning {
                    id: id.clone(),
                    message: "The audio runtime could not open this note".into(),
                });
                return false;
            }
        }
        true
    }

    pub(super) fn note_commands(
        &mut self,
        id: AnnotationId,
        approximate: bool,
        aligned: bool,
        span: bool,
        snapshot: &PlaybackSnapshot,
    ) -> Result<Vec<PlaybackCommand>, String> {
        // Even a missing replacement note cancels the previously deferred
        // action. A late Ready must not open the superseded note.
        self.pending_note_open = None;
        self.span_stop_ms = None;
        let target = self
            .model
            .annotations
            .get(&id)
            .ok_or("That note no longer exists")?
            .target
            .item_id
            .clone();
        let mut commands = Vec::new();
        if self.selected.as_ref() != Some(&target) {
            // A receipt must precede any saved-position or annotation seek.
            commands.push(
                self.load_selection(&target, Some(0))
                    .ok_or("The note's recording is no longer in the library")?,
            );
            self.hold_progress = true;
        }
        let request = PendingNoteOpen {
            token: self.token,
            id,
            approximate,
            aligned,
            span,
        };
        if !self.matches(snapshot)
            || matches!(
                snapshot.state,
                PlaybackState::Empty | PlaybackState::Loading
            )
        {
            self.pending_note_open = Some(request);
            self.hold_progress = true;
        } else {
            commands.extend(self.finish_note_open(request, snapshot)?);
        }
        Ok(commands)
    }

    pub(super) fn aligned_offset(
        &self,
        id: &AnnotationId,
        snapshot: &PlaybackSnapshot,
    ) -> Result<u64, String> {
        let derived = self
            .model
            .derived_positions
            .get(id)
            .ok_or("Realign this note first")?;
        self.model
            .validate_derived_position(id, derived)
            .map_err(|error| format!("Saved alignment evidence is invalid: {error:?}"))?;
        let actual = snapshot
            .representation
            .as_ref()
            .and_then(|receipt| receipt.complete_digest.as_ref());
        if derived.destination.complete_digest.is_none()
            || actual != derived.destination.complete_digest.as_ref()
        {
            return Err(
                "The loaded copy has not verified the alignment's destination digest".into(),
            );
        }
        Ok(derived.offset_ms)
    }

    pub(super) fn finish_note_open(
        &mut self,
        request: PendingNoteOpen,
        snapshot: &PlaybackSnapshot,
    ) -> Result<Vec<PlaybackCommand>, String> {
        let note = self
            .model
            .annotations
            .get(&request.id)
            .ok_or("That note no longer exists")?;
        if request.token != self.token
            || !self.matches(snapshot)
            || self.selected.as_ref() != Some(&note.target.item_id)
            || !matches!(
                snapshot.state,
                PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Ended
            )
        {
            return Err("Wait until the note's recording has loaded".into());
        }
        let identity = snapshot
            .representation
            .as_ref()
            .map_or(RepresentationIdentity::Unproven, |receipt| {
                note.target.representation.compare(receipt)
            });
        let aligned_offset = if request.aligned {
            Some(self.aligned_offset(&request.id, snapshot)?)
        } else {
            None
        };
        if !request.approximate && aligned_offset.is_none() {
            match identity {
                RepresentationIdentity::Same => {},
                RepresentationIdentity::Different => return Err("This copy differs.".into()),
                RepresentationIdentity::Unproven => return Err("Couldn't verify this copy.".into()),
            }
        }
        let position_ms = aligned_offset.unwrap_or(note.target.offset_ms);
        let stop = if request.span {
            Some(
                note.target
                    .end_offset_ms
                    .ok_or("That note covers a moment, not a span")?,
            )
        } else {
            None
        };
        self.next_note_seek = self
            .next_note_seek
            .checked_add(1)
            .ok_or("Note seek identity exhausted")?;
        self.pending_note_seek = Some(self.next_note_seek);
        self.hold_progress = true;
        self.span_stop_ms = stop;
        self.note_warning = None;
        Ok(vec![
            PlaybackCommand::SeekNote {
                token: request.token,
                request_id: self.next_note_seek,
                position_ms,
            },
            PlaybackCommand::Play,
        ])
    }

    pub(super) fn save_text_note(
        &mut self,
        item: &ItemId,
        anchor: CaptureAnchor,
        plain_text: String,
        now_ms: u64,
    ) -> HostOutcome {
        if plain_text.trim().is_empty() {
            return HostOutcome::notice("Write a note before saving");
        }
        // The host froze the target when the note began; that is the truth.
        // A dock that supplies its own anchor is honoured only when it names
        // this item, and a note with no frozen target anchors where the
        // listener stands now.
        let frozen = self.anchors.remove(item);
        let anchor = match frozen {
            Some(frozen) => frozen,
            None if anchor.item_id == *item => anchor,
            None => self.anchor(item),
        };
        let id = self.next_note_id();
        match self
            .model
            .add_text_annotation(id.clone(), anchor, plain_text, now_ms)
        {
            Ok(()) => HostOutcome {
                model_changed: true,
                added_note: Some(id),
                notice: None,
            },
            Err(error) => HostOutcome::notice(format!("Could not add that note: {error:?}")),
        }
    }
}
