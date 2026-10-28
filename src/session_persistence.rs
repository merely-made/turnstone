// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Host-owned session write authority, independent of graph contents.
use std::io;
use std::path::{Path, PathBuf};
use std::fs::{File, OpenOptions};
use std::sync::Arc;
use mere::kernel::graph::Graph;

/// Process-shared capability for writing one session directory.
#[derive(Clone, Debug)]
pub(crate) struct SessionWriteLease(Arc<File>);

impl SessionWriteLease {
    pub(crate) fn acquire(directory: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(directory)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join(".turnstone-session-writer.lock"))?;
        file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => io::Error::new(
                io::ErrorKind::WouldBlock,
                "another process owns this session writer",
            ),
            std::fs::TryLockError::Error(error) => error,
        })?;
        Ok(Self(Arc::new(file)))
    }
}

/// Missing targets with replacement evidence are interrupted saves, not fresh
/// sessions. Preserve those bytes until an explicit recovery resolves them.
pub(crate) fn require_complete_file(path: &Path) -> io::Result<bool> {
    if path.try_exists()? {
        return Ok(true);
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    for suffix in ["previous", "tmp"] {
        let evidence = path.with_extension(format!("{extension}.{suffix}"));
        if evidence.try_exists()? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "incomplete replacement at {}; preserved {}",
                    path.display(),
                    evidence.display()
                ),
            ));
        }
    }
    // Current Pandect creates unique sibling temporaries. A missing target
    // beside any same-target temporary remains interrupted-save evidence,
    // regardless of the writer's private process/counter naming scheme.
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        let prefix = format!("{}.", name.to_string_lossy());
        match std::fs::read_dir(parent) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name.starts_with(&prefix) && name.ends_with(".tmp") {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "incomplete replacement at {}; preserved {}",
                                path.display(),
                                entry.path().display()
                            ),
                        ));
                    }
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {},
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

#[derive(Clone, Debug)]
pub(crate) enum SessionPersistence<P> {
    Unbound,
    Writable {
        directory: PathBuf,
        placement: Option<P>,
        _lease: Option<SessionWriteLease>,
    },
    Refused {
        directory: PathBuf,
        reason: String,
    },
}

impl<P: Copy> SessionPersistence<P> {
    pub(crate) fn writable(directory: PathBuf, placement: Option<P>) -> io::Result<Self> {
        let lease = SessionWriteLease::acquire(&directory)?;
        Ok(Self::writable_with_lease(directory, placement, lease))
    }

    pub(crate) fn writable_with_lease(directory: PathBuf, placement: Option<P>, lease: SessionWriteLease) -> Self {
        Self::Writable {
            directory,
            placement,
            _lease: Some(lease),
        }
    }

    #[cfg(test)]
    fn test_writable(directory: PathBuf, placement: Option<P>) -> Self {
        Self::Writable { directory, placement, _lease: None }
    }

    pub(crate) fn refused(directory: PathBuf, reason: String) -> Self {
        Self::Refused { directory, reason }
    }

    pub(crate) fn owns_directory(&self, directory: &Path) -> bool {
        matches!(self, Self::Writable { directory: owner, _lease: Some(_), .. } if owner == directory)
    }

