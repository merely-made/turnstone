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
use std::sync::{Arc, OnceLock};

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

/// Redshank's model, store, and output authority for one Turnstone session.
pub struct RedshankHost {
    model: RedshankModel,
    store: Option<JsonDirectoryStore>,
    runtime: OnceLock<PlaybackRuntime>,
    playback_wake: Option<armillary::Wake>,
    retained_voice: Arc<std::sync::Mutex<Option<(String, Vec<u8>)>>>,
    /// The shell's fetch handle, kept so a reopened session starts its runtime
    /// over the same stores. None only for a host that never had one.
    fetch: Option<Arc<dyn Fetch>>,
    output: Output,
    /// The playback identity of the current selection. A snapshot belongs to
    /// the selection only when its load token matches.
    token: u64,
    /// Changes on selection or capture transitions, including round trips.
    control_generation: uuid::Uuid,
    selected: Option<ItemId>,
    /// Saved progress the selection resumed from, for the dock's mark.
    resumed_from_ms: Option<u64>,
    /// A frozen capture target per item, taken when a note begins.
    anchors: BTreeMap<ItemId, CaptureAnchor>,
    /// A fresh identity for every capture, even at the same source offset.
    capture_generations: BTreeMap<ItemId, uuid::Uuid>,
    next_note: u64,
    pending_note_open: Option<PendingNoteOpen>,
    pending_note_seek: Option<u64>,
    next_note_seek: u64,
    hold_progress: bool,
    span_stop_ms: Option<u64>,
    note_warning: Option<NoteOpenWarning>,
}

#[cfg(test)]
#[path = "redshank_host/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "redshank_host/note_capture_tests.rs"]
mod note_capture_tests;

#[path = "redshank_host/feed_title.rs"]
mod feed_title;
#[path = "redshank_host/host_outcome_changed.rs"]
mod host_outcome_changed;
#[path = "redshank_host/impl_default_for_redshank_host.rs"]
mod impl_default_for_redshank_host;
#[path = "redshank_host/redshank_host_command.rs"]
mod redshank_host_command;
#[path = "redshank_host/redshank_host_project.rs"]
mod redshank_host_project;
#[path = "redshank_host/redshank_host_retained_voice_preview.rs"]
mod redshank_host_retained_voice_preview;
use feed_title::feed_title;
#[path = "redshank_host/face.rs"]
mod face;
use face::face;
#[path = "redshank_host/format_tag.rs"]
mod format_tag;
use format_tag::format_tag;

#[cfg(test)]
#[path = "redshank_host/startup_tests.rs"]
mod startup_tests;

#[path = "redshank_host/lazy_playback.rs"]
mod lazy_playback;
