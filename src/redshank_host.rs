// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The one Redshank authority in this application.
//!
//! Turnstone is Redshank's second host (the listening and annotation port
//! plan, Phase 7). The app owns exactly one listening model, one durable
//! store, and one audio runtime; every episode tile shares them. The tile
//! itself is Redshank's own compact dock, admitted through the
//! contributed-surface seam — there is no Turnstone copy of it.
//!
//! What this file does NOT own: the graph. Projecting an item, its progress,
//! and its notes into Turnstone's graph is `app::redshank_arms`, because the
//! graph belongs to `App`.
//!
//! One honest gap, recorded because the port plan asks for it: the runtime
//! streams HTTP(S) ranges itself (ureq + rustls inside `redshank-playback`)
//! rather than through Turnstone's `mere-fetch` authority. See
//! `design_docs/2026-09-14_redshank_episode_surface_plan.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use redshank_model::{
    Annotation, AnnotationId, CaptureAnchor, ItemId, LibraryItem, MediaSource, NoteBody, Progress,
    RedshankModel, RepresentationReceipt,
};
use redshank_playback::{PlaybackCommand, PlaybackRuntime, PlaybackSnapshot, PlaybackState};
use redshank_storage::{JsonDirectoryStore, ModelStore};
use redshank_surfaces::{
    CompactCommand, CompactPlayerState, Face, NoteKind, NoteMarker, NowPlaying, SourceKind,
    TransportState,
};

/// Where the listening model lives inside a Turnstone session directory.
pub const MODEL_DIR: &str = "redshank";

/// Whether this host may open the machine's audio device.
///
/// `Silent` keeps the model, the notes, and the graph projection working with
/// no runtime thread and no device: what a headless test or an audio-free
/// profile wants. The dock then reports the transport as unavailable rather
/// than pretending to play.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Output {
    Device,
    #[default]
    Silent,
}

/// What applying one dock command changed, so the caller can persist and
/// re-project only when something moved.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HostOutcome {
    /// The listening model changed and wants saving.
    pub model_changed: bool,
    /// A note the graph has not seen yet.
    pub added_note: Option<AnnotationId>,
    /// A refusal to show the listener; the dock keeps its own state.
    pub notice: Option<String>,
}

impl HostOutcome {
    fn changed() -> Self {
        Self {
            model_changed: true,
            ..Self::default()
        }
    }

    fn notice(message: impl Into<String>) -> Self {
        Self {
            notice: Some(message.into()),
            ..Self::default()
        }
    }
}

/// Redshank's model, store, and output authority for one Turnstone session.
pub struct RedshankHost {
    model: RedshankModel,
    store: Option<JsonDirectoryStore>,
    runtime: Option<PlaybackRuntime>,
    output: Output,
    /// The playback identity of the current selection. A snapshot belongs to
    /// the selection only when its load token matches.
    token: u64,
    selected: Option<ItemId>,
    /// Saved progress the selection resumed from, for the dock's mark.
    resumed_from_ms: Option<u64>,
    /// A frozen capture target per item, taken when a note begins.
    anchors: BTreeMap<ItemId, CaptureAnchor>,
    next_note: u64,
}

impl Default for RedshankHost {
    fn default() -> Self {
        Self {
            model: RedshankModel::default(),
            store: None,
            runtime: None,
            output: Output::Silent,
            token: 0,
            selected: None,
            resumed_from_ms: None,
            anchors: BTreeMap::new(),
            next_note: 0,
        }
    }
}

impl RedshankHost {
    /// Open the model under `<session_dir>/redshank/`, creating nothing until
    /// the first save. A store that cannot be read leaves an empty model
    /// rather than refusing to run.
    pub fn open(session_dir: &Path, output: Output) -> Self {
        let root = Self::model_root(session_dir);
        let store = JsonDirectoryStore::new(root);
        let model = match store.load() {
            Ok(Some(model)) => model,
            Ok(None) => RedshankModel::default(),
            Err(error) => {
                tracing::warn!(?error, "the Redshank model could not be read; starting empty");
                RedshankModel::default()
            },
        };
        let next_note = model
            .annotations
            .keys()
            .filter_map(|id| id.0.strip_prefix("note-"))
            .filter_map(|tail| tail.parse::<u64>().ok())
            .max()
            .map_or(0, |highest| highest + 1);
        Self {
            model,
            store: Some(store),
            runtime: (output == Output::Device).then(PlaybackRuntime::start),
            output,
            next_note,
            ..Self::default()
        }
    }

