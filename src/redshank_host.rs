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
//! The runtime streams HTTP(S) ranges through the fetch handle the shell
//! hands in, built over Turnstone's session stores, so an episode's requests
//! carry the same cookies as the pages (ranged fetch plan, lane T1). The
//! host holds that handle so a session reopen keeps it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fetch::Fetch;

use redshank_model::{
    Annotation, AnnotationId, CaptureAnchor, ItemId, LibraryItem, MediaSource, NoteBody, Progress,
    RedshankModel, RepresentationIdentity, RepresentationReceipt,
};
use redshank_playback::{PlaybackCommand, PlaybackRuntime, PlaybackSnapshot, PlaybackState};
use redshank_storage::{JsonDirectoryStore, ModelStore};
use redshank_surfaces::{
    CompactCommand, CompactPlayerState, Face, NoteKind, NoteMarker, NoteOpenWarning, NowPlaying,
    SourceKind, TransportState,
};

/// Where the listening model lives inside a Turnstone session directory.
pub const MODEL_DIR: &str = "redshank";

#[derive(Clone)]
struct PendingNoteOpen {
    token: u64,
    id: AnnotationId,
    approximate: bool,
    aligned: bool,
    span: bool,
}

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
    /// The shell's fetch handle, kept so a reopened session starts its runtime
    /// over the same stores. None only for a host that never had one.
    fetch: Option<Arc<dyn Fetch>>,
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
    pending_note_open: Option<PendingNoteOpen>,
    pending_note_seek: Option<u64>,
    next_note_seek: u64,
    hold_progress: bool,
    span_stop_ms: Option<u64>,
    note_warning: Option<NoteOpenWarning>,
}

impl Default for RedshankHost {
    fn default() -> Self {
        Self {
            model: RedshankModel::default(),
            store: None,
            runtime: None,
            fetch: None,
            output: Output::Silent,
            token: 0,
            selected: None,
            resumed_from_ms: None,
            anchors: BTreeMap::new(),
            next_note: 0,
            pending_note_open: None,
            pending_note_seek: None,
            next_note_seek: 0,
            hold_progress: false,
            span_stop_ms: None,
            note_warning: None,
        }
    }
}

impl RedshankHost {
    /// Open the model under `<session_dir>/redshank/`, creating nothing until
    /// the first save. A store that cannot be read leaves an empty model
    /// rather than refusing to run. `fetch` is the shell's handle; playback
    /// streams through it and nothing else.
    pub fn open(session_dir: &Path, output: Output, fetch: Arc<dyn Fetch>) -> Self {
        Self::open_with(session_dir, output, Some(fetch))
    }

    /// Reopen under another session directory with this host's output and
    /// handle. A host that never had a handle reopens its model and notes but
    /// starts no runtime, rather than minting a handle of its own.
    pub fn reopen(&self, session_dir: &Path) -> Self {
        Self::open_with(session_dir, self.output, self.fetch.clone())
    }

