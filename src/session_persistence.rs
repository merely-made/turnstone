// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Host-owned session write authority, independent of graph contents.
use std::io;
use std::path::{Path, PathBuf};

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
    Ok(false)
}

#[derive(Clone, Debug)]
pub(crate) enum SessionPersistence<P> {
    Unbound,
    Writable {
        directory: PathBuf,
        placement: Option<P>,
    },
    Refused {
        directory: PathBuf,
        reason: String,
    },
}

impl<P: Copy> SessionPersistence<P> {
    pub(crate) fn writable(directory: PathBuf, placement: Option<P>) -> Self {
        Self::Writable {
            directory,
            placement,
        }
    }

    pub(crate) fn refused(directory: PathBuf, reason: String) -> Self {
        Self::Refused { directory, reason }
    }

    pub(crate) fn placement_for(&self, directory: &Path) -> io::Result<Option<P>> {
        match self {
            Self::Writable {
                directory: owner,
                placement,
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
    pub(crate) fn save_with(
        &self,
        directory: &Path,
        save_facets: impl FnOnce() -> io::Result<()>,
        save_graph: impl FnOnce(Option<P>) -> io::Result<()>,
    ) -> io::Result<()> {
        let placement = self.placement_for(directory)?;
        save_facets()?;
        save_graph(placement)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    #[test]
    fn ordinary_save_keeps_optional_placement_and_writes_facets_first() {
        for profile in [Some(7u8), None] {
            let state = SessionPersistence::writable(PathBuf::from("session-a"), profile);
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
            SessionPersistence::writable(PathBuf::from("session-b"), Some(7u8)),
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
        let state = SessionPersistence::writable(PathBuf::from("session-a"), Some(7u8));
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
        let mut state = SessionPersistence::writable(PathBuf::from("session-a"), Some(7u8));
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
        state = SessionPersistence::writable(PathBuf::from("session-a"), None);
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
        let state = SessionPersistence::writable(root.clone(), Some(7u8));
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
