// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl RedshankHost {
    pub(crate) fn retained_voice_preview(&self) -> Option<redshank_playback::PreviewSnapshot> {
        self.snapshot().preview
    }

    /// Play a host-resolved retained voice body on the existing output runtime.
    /// This uses no microphone and leaves the recording target/model unchanged.
    pub fn play_retained_voice_bytes(&mut self, id: &str, bytes: Vec<u8>) -> Result<(), String> {
        if bytes.is_empty() || bytes.len() > redshank_playback::MAX_HOST_BLOB_BYTES {
            return Err("Retained voice-note audio exceeds its budget".into());
        }
        let id = format!("clip:{id}");
        *self
            .retained_voice
            .lock()
            .map_err(|_| "Retained voice buffer unavailable")? = Some((id.clone(), bytes));
        if !self.send(PlaybackCommand::StartPreview {
            id: id.clone(),
            source: MediaSource::HostBlob { id },
        }) {
            self.retained_voice
                .lock()
                .map_err(|_| "Retained voice buffer unavailable")?
                .take();
            return Err("No audio output in this session".into());
        }
        Ok(())
    }

    pub fn stop_retained_voice(&mut self, id: &str) -> Result<(), String> {
        let expected = format!("clip:{id}");
        {
            let mut pending = self
                .retained_voice
                .lock()
                .map_err(|_| "Retained voice buffer unavailable")?;
            if pending.as_ref().is_some_and(|(id, _)| id == &expected) {
                pending.take();
            }
        }
        if self
            .snapshot()
            .preview
            .as_ref()
            .is_some_and(|p| p.id == expected)
        {
            if !self.send(PlaybackCommand::StopPreview) {
                return Err("The audio runtime is unavailable".into());
            }
        }
        Ok(())
    }

    /// Open the model under `<session_dir>/redshank/`, creating nothing until
    /// the first save. A store that cannot be read leaves an empty model
    /// rather than refusing to run. `fetch` is the shell's handle; playback
    /// streams through it and nothing else.
    pub fn open(session_dir: &Path, output: Output, fetch: Arc<dyn Fetch>) -> Self {
        Self::open_with(session_dir, output, Some(fetch))
    }

    /// Configure playback without opening a session store or runtime. Refused
    /// sessions keep this configuration so the next successful reopen recovers.
    pub(crate) fn detached(output: Output, fetch: Arc<dyn Fetch>) -> Self {
        Self {
            output,
            fetch: Some(fetch),
            ..Self::default()
        }
    }

    pub(crate) fn suspended(&self) -> Self {
        Self {
            output: self.output,
            playback_wake: self.playback_wake.clone(),
            fetch: self.fetch.clone(),
            ..Self::default()
        }
    }

    /// Reopen under another session directory with this host's output and
    /// handle. A host that never had a handle reopens its model and notes but
    /// starts no runtime, rather than minting a handle of its own.
    pub fn reopen(&self, session_dir: &Path) -> Self {
        let mut host = Self::open_with(session_dir, self.output, self.fetch.clone());
        host.playback_wake = self.playback_wake.clone();
        host
    }

    pub(super) fn open_with(
        session_dir: &Path,
        output: Output,
        fetch: Option<Arc<dyn Fetch>>,
    ) -> Self {
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
        let retained_voice: Arc<std::sync::Mutex<Option<(String, Vec<u8>)>>> = Arc::default();
        Self {
            model,
            store: Some(store),
            fetch,
            output,
            retained_voice,
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

    /// Identity of the current control context, including selection and captures.
    pub(crate) fn control_generation(&self) -> uuid::Uuid {
        self.control_generation
    }

    pub(super) fn advance_control_generation(&mut self) {
        self.control_generation = uuid::Uuid::new_v4();
    }

    pub fn runtime(&self) -> Option<&PlaybackRuntime> {
        self.runtime.get()
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

    pub(super) fn select(&mut self, id: &ItemId) {
        if let Some(command) = self.load_selection(id, None) {
            self.send(command);
        }
    }

    pub(super) fn load_selection(
        &mut self,
        id: &ItemId,
        resume_override: Option<u64>,
    ) -> Option<PlaybackCommand> {
        let Some(source) = self.model.library.get(id).map(|item| item.source().clone()) else {
            return None;
        };
        self.advance_control_generation();
        if self.selected.as_ref() != Some(id) {
            self.anchors.clear();
            self.capture_generations.clear();
        }
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

    pub(super) fn send(&self, command: PlaybackCommand) -> bool {
        let runtime = if matches!(
            command,
            PlaybackCommand::Load { .. } | PlaybackCommand::StartPreview { .. }
        ) {
            self.ensure_runtime()
        } else {
            self.runtime()
        };
        let Some(runtime) = runtime else {
            return false;
        };
        if let Err(error) = runtime.command(command) {
            tracing::warn!(%error, "the Redshank playback runtime refused a command");
            return false;
        }
        true
    }

    pub(super) fn snapshot(&self) -> PlaybackSnapshot {
        self.runtime
            .get()
            .map(PlaybackRuntime::snapshot)
            .unwrap_or_default()
    }

    pub(super) fn send_transport(&mut self, command: PlaybackCommand) -> bool {
        let sent = self.send(command);
        if sent && self.pending_note_seek.is_none() {
            self.hold_progress = false;
        }
        sent
    }

    pub(super) fn acknowledge_note_seek(&mut self, snapshot: &PlaybackSnapshot) {
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

    pub(super) fn matches(&self, snapshot: &PlaybackSnapshot) -> bool {
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

    pub(super) fn record_snapshot_progress(
        &mut self,
        snapshot: &PlaybackSnapshot,
        now_ms: u64,
    ) -> bool {
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
    pub(super) fn anchor(&self, item: &ItemId) -> CaptureAnchor {
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

    pub(super) fn representation(&self, item: &ItemId) -> RepresentationReceipt {
        self.model
            .library
            .get(item)
            .and_then(|item| item.source().cached_representation().cloned())
            .unwrap_or_default()
    }

    pub(super) fn next_note_id(&mut self) -> AnnotationId {
        let id = AnnotationId(format!("note-{}", self.next_note));
        self.next_note += 1;
        id
    }

    /// The anchor a `BeginTextNote` froze, if the host is still holding one.
    pub fn pending_anchor(&self, item: &ItemId) -> Option<&CaptureAnchor> {
        self.anchors.get(item)
    }

    /// Identity of the currently frozen capture, distinct after every begin.
    pub fn pending_capture_generation(&self, item: &ItemId) -> Option<uuid::Uuid> {
        self.capture_generations.get(item).copied()
    }

    pub fn annotation(&self, id: &AnnotationId) -> Option<&Annotation> {
        self.model.annotations.get(id)
    }
}