    fn open_with(session_dir: &Path, output: Output, fetch: Option<Arc<dyn Fetch>>) -> Self {
        let root = Self::model_root(session_dir);
        let store = JsonDirectoryStore::new(root);
        let model = match store.load() {
            Ok(Some(model)) => model,
            Ok(None) => RedshankModel::default(),
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "the Redshank model could not be read; starting empty"
                );
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
            runtime: match (&fetch, output) {
                (Some(fetch), Output::Device) => Some(PlaybackRuntime::start_with(fetch.clone())),
                _ => None,
            },
            fetch,
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

    /// The fetch handle this host streams through, for the session that
    /// replaces it.
    pub fn fetch(&self) -> Option<Arc<dyn Fetch>> {
        self.fetch.clone()
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
        if let Some(command) = self.load_selection(id, None) {
            self.send(command);
        }
    }

    fn load_selection(
        &mut self,
        id: &ItemId,
        resume_override: Option<u64>,
    ) -> Option<PlaybackCommand> {
        let Some(source) = self.model.library.get(id).map(|item| item.source().clone()) else {
            return None;
        };
        self.pending_note_open = None;
        self.pending_note_seek = None;
        self.span_stop_ms = None;
        self.note_warning = None;
        self.hold_progress = false;
        self.token = self.token.wrapping_add(1);
        self.selected = Some(id.clone());
        self.model.selected_item = Some(id.clone());
        let resume = resume_override.unwrap_or_else(|| {
            self.model
                .progress
                .get(id)
                .map_or(0, |progress| progress.position_ms)
        });
        self.resumed_from_ms = (resume > 0).then_some(resume);
        Some(PlaybackCommand::Load {
            token: self.token,
            source,
            resume_ms: resume,
        })
    }

    fn send(&self, command: PlaybackCommand) -> bool {
        let Some(runtime) = &self.runtime else {
            return false;
        };
        if let Err(error) = runtime.command(command) {
            tracing::warn!(%error, "the Redshank playback runtime refused a command");
            return false;
        }
        true
    }

    fn snapshot(&self) -> PlaybackSnapshot {
        self.runtime
            .as_ref()
            .map(PlaybackRuntime::snapshot)
            .unwrap_or_default()
    }

    fn send_transport(&mut self, command: PlaybackCommand) -> bool {
        let sent = self.send(command);
        if sent && self.pending_note_seek.is_none() {
            self.hold_progress = false;
        }
        sent
    }

    fn acknowledge_note_seek(&mut self, snapshot: &PlaybackSnapshot) {
        if self.matches(snapshot)
            && matches!(
                snapshot.state,
                PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Ended
            )
            && self
                .pending_note_seek
                .is_some_and(|request| snapshot.completed_note_seek == Some(request))
        {
            self.pending_note_seek = None;
            self.hold_progress = false;
        }
    }

    fn matches(&self, snapshot: &PlaybackSnapshot) -> bool {
        self.selected.is_some() && snapshot.load_token == Some(self.token)
    }

    /// Record where playback reached. Called each frame by the host loop.
    pub fn record_progress(&mut self, now_ms: u64) -> bool {
        let snapshot = self.snapshot();
        self.acknowledge_note_seek(&snapshot);
        if self.matches(&snapshot) {
            if snapshot.state != PlaybackState::Loading
                && let Some(request) = self.pending_note_open.take()
            {
                let id = request.id.clone();
                match self.finish_note_open(request, &snapshot) {
                    Ok(commands) => {
                        self.send_note_commands(&id, commands);
                    },
                    Err(message) => self.note_warning = Some(NoteOpenWarning { id, message }),
                }
            }
            if self.pending_note_seek.is_none()
                && snapshot.state == PlaybackState::Playing
                && self
                    .span_stop_ms
                    .is_some_and(|stop| snapshot.position_ms >= stop)
            {
                self.span_stop_ms = None;
                self.send(PlaybackCommand::Pause);
            }
        }
        self.record_snapshot_progress(&snapshot, now_ms)
    }

    fn record_snapshot_progress(&mut self, snapshot: &PlaybackSnapshot, now_ms: u64) -> bool {
        if self.hold_progress
            || !self.matches(snapshot)
            || !matches!(
                snapshot.state,
                PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Ended
            )
        {
            return false;
        }
        let completed = snapshot.state == PlaybackState::Ended;
        let id = self.selected.clone().expect("a matched selection");
        if self.model.progress.get(&id).is_some_and(|saved| {
            saved.position_ms == snapshot.position_ms && saved.completed == completed
        }) {
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
        let live = self.matches(&snapshot)
            && self.selected.as_ref() == Some(item)
            && !self.hold_progress
            && matches!(
                snapshot.state,
                PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Ended
            );
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
            fingerprint: if live && !self.hold_progress {
                snapshot
                    .fingerprint_context
                    .as_ref()
                    .and_then(|context| context.at(offset, self.model.settings.alignment_window_ms))
            } else {
                None
            },
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

    fn open_note(
        &mut self,
        id: AnnotationId,
        approximate: bool,
        aligned: bool,
        span: bool,
    ) -> HostOutcome {
        if self.runtime.is_none() {
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

    fn send_note_commands(&mut self, id: &AnnotationId, commands: Vec<PlaybackCommand>) -> bool {
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

    fn note_commands(
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

    fn aligned_offset(
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

    fn finish_note_open(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt(byte: char) -> RepresentationReceipt {
        RepresentationReceipt {
            byte_length: Some(1_000),
            complete_digest: Some(format!("blake3:{}", byte.to_string().repeat(64))),
            ..Default::default()
        }
    }

    fn note_host() -> (RedshankHost, PlaybackSnapshot, AnnotationId) {
        let mut host = RedshankHost::default();
        let item = ItemId("episode".into());
        host.model
            .add_item(LibraryItem::DirectAudio {
                id: item.clone(),
                title: "Episode".into(),
                source: MediaSource::Enclosure {
                    url: "https://example.invalid/audio.mp3".into(),
                },
            })
            .unwrap();
        host.selected = Some(item.clone());
        host.token = 7;
        host.model.progress.insert(
            item.clone(),
            Progress {
                position_ms: 8_000,
                completed: false,
                updated_at_ms: 1,
            },
        );
        let id = AnnotationId("note".into());
        host.model
            .add_text_annotation(
                id.clone(),
                CaptureAnchor {
                    item_id: item,
                    offset_ms: 16_000,
                    end_offset_ms: Some(19_000),
                    pressed_offset_ms: None,
                    representation: receipt('a'),
                    fingerprint: Some(redshank_model::AudioFingerprint {
                        version: 1,
                        frame_ms: 100,
                        anchor_offset_ms: 5_000,
                        frames: vec![[1; 16]; 50],
                    }),
                },
                "original note".into(),
                0,
            )
            .unwrap();
        let snapshot = PlaybackSnapshot {
            load_token: Some(7),
            state: PlaybackState::Paused,
            representation: Some(receipt('a')),
            position_ms: 8_000,
            duration_ms: Some(60_000),
            ..Default::default()
        };
        (host, snapshot, id)
    }

    fn request(
        host: &RedshankHost,
        id: &AnnotationId,
        approximate: bool,
        aligned: bool,
        span: bool,
    ) -> PendingNoteOpen {
        PendingNoteOpen {
            token: host.token,
            id: id.clone(),
            approximate,
            aligned,
            span,
        }
    }

    #[test]
    fn note_opens_require_ready_matching_receipts_or_explicit_approximation() {
        let (mut host, snapshot, id) = note_host();
        for representation in [
            None,
            Some(RepresentationReceipt::default()),
            Some(receipt('b')),
        ] {
            let changed = PlaybackSnapshot {
                representation,
                ..snapshot.clone()
            };
            assert!(
                host.finish_note_open(request(&host, &id, false, false, false), &changed)
                    .is_err()
            );
            assert!(host.pending_note_seek.is_none());
            let commands = host
                .finish_note_open(request(&host, &id, true, false, false), &changed)
                .unwrap();
            assert!(matches!(
                &commands[0],
                PlaybackCommand::SeekNote {
                    token: 7,
                    position_ms: 16_000,
                    ..
                }
            ));
            host.pending_note_seek = None;
        }
        for state in [
            PlaybackState::Loading,
            PlaybackState::Unavailable("failed".into()),
        ] {
            let loading = PlaybackSnapshot {
                state,
                ..snapshot.clone()
            };
            assert!(
                host.finish_note_open(request(&host, &id, true, false, false), &loading)
                    .is_err()
            );
        }
        let stale = PlaybackSnapshot {
            load_token: Some(6),
            ..snapshot
        };
        assert!(
            host.finish_note_open(request(&host, &id, true, false, false), &stale)
                .is_err()
        );
    }

    #[test]
    fn another_items_note_loads_paused_at_zero_and_holds_saved_progress() {
        let (mut host, snapshot, id) = note_host();
        host.selected = None;
        let commands = host
            .note_commands(id, false, false, false, &snapshot)
            .unwrap();
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            &commands[0],
            PlaybackCommand::Load {
                token: 8,
                resume_ms: 0,
                ..
            }
        ));
        assert!(host.pending_note_open.is_some());
        let ready = PlaybackSnapshot {
            load_token: Some(8),
            position_ms: 0,
            representation: None,
            ..snapshot
        };
        let pending = host.pending_note_open.take().unwrap();
        assert!(host.finish_note_open(pending, &ready).is_err());
        assert!(!host.record_snapshot_progress(&ready, 9));
        assert_eq!(
            host.model.progress[&ItemId("episode".into())].position_ms,
            8_000
        );
    }

    #[test]
    fn only_the_latest_seek_acknowledgement_releases_saved_progress() {
        let (mut host, mut snapshot, id) = note_host();
        host.finish_note_open(request(&host, &id, false, false, false), &snapshot)
            .unwrap();
        let earlier = host.pending_note_seek.unwrap();
        host.finish_note_open(request(&host, &id, false, false, false), &snapshot)
            .unwrap();
        let latest = host.pending_note_seek.unwrap();
        snapshot.completed_note_seek = Some(earlier);
        host.acknowledge_note_seek(&snapshot);
        assert!(!host.record_snapshot_progress(&snapshot, 10));
        snapshot.completed_note_seek = Some(latest);
        snapshot.load_token = Some(6);
        host.acknowledge_note_seek(&snapshot);
        assert!(host.hold_progress);
        snapshot.load_token = Some(7);
        snapshot.state = PlaybackState::Unavailable("seek failed".into());
        host.acknowledge_note_seek(&snapshot);
        assert!(host.hold_progress);
        snapshot.state = PlaybackState::Paused;
        snapshot.position_ms = 16_000;
        host.acknowledge_note_seek(&snapshot);
        assert!(host.record_snapshot_progress(&snapshot, 11));
        assert_eq!(
            host.model.progress[&ItemId("episode".into())].position_ms,
            16_000
        );
    }

    #[test]
    fn refused_replacement_and_failed_runtime_dispatch_keep_the_progress_hold() {
        let (mut host, snapshot, id) = note_host();
        let commands = host
            .finish_note_open(request(&host, &id, false, false, false), &snapshot)
            .unwrap();
        let accepted = host.pending_note_seek;
        let changed = PlaybackSnapshot {
            representation: Some(receipt('b')),
            ..snapshot.clone()
        };
        assert!(
            host.finish_note_open(request(&host, &id, false, false, false), &changed)
                .is_err()
        );
        assert_eq!(host.pending_note_seek, accepted);
        assert!(
            !host.send_note_commands(&id, commands),
            "silent host has no runtime to accept the seek"
        );
        host.command(&ItemId("episode".into()), CompactCommand::Pause, 12);
        assert!(host.pending_note_seek.is_none());
        assert!(host.hold_progress);
        assert!(!host.record_snapshot_progress(&snapshot, 12));
    }

    #[test]
    fn new_transport_supersedes_an_unacknowledged_note_seek_when_delivery_succeeds() {
        let (mut host, snapshot, id) = note_host();
        host.finish_note_open(request(&host, &id, false, false, false), &snapshot)
            .unwrap();
        assert!(host.pending_note_seek.is_some());
        assert!(host.hold_progress);
        // An empty runtime accepts Pause without opening a decoder or device.
        host.runtime = Some(PlaybackRuntime::start_with(Arc::new(Inert)));
        host.command(&ItemId("episode".into()), CompactCommand::Pause, 13);
        assert!(host.pending_note_seek.is_none());
        assert!(!host.hold_progress);
    }

    #[test]
    fn missing_replacement_note_cancels_the_old_deferred_open_and_span() {
        let (mut host, mut snapshot, id) = note_host();
        snapshot.state = PlaybackState::Loading;
        host.note_commands(id, false, false, true, &snapshot)
            .unwrap();
        assert!(host.pending_note_open.is_some());
        host.span_stop_ms = Some(19_000);
        assert!(
            host.note_commands(
                AnnotationId("deleted-note".into()),
                false,
                false,
                false,
                &snapshot
            )
            .is_err()
        );
        assert!(host.pending_note_open.is_none());
        assert!(host.span_stop_ms.is_none());
        assert!(host.hold_progress);
    }

    #[test]
    fn capturing_cancels_a_pending_open_and_compact_projection_keeps_its_warning() {
        let (mut host, mut snapshot, id) = note_host();
        snapshot.state = PlaybackState::Loading;
        host.note_commands(id.clone(), false, false, false, &snapshot)
            .unwrap();
        assert!(host.pending_note_open.is_some());
        let item = ItemId("episode".into());
        host.command(&item, CompactCommand::BeginTextNote, 1);
        assert!(host.pending_note_open.is_none());
        host.note_warning = Some(NoteOpenWarning {
            id: id.clone(),
            message: "Couldn't verify this copy.".into(),
        });
        assert_eq!(host.project(&item).note_warning.unwrap().id, id);
        assert!(host.project(&ItemId("other".into())).note_warning.is_none());
    }

    #[test]
    fn aligned_points_require_the_loaded_digest_and_never_remap_span_ends() {
        let (mut host, mut snapshot, id) = note_host();
        let original = host.model.annotations[&id].target.clone();
        let derived = redshank_model::DerivedPosition {
            original_target: original.clone(),
            destination: receipt('b'),
            offset_ms: 46_000,
            algorithm_version: 1,
            confidence_per_mille: 990,
            runner_up_per_mille: 500,
            reference_duration_ms: 5_000,
        };
        host.model.store_derived_position(&id, derived).unwrap();
        snapshot.representation = Some(receipt('b'));
        // A saved estimate never changes the meaning of plain Open at.
        assert_eq!(
            host.finish_note_open(request(&host, &id, false, false, false), &snapshot)
                .unwrap_err(),
            "This copy differs."
        );
        assert!(host.pending_note_seek.is_none());
        let commands = host
            .finish_note_open(request(&host, &id, false, true, false), &snapshot)
            .unwrap();
        assert!(matches!(
            &commands[0],
            PlaybackCommand::SeekNote {
                position_ms: 46_000,
                ..
            }
        ));
        assert_eq!(host.model.annotations[&id].target, original);
        assert!(
            host.finish_note_open(request(&host, &id, false, false, true), &snapshot)
                .is_err()
        );
        snapshot.representation = Some(receipt('c'));
        assert!(
            host.finish_note_open(request(&host, &id, false, true, false), &snapshot)
                .is_err()
        );
        snapshot.representation = None;
        assert!(
            host.finish_note_open(request(&host, &id, false, true, false), &snapshot)
                .is_err()
        );
    }

    /// A handle that is never called: what the test needs is its identity.
    struct Inert;

    impl Fetch for Inert {
        fn read_range(
            &self,
            _: &str,
            _: fetch::Range,
            _: Option<&str>,
        ) -> Result<fetch::RangeReply, fetch::FetchError> {
            unreachable!("nothing fetches in this test")
        }
        fn read_all(
            &self,
            _: &str,
            _: Option<&str>,
            _: u64,
        ) -> Result<fetch::Body, fetch::FetchError> {
            unreachable!("nothing fetches in this test")
        }
        fn read_into(
            &self,
            _: &str,
            _: &mut dyn std::io::Write,
            _: Option<u64>,
        ) -> Result<fetch::Facts, fetch::FetchError> {
            unreachable!("nothing fetches in this test")
        }
    }

    #[test]
    fn the_shells_fetch_handle_survives_a_session_reopen() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let handle: Arc<dyn Fetch> = Arc::new(Inert);

        let host = RedshankHost::open(first.path(), Output::Silent, handle.clone());
        assert!(Arc::ptr_eq(&host.fetch().unwrap(), &handle));
        let reopened = host.reopen(second.path());
        assert!(
            Arc::ptr_eq(&reopened.fetch().unwrap(), &handle),
            "a reopened session must stream through the same handle, not a new one"
        );

        // A host that never had a handle reopens its model and starts nothing.
        let bare = RedshankHost::default().reopen(second.path());
        assert!(bare.fetch().is_none());
        assert!(bare.runtime().is_none());
    }
}