    pub fn model_root(session_dir: &Path) -> PathBuf {
        session_dir.join(MODEL_DIR)
    }

    pub fn model(&self) -> &RedshankModel {
        &self.model
    }

    pub fn selected(&self) -> Option<&ItemId> {
        self.selected.as_ref()
    }

    pub fn runtime(&self) -> Option<&PlaybackRuntime> {
        self.runtime.as_ref()
    }

    pub fn output(&self) -> Output {
        self.output
    }

    /// Write the model out. Silent on success; a store failure is a warning,
    /// not a refusal, because the listener's next action must still work.
    pub fn save(&self) {
        let Some(store) = &self.store else {
            return;
        };
        if let Err(error) = store.save(&self.model) {
            tracing::warn!(?error, "the Redshank model could not be saved");
        }
    }

    /// Ensure `item` exists in the library, then select and load it.
    ///
    /// Idempotent: reopening the same episode keeps its progress and notes and
    /// does not mint a second library item.
    pub fn open_item(&mut self, item: LibraryItem) -> HostOutcome {
        let id = item.id().clone();
        let known = self.model.library.contains_key(&id);
        if !known && let Err(error) = self.model.add_item(item) {
            return HostOutcome::notice(format!("Could not open that episode: {error:?}"));
        }
        self.select(&id);
        HostOutcome {
            model_changed: !known,
            ..HostOutcome::default()
        }
    }

    fn select(&mut self, id: &ItemId) {
        let Some(source) = self.model.library.get(id).map(|item| item.source().clone()) else {
            return;
        };
        self.token = self.token.wrapping_add(1);
        self.selected = Some(id.clone());
        self.model.selected_item = Some(id.clone());
        let resume = self
            .model
            .progress
            .get(id)
            .map_or(0, |progress| progress.position_ms);
        self.resumed_from_ms = (resume > 0).then_some(resume);
        self.send(PlaybackCommand::Load {
            token: self.token,
            source,
            resume_ms: resume,
        });
    }

    fn send(&self, command: PlaybackCommand) {
        let Some(runtime) = &self.runtime else {
            return;
        };
        if let Err(error) = runtime.command(command) {
            tracing::warn!(%error, "the Redshank playback runtime refused a command");
        }
    }

    fn snapshot(&self) -> PlaybackSnapshot {
        self.runtime
            .as_ref()
            .map(PlaybackRuntime::snapshot)
            .unwrap_or_default()
    }

    fn matches(&self, snapshot: &PlaybackSnapshot) -> bool {
        self.selected.is_some() && snapshot.load_token == Some(self.token)
    }

    /// Record where playback reached. Called each frame by the host loop.
    pub fn record_progress(&mut self, now_ms: u64) -> bool {
        let snapshot = self.snapshot();
        if !self.matches(&snapshot)
            || !matches!(
                snapshot.state,
                PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Ended
            )
        {
            return false;
        }
        let completed = snapshot.state == PlaybackState::Ended;
        let id = self.selected.clone().expect("a matched selection");
        if self
            .model
            .progress
            .get(&id)
            .is_some_and(|saved| saved.position_ms == snapshot.position_ms && saved.completed == completed)
        {
            return false;
        }
        self.model.progress.insert(
            id,
            Progress {
                position_ms: snapshot.position_ms,
                completed,
                updated_at_ms: now_ms,
            },
        );
        true
    }

    /// The capture target for a note on `item`, frozen at the current offset.
    ///
    /// A silent host still anchors: a listener reviewing an episode without
    /// audio may annotate the position the model remembers.
    fn anchor(&self, item: &ItemId) -> CaptureAnchor {
        let snapshot = self.snapshot();
        let live = self.matches(&snapshot) && self.selected.as_ref() == Some(item);
        let pressed = if live {
            snapshot.position_ms
        } else {
            self.model
                .progress
                .get(item)
                .map_or(0, |progress| progress.position_ms)
        };
        let offset = pressed.saturating_sub(self.model.settings.reaction_offset_ms);
        CaptureAnchor {
            item_id: item.clone(),
            offset_ms: offset,
            end_offset_ms: None,
            pressed_offset_ms: (offset != pressed).then_some(pressed),
            representation: if live {
                snapshot.representation.clone().unwrap_or_default()
            } else {
                self.representation(item)
            },
        }
    }

