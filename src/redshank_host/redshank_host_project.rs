// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl RedshankHost {
    /// Project the runtime snapshot and the model into the dock's state.
    ///
    /// Mirrors the standalone session's `project` for the compact fields only:
    /// the tabs, roster, and scenes are Turnstone's to supply.
    pub fn project(&self, item: &ItemId) -> CompactPlayerState {
        let snapshot = self.snapshot();
        let matched = self.matches(&snapshot) && self.selected.as_ref() == Some(item);
        let library = self.model.library.get(item);
        let completed = self
            .model
            .progress
            .get(item)
            .is_some_and(|progress| progress.completed);
        let transport = match library {
            None => TransportState::Empty,
            Some(_) if self.output == Output::Silent || self.fetch.is_none() => {
                TransportState::Unavailable("No audio output in this session".into())
            },
            Some(_) if !cfg!(feature = "audio") => {
                TransportState::Unavailable("Audio is not included in this build".into())
            },
            Some(_) if !matched => TransportState::Buffering,
            Some(_) => match &snapshot.state {
                PlaybackState::Empty | PlaybackState::Loading => TransportState::Buffering,
                PlaybackState::Playing => TransportState::Playing,
                PlaybackState::Ended => TransportState::Completed,
                PlaybackState::Paused if completed => TransportState::Completed,
                PlaybackState::Paused => TransportState::Paused,
                PlaybackState::Unavailable(error) => TransportState::Unavailable(error.clone()),
            },
        };
        let markers = self
            .model
            .annotations_for_item(item)
            .into_iter()
            .map(|note| NoteMarker {
                id: note.id.clone(),
                offset_ms: note.target.offset_ms,
                end_offset_ms: note.target.end_offset_ms,
                kind: match note.body {
                    NoteBody::Text { .. } => NoteKind::Text,
                    NoteBody::Audio { .. } => NoteKind::Voice,
                },
            })
            .collect();
        // Built by assignment rather than functional update: the command
        // queue is the surface crate's own private field.
        let mut state = CompactPlayerState::default();
        state.note_warning = self
            .note_warning
            .as_ref()
            .filter(|warning| {
                self.model
                    .annotations
                    .get(&warning.id)
                    .is_some_and(|note| &note.target.item_id == item)
            })
            .cloned();
        state.transport = transport;
        state.skip_backward_ms = self.model.settings.skip_backward_ms;
        state.skip_forward_ms = self.model.settings.skip_forward_ms;
        state.rate_percent = if matched {
            snapshot.rate_percent
        } else {
            self.model.settings.playback_rate_percent
        };
        state.volume_percent = if matched {
            snapshot.volume_percent
        } else {
            self.model.settings.volume_percent
        };
        // No microphone adapter in Turnstone this pass, so the dock disables
        // Voice honestly rather than offering a dead control.
        state.voice_capture_available = false;
        state.now_playing = library.map(|library| NowPlaying {
            item_id: item.clone(),
            title: library.title().to_owned(),
            feed_title: feed_title(library),
            face: face(library),
            source: SourceKind::from_item(library),
            position_ms: if matched {
                snapshot.position_ms
            } else {
                self.model
                    .progress
                    .get(item)
                    .map_or(0, |progress| progress.position_ms)
            },
            duration_ms: if matched { snapshot.duration_ms } else { None },
            resumed_from_ms: if self.selected.as_ref() == Some(item) {
                self.resumed_from_ms
            } else {
                None
            },
            buffered_percent: if matched {
                snapshot.buffered_percent
            } else {
                0
            },
            markers,
        });
        state
    }
}