    pub(crate) fn placement_for(&self, directory: &Path) -> io::Result<Option<P>> {
        match self {
            Self::Writable {
                directory: owner,
                placement,
                _lease: _,
            } if owner == directory => Ok(*placement),
            Self::Writable { .. } => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "graph runtime belongs to a different session directory",
            )),
            Self::Unbound => Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "graph runtime has no session persistence authority",
            )),
            Self::Refused { directory, reason } => Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "session persistence refused at {}: {reason}",
                    directory.display()
                ),
            )),
        }
    }

    /// A graph replacement can revoke qualification, never a load refusal.
    pub(crate) fn clear_placement(&mut self) {
        if let Self::Writable { placement, .. } = self {
            *placement = None;
        }
    }

    /// The facet sidecar holds migration evidence. Do not replace the graph
    /// if those facets could not be persisted, and propagate every error.
    fn save_with_unchecked(
        &self,
        directory: &Path,
        save_facets: impl FnOnce() -> io::Result<()>,
        save_graph: impl FnOnce(Option<P>) -> io::Result<()>,
    ) -> io::Result<()> {
        let placement = self.placement_for(directory)?;
        save_facets()?;
        save_graph(placement)
    }

    #[cfg(test)]
    fn save_with(
        &self,
        directory: &Path,
        save_facets: impl FnOnce() -> io::Result<()>,
        save_graph: impl FnOnce(Option<P>) -> io::Result<()>,
    ) -> io::Result<()> {
        self.save_with_unchecked(directory, save_facets, save_graph)
    }

    pub(crate) fn save_graph_with(
        &self,
        directory: &Path,
        candidate: &Graph,
        save_facets: impl FnOnce() -> io::Result<()>,
        save_graph: impl FnOnce(Option<P>) -> io::Result<()>,
    ) -> io::Result<()> {
        self.placement_for(directory)?;
        if !matches!(self, Self::Writable { _lease: Some(_), .. }) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "session writer lease is absent"));
        }
        let graph_path = directory.join(pandect::session_graph_store::GRAPH_FILE);
        if require_complete_file(&graph_path)? {
            let input = crate::session_graph_file::load_profiled_snapshot(&graph_path)
                .map_err(io::Error::other)?
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "session graph disappeared"))?;
            let mut before = pandect::graph_placement::materialize_snapshot(&input.snapshot, input.placement)
                .map_err(io::Error::other)?;
            let facets = pandect::load_node_facets(directory).map_err(io::Error::other)?.unwrap_or_default();
            before.overlay_facets(facets);
            athanor::workshop_destination::preserve_graph_operations(&before, candidate)
                .map_err(io::Error::other)?;
        }
        self.save_with_unchecked(directory, save_facets, save_graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    #[test]
    fn session_writer_lease_is_exclusive_and_clone_scoped() {
        let root = tempfile::tempdir().unwrap();
        let lease = SessionWriteLease::acquire(root.path()).unwrap();
        assert!(SessionWriteLease::acquire(root.path()).is_err());
        let shared = lease.clone();
        drop(lease);
        assert!(SessionWriteLease::acquire(root.path()).is_err());
        drop(shared);
        assert!(SessionWriteLease::acquire(root.path()).is_ok());
    }

    #[test]
    fn ordinary_save_keeps_optional_placement_and_writes_facets_first() {
        for profile in [Some(7u8), None] {
            let state = SessionPersistence::test_writable(PathBuf::from("session-a"), profile);
            let steps = RefCell::new(Vec::new());
            state
                .save_with(
                    Path::new("session-a"),
                    || {
                        steps.borrow_mut().push("facets");
                        Ok(())
                    },
                    |placement| {
                        assert_eq!(placement, profile);
                        steps.borrow_mut().push("graph");
                        Ok(())
                    },
                )
                .unwrap();
            assert_eq!(*steps.borrow(), ["facets", "graph"]);
        }
    }

    #[test]
    fn refused_unbound_and_wrong_destination_write_nothing() {
        for state in [
            SessionPersistence::refused(PathBuf::from("session-a"), "invalid input".into()),
            SessionPersistence::Unbound,
            SessionPersistence::test_writable(PathBuf::from("session-b"), Some(7u8)),
        ] {
            assert!(
                state
                    .save_with(
                        Path::new("session-a"),
                        || panic!("facet write"),
                        |_| panic!("graph write")
                    )
                    .is_err()
            );
        }
    }

    #[test]
    fn failed_facets_prevent_graph_write_and_graph_failure_is_returned() {
        let state = SessionPersistence::test_writable(PathBuf::from("session-a"), Some(7u8));
        let err = state
            .save_with(
                Path::new("session-a"),
                || Err(std::io::Error::other("facet failure")),
                |_| panic!("graph write"),
            )
            .unwrap_err();
        assert_eq!(err.to_string(), "facet failure");
        let err = state
            .save_with(
                Path::new("session-a"),
                || Ok(()),
                |_| Err(std::io::Error::other("graph failure")),
            )
            .unwrap_err();
        assert_eq!(err.to_string(), "graph failure");
    }

    #[test]
    fn graph_replacement_clears_qualification_but_cannot_clear_refusal() {
        let mut state = SessionPersistence::test_writable(PathBuf::from("session-a"), Some(7u8));
        state.clear_placement();
        state
            .save_with(
                Path::new("session-a"),
                || Ok(()),
                |profile| {
                    assert_eq!(profile, None);
                    Ok(())
                },
            )
            .unwrap();
        state = SessionPersistence::refused(PathBuf::from("session-a"), "bad".into());
        state.clear_placement();
        assert!(
            state
                .save_with(
                    Path::new("session-a"),
                    || panic!("facets"),
                    |_| panic!("graph")
                )
                .is_err()
        );
        // Successful explicit reload replaces the refusal, rather than a
        // display-graph edit or focus change doing so accidentally.
        state = SessionPersistence::test_writable(PathBuf::from("session-a"), None);
        state
            .save_with(Path::new("session-a"), || Ok(()), |_| Ok(()))
            .unwrap();
    }
    #[test]
    fn facet_write_failure_preserves_actual_graph_bytes_and_evidence_retry() {
        let root = std::env::temp_dir().join(format!(
            "turnstone-persistence-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let graph = root.join("graph.json");
        let facet_tmp = root.join("facets.json.tmp");
        std::fs::write(&graph, b"original graph with old labels").unwrap();
        std::fs::create_dir(&facet_tmp).unwrap();
        let state = SessionPersistence::test_writable(root.clone(), Some(7u8));
        assert!(
            state
                .save_with(
                    &root,
                    || std::fs::write(&facet_tmp, b"migration evidence"),
                    |_| std::fs::write(&graph, b"stripped graph")
                )
                .is_err()
        );
        assert_eq!(
            std::fs::read(&graph).unwrap(),
            b"original graph with old labels"
        );
        std::fs::remove_dir(&facet_tmp).unwrap();
        state
            .save_with(
                &root,
                || std::fs::write(&facet_tmp, b"migration evidence"),
                |_| std::fs::write(&graph, b"stripped graph"),
            )
            .unwrap();
        assert_eq!(std::fs::read(&facet_tmp).unwrap(), b"migration evidence");
        assert_eq!(std::fs::read(&graph).unwrap(), b"stripped graph");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn interrupted_replacement_is_not_a_fresh_writable_session() {
        let root = std::env::temp_dir().join(format!(
            "turnstone-replacement-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("graph.json");
        assert!(!require_complete_file(&target).unwrap());
        for suffix in ["previous", "tmp"] {
            let evidence = target.with_extension(format!("json.{suffix}"));
            std::fs::write(&evidence, b"retained graph").unwrap();
            assert!(require_complete_file(&target).is_err());
            assert_eq!(std::fs::read(&evidence).unwrap(), b"retained graph");
            std::fs::remove_file(evidence).unwrap();
        }
        std::fs::write(&target, b"successfully installed graph").unwrap();
        assert!(require_complete_file(&target).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// A real destination-replacement fault, independent of the writer's private
/// temporary filename. Used only with test-owned directories. The old file is
/// held beside the obstruction, checked unchanged, and restored before retry.
#[cfg(test)]
pub(crate) struct BlockedReplacement {
    target: PathBuf,
    held: PathBuf,
    backup: PathBuf,
    had_original: bool,
    active: bool,
}

#[cfg(test)]
impl BlockedReplacement {
    pub(crate) fn new(target: &Path) -> Self {
        let held = target.with_extension("fixture-held");
        let backup = target.with_extension(format!(
            "{}.previous",
            target.extension().unwrap().to_string_lossy()
        ));
        assert!(
            !held.exists() && !backup.exists(),
            "fault fixture must own fresh paths"
        );
        let had_original = target.exists();
        if had_original {
            std::fs::rename(target, &held).unwrap();
        }
        std::fs::create_dir(target).unwrap();
        // A platform using the documented backup-rename fallback must also
        // refuse this non-file destination rather than bypass the obstruction.
        std::fs::create_dir(&backup).unwrap();
        Self {
            target: target.into(),
            held,
            backup,
            had_original,
            active: true,
        }
    }
    pub(crate) fn read_preserved(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(if path == self.target && self.had_original {
            &self.held
        } else {
            path
        })
    }
    pub(crate) fn restore(mut self) {
        self.reset().unwrap();
    }
    fn reset(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        if self.target.is_dir() {
            std::fs::remove_dir(&self.target)?;
        } else if self.target.exists() {
            std::fs::remove_file(&self.target)?;
        }
        std::fs::remove_dir(&self.backup)?;
        if self.had_original {
            std::fs::rename(&self.held, &self.target)?;
        }
        self.active = false;
        Ok(())
    }
}
#[cfg(test)]
impl Drop for BlockedReplacement {
    fn drop(&mut self) {
        let _ = self.reset();
    }
}