    fn representation(&self, item: &ItemId) -> RepresentationReceipt {
        self.model
            .library
            .get(item)
            .and_then(|item| item.source().cached_representation().cloned())
            .unwrap_or_default()
    }

    fn next_note_id(&mut self) -> AnnotationId {
        let id = AnnotationId(format!("note-{}", self.next_note));
        self.next_note += 1;
        id
    }

    /// The anchor a `BeginTextNote` froze, if the host is still holding one.
    pub fn pending_anchor(&self, item: &ItemId) -> Option<&CaptureAnchor> {
        self.anchors.get(item)
    }

    pub fn annotation(&self, id: &AnnotationId) -> Option<&Annotation> {
        self.model.annotations.get(id)
    }

    /// Apply one command from an episode tile's dock.
    ///
    /// Voice capture is deliberately absent this pass: Turnstone has no
    /// microphone adapter, the projection keeps `voice_capture_available`
    /// false, and a voice command that arrives anyway is refused in words.
    pub fn command(&mut self, item: &ItemId, command: CompactCommand, now_ms: u64) -> HostOutcome {
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
                    self.send(PlaybackCommand::Seek(0));
                }
                self.send(PlaybackCommand::Play);
                HostOutcome::default()
            },
            CompactCommand::Pause => {
                self.send(PlaybackCommand::Pause);
                HostOutcome::default()
            },
            CompactCommand::Replay => {
                self.send(PlaybackCommand::Seek(0));
                self.send(PlaybackCommand::Play);
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
                self.send(PlaybackCommand::Seek(position));
                HostOutcome::default()
            },
            CompactCommand::SkipBackward(ms) => {
                let position = self.snapshot().position_ms.saturating_sub(ms);
                self.send(PlaybackCommand::Seek(position));
                HostOutcome::default()
            },
            CompactCommand::SkipForward(ms) => {
                let position = self.snapshot().position_ms.saturating_add(ms);
                self.send(PlaybackCommand::Seek(position));
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
            CompactCommand::OpenNote(id) | CompactCommand::PlaySpan(id) => {
                let Some(note) = self.model.annotations.get(&id) else {
                    return HostOutcome::notice("That note no longer exists");
                };
                let offset = note.target.offset_ms;
                let target = note.target.item_id.clone();
                if self.selected.as_ref() != Some(&target) {
                    self.select(&target);
                }
                self.send(PlaybackCommand::Seek(offset));
                self.send(PlaybackCommand::Play);
                HostOutcome::default()
            },
            CompactCommand::BeginVoiceNote
            | CompactCommand::FinishVoiceNote
            | CompactCommand::CancelVoiceNote
            | CompactCommand::OpenVoiceNote(_)
            | CompactCommand::PlayVoiceNote(_)
            | CompactCommand::StopVoiceNote(_) => {
                HostOutcome::notice("Voice notes need a microphone Turnstone does not supply yet")
            },
            other => {
                tracing::debug!(?other, "a dock command outside the episode tile's vocabulary");
                HostOutcome::default()
            },
        }
    }

    fn save_text_note(
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
            Some(_) if self.output == Output::Silent => {
                TransportState::Unavailable("No audio output in this session".into())
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
            buffered_percent: if matched { snapshot.buffered_percent } else { 0 },
            markers,
        });
        state
    }
}

/// The show a feed episode belongs to, as far as a projection payload knows:
/// the feed's host. The full show title lives in Turnstone's own feed store,
/// not in the pane source.
fn feed_title(item: &LibraryItem) -> Option<String> {
    let LibraryItem::FeedEpisode { feed_url, .. } = item else {
        return None;
    };
    url::Url::parse(feed_url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_owned))
}

fn face(item: &LibraryItem) -> Face {
    match item {
        LibraryItem::FeedEpisode { facts, .. } => match facts.artwork.clone() {
            Some(artwork) => Face::Artwork(artwork),
            None => Face::Tag(format_tag(item)),
        },
        _ => Face::Tag(format_tag(item)),
    }
}

/// The short format tag the dock's face shows when there is no artwork.
fn format_tag(item: &LibraryItem) -> String {
    let address = match item.source() {
        MediaSource::Local { path } => path.clone(),
        MediaSource::Enclosure { url } => url.clone(),
        MediaSource::Cached { origin_url, .. } => origin_url.clone(),
        MediaSource::HostBlob { id } => id.clone(),
    };
    address
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .filter(|extension| extension.len() <= 4 && extension.chars().all(char::is_alphanumeric))
        .unwrap_or_else(|| "audio".to_owned())
}
