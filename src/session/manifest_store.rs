// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! In-memory + on-disk store for [`GraphSessionManifest`]s.
//!
//! The runtime layer of session persistence per the
//! [GraphSessionManifest implementation plan](../../../../design_docs/mere_docs/implementation_strategy/2026-05-11_graph_session_manifest_plan.md).
//! Owns the durable session identity records; host adapters (gpui or
//! otherwise) layer their runtime graph entities on top.
//!
//! Storage layout:
//!
//! ```text
//! <sessions_dir>/
//! ├── <session_id_1>/
//! │   └── manifest.json
//! ├── <session_id_2>/
//! │   └── manifest.json
//! └── .trash/
//!     └── <session_id_dead>/manifest.json
//! ```
//!
//! Graph-data persistence (the `graph/` directory under each
//! session) is the **next slice** — it depends on the eidetic graph
//! persistence boundary being clean for `kernel::graph::Graph`.
//! Until then, manifests persist independently; graph entities
//! reset on app restart.
//!
//! ## Lifecycle (host-side responsibilities)
//!
//! - App start: call [`ManifestStore::load_from_disk`] against the
//!   configured sessions dir.
//! - Session creation: call [`ManifestStore::insert`] with a fresh
//!   manifest; the store marks it dirty.
//! - Session mutation: call [`ManifestStore::mark_dirty`] when the
//!   manifest's contents have changed.
//! - Periodically (debounced) and on app exit: call
//!   [`ManifestStore::flush_dirty`] to write out the dirty
//!   manifests.
//! - Session kill: call [`ManifestStore::move_to_trash`]; the
//!   manifest is removed from the live map + the directory moves
//!   under `<sessions_dir>/.trash/`.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use pandect::SessionId;

use pandect::manifest::GraphSessionManifest;

/// Filename of the per-session manifest inside its directory.
pub const MANIFEST_FILE: &str = "manifest.json";

/// Subdirectory under the sessions root where killed sessions are
/// moved instead of being deleted outright.
pub const TRASH_DIR: &str = ".trash";

/// In-memory + on-disk store for session manifests.
///
/// Wraps a `HashMap<SessionId, GraphSessionManifest>` plus a dirty
/// set. Mutations mark sessions dirty; [`Self::flush_dirty`] writes
/// them to disk in one pass. Reads (`get`, `iter`, `len`) are
/// pure-memory.
#[derive(Default)]
pub struct ManifestStore {
    manifests: HashMap<SessionId, GraphSessionManifest>,
    dirty: HashSet<SessionId>,
    /// Root directory the store reads from / writes to. Set by
    /// [`Self::with_root`] or [`Self::load_from_disk`].
    root: Option<PathBuf>,
}

/// Outcome of a load-from-disk pass over a sessions directory.
/// Surfaces enough detail for diagnostics receipts per the
/// multiplexer framing brief §8.
#[derive(Debug, Default)]
pub struct LoadReport {
    /// Sessions whose manifest parsed cleanly.
    pub loaded: Vec<SessionId>,
    /// Sessions whose directory existed but whose `manifest.json`
    /// failed to parse. Directory is left in place for recovery.
    pub failed: Vec<LoadFailure>,
}

/// One malformed manifest. The store leaves the on-disk directory
/// untouched so the user can inspect or restore.
#[derive(Debug)]
pub struct LoadFailure {
    pub dir_name: String,
    pub reason: String,
}

impl ManifestStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach the store to a sessions-root directory without
    /// reading anything from it. Useful for tests + the common case
    /// where the host wants the path bound up front so subsequent
    /// `flush_dirty` calls don't need a path argument.
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            manifests: HashMap::new(),
            dirty: HashSet::new(),
            root: Some(root.into()),
        }
    }

    /// The sessions root, if one is bound.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Bind / re-bind the sessions root.
    pub fn set_root(&mut self, root: impl Into<PathBuf>) {
        self.root = Some(root.into());
    }

    /// Look up a manifest by its `SessionId`.
    pub fn get(&self, id: SessionId) -> Option<&GraphSessionManifest> {
        self.manifests.get(&id)
    }

    /// Mutable lookup. After mutation the caller should call
    /// [`Self::mark_dirty`] (or use [`Self::update`] which handles
    /// both).
    pub fn get_mut(&mut self, id: SessionId) -> Option<&mut GraphSessionManifest> {
        self.manifests.get_mut(&id)
    }

    /// Mutate a manifest and mark it dirty in one call. Returns
    /// `true` if the session existed and was updated.
    pub fn update(&mut self, id: SessionId, f: impl FnOnce(&mut GraphSessionManifest)) -> bool {
        let Some(m) = self.manifests.get_mut(&id) else {
            return false;
        };
        f(m);
        m.touch();
        self.dirty.insert(id);
        true
    }

    /// Iterate over every `(SessionId, &Manifest)`.
    pub fn iter(&self) -> impl Iterator<Item = (SessionId, &GraphSessionManifest)> {
        self.manifests.iter().map(|(k, v)| (*k, v))
    }

    pub fn len(&self) -> usize {
        self.manifests.len()
    }

    pub fn is_empty(&self) -> bool {
        self.manifests.is_empty()
    }

    /// Insert a fresh manifest. Marks dirty. Returns the
    /// previously-stored manifest under that id if any (usually
    /// `None`).
    pub fn insert(&mut self, manifest: GraphSessionManifest) -> Option<GraphSessionManifest> {
        let id = manifest.session_id;
        self.dirty.insert(id);
        self.manifests.insert(id, manifest)
    }

    /// Remove a session from the in-memory map. Does **not** touch
    /// disk — use [`Self::move_to_trash`] for the disk side.
    pub fn remove(&mut self, id: SessionId) -> Option<GraphSessionManifest> {
        self.dirty.remove(&id);
        self.manifests.remove(&id)
    }

    /// Mark a session as needing a flush. Used by callers that
    /// mutate the manifest through `get_mut` directly.
    pub fn mark_dirty(&mut self, id: SessionId) {
        if self.manifests.contains_key(&id) {
            self.dirty.insert(id);
        }
    }

    pub fn is_dirty(&self, id: SessionId) -> bool {
        self.dirty.contains(&id)
    }

    pub fn dirty_count(&self) -> usize {
        self.dirty.len()
    }

    /// Walk a sessions directory, deserialise every well-formed
    /// `manifest.json` underneath it, and populate the in-memory
    /// map. The `<root>/.trash/` directory is skipped.
    ///
    /// Returns a [`LoadReport`] enumerating successes and failures
    /// so the caller can emit diagnostics. The function itself
    /// only fails for I/O errors on the root directory traversal;
    /// per-session parse errors are accumulated into the report.
    pub fn load_from_disk(&mut self, root: impl Into<PathBuf>) -> io::Result<LoadReport> {
        let root = root.into();
        self.root = Some(root.clone());
        let mut report = LoadReport::default();
        if !root.exists() {
            // Fresh install: nothing to load. Caller will seed a
            // default session if it wants one.
            return Ok(report);
        }
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if dir_name == TRASH_DIR {
                continue;
            }
            let manifest_path = path.join(MANIFEST_FILE);
            if !manifest_path.exists() {
                report.failed.push(LoadFailure {
                    dir_name: dir_name.to_string(),
                    reason: "manifest.json missing".to_string(),
                });
                continue;
            }
            match fs::read_to_string(&manifest_path)
                .map_err(|e| e.to_string())
                .and_then(|text| {
                    serde_json::from_str::<GraphSessionManifest>(&text).map_err(|e| e.to_string())
                }) {
                Ok(manifest) => {
                    let id = manifest.session_id;
                    self.manifests.insert(id, manifest);
                    report.loaded.push(id);
                },
                Err(reason) => {
                    report.failed.push(LoadFailure {
                        dir_name: dir_name.to_string(),
                        reason,
                    });
                },
            }
        }
        Ok(report)
    }

    /// Write every dirty manifest to its session directory. Clears
    /// the dirty set. Requires a root to be bound (via
    /// [`Self::with_root`], [`Self::set_root`], or
    /// [`Self::load_from_disk`]).
    ///
    /// Returns the count of manifests flushed. Per-manifest write
    /// failures are surfaced as `io::Error` — callers should
    /// decide whether to keep going or bubble up.
    pub fn flush_dirty(&mut self) -> io::Result<usize> {
        let root = self.root.clone().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "ManifestStore::flush_dirty called with no bound root",
            )
        })?;
        let to_flush: Vec<SessionId> = self.dirty.iter().copied().collect();
        let mut written = 0;
        for id in to_flush {
            let Some(manifest) = self.manifests.get(&id) else {
                self.dirty.remove(&id);
                continue;
            };
            let session_dir = root.join(id.as_uuid().to_string());
            fs::create_dir_all(&session_dir)?;
            let manifest_path = session_dir.join(MANIFEST_FILE);
            let json = serde_json::to_string_pretty(manifest)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            // Atomic-ish write: write to a temp file, then rename.
            // On the same FS the rename is atomic; this prevents a
            // half-written manifest if the process dies mid-flush.
            let tmp_path = session_dir.join(format!("{MANIFEST_FILE}.tmp"));
            fs::write(&tmp_path, json)?;
            fs::rename(&tmp_path, &manifest_path)?;
            self.dirty.remove(&id);
            written += 1;
        }
        Ok(written)
    }

    /// Move a session's on-disk directory to `<root>/.trash/`. The
    /// in-memory manifest is removed too. Recoverable manually
    /// from disk until the user clears the trash.
    pub fn move_to_trash(&mut self, id: SessionId) -> io::Result<bool> {
        let root = self.root.clone().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "ManifestStore::move_to_trash called with no bound root",
            )
        })?;
        let removed_in_memory = self.remove(id).is_some();
        let source = root.join(id.as_uuid().to_string());
        if !source.exists() {
            // No on-disk artefacts; in-memory removal stands alone.
            return Ok(removed_in_memory);
        }
        let trash_root = root.join(TRASH_DIR);
        fs::create_dir_all(&trash_root)?;
        let target = trash_root.join(id.as_uuid().to_string());
        // If a trash entry under the same id exists, append a
        // disambiguating timestamp to avoid clobbering.
        let target = if target.exists() {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            trash_root.join(format!("{}-{stamp}", id.as_uuid()))
        } else {
            target
        };
        fs::rename(&source, &target)?;
        Ok(true)
    }

    /// The trashed sessions' manifests, newest trashed-directory first (by
    /// directory modification time — good enough for a "removed" list). Each
    /// trash entry is a whole session directory ([`Self::move_to_trash`]
    /// moves it intact), so its `manifest.json` still describes it; an entry
    /// whose manifest is missing or malformed is skipped, never fatal. Reads
    /// the disk on every call — callers that render per frame should cache
    /// and refresh on trash/restore.
    pub fn list_trash(&self) -> Vec<GraphSessionManifest> {
        let Some(root) = &self.root else {
            return Vec::new();
        };
        let trash_root = root.join(TRASH_DIR);
        let Ok(entries) = fs::read_dir(&trash_root) else {
            return Vec::new();
        };
        let mut found: Vec<(std::time::SystemTime, GraphSessionManifest)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Ok(text) = fs::read_to_string(path.join(MANIFEST_FILE)) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_str::<GraphSessionManifest>(&text) else {
                continue;
            };
            let when = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            found.push((when, manifest));
        }
        found.sort_by(|(a, _), (b, _)| b.cmp(a));
        found.into_iter().map(|(_, m)| m).collect()
    }

    /// Restore a trashed session: move its directory back under the root and
    /// re-insert its manifest into the in-memory map — the recovery half of
    /// [`Self::move_to_trash`], same-identity by construction (the directory
    /// moved intact, `SessionId` and all). Picks the newest trash entry when
    /// disambiguated duplicates exist. `Ok(false)` when no trash entry holds
    /// this id; an error when a LIVE session directory already occupies the
    /// slot (restore must never clobber the living).
    pub fn restore_from_trash(&mut self, id: SessionId) -> io::Result<bool> {
        let root = self.root.clone().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "ManifestStore::restore_from_trash called with no bound root",
            )
        })?;
        let trash_root = root.join(TRASH_DIR);
        let prefix = id.as_uuid().to_string();
        // Exact entry, else the newest `-stamp` disambiguated one.
        let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        if let Ok(entries) = fs::read_dir(&trash_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if name == prefix || name.starts_with(&format!("{prefix}-")) {
                    let when = entry
                        .metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    candidates.push((when, path));
                }
            }
        }
        candidates.sort_by(|(a, _), (b, _)| b.cmp(a));
        let Some((_, source)) = candidates.into_iter().next() else {
            return Ok(false);
        };
        let target = root.join(&prefix);
        if target.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "a live session directory already occupies this id",
            ));
        }
        fs::rename(&source, &target)?;
        let text = fs::read_to_string(target.join(MANIFEST_FILE))?;
        let manifest: GraphSessionManifest = serde_json::from_str(&text)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        self.manifests.insert(manifest.session_id, manifest);
        Ok(true)
    }
}

#[cfg(test)]
#[path = "manifest_store/tests.rs"]
mod tests;
