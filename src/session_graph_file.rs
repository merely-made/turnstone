// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's file-backed session graph envelope.
//!
//! Pandect no longer owns a loose `graph.json` writer. Turnstone retains its
//! existing session directory owner and stores the declared placement beside
//! the canonical snapshot in one atomic replacement.

use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use mere::kernel::graph::Graph;
use mere::kernel::persistence::GraphSnapshot;
use pandect::graph_placement::{PlacementProfile, materialize_snapshot};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ProfiledGraphSnapshot {
    #[serde(flatten)]
    pub snapshot: GraphSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<PlacementProfile>,
}

pub(crate) fn save_profiled(
    path: &Path,
    graph: &Graph,
    placement: Option<PlacementProfile>,
) -> io::Result<()> {
    let input = ProfiledGraphSnapshot {
        snapshot: graph.to_snapshot(),
        placement,
    };
    let bytes = serde_json::to_vec_pretty(&input)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    write_bytes_with_backup(path, &bytes)
}

pub(crate) fn save(path: &Path, graph: &Graph) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(&graph.to_snapshot())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    write_bytes_with_backup(path, &bytes)
}

pub(crate) fn load_profiled_snapshot(path: &Path) -> io::Result<Option<ProfiledGraphSnapshot>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub(crate) fn load_snapshot(path: &Path) -> io::Result<Option<GraphSnapshot>> {
    load_profiled_snapshot(path)?
        .map(|input| {
            if input.placement == Some(PlacementProfile::LegacySurfaceV1) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "legacy graph placement requires qualified replay",
                ));
            }
            Ok(input.snapshot)
        })
        .transpose()
}

pub(crate) fn load(path: &Path) -> io::Result<Option<Graph>> {
    load_profiled_snapshot(path)?
        .map(|input| {
            materialize_snapshot(&input.snapshot, input.placement)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
        })
        .transpose()
}

fn write_bytes_with_backup(path: &Path, bytes: &[u8]) -> io::Result<()> {
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "graph path has no file name"))?
        .to_string_lossy();
    let temporary = loop {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            "{file_name}.{}.{}.tmp",
            std::process::id(),
            sequence
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
                    let _ = std::fs::remove_file(&candidate);
                    return Err(error);
                }
                break candidate;
            },
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };

    let backup = path.with_extension("json.previous");
    if path.exists()
        && let Err(error) = std::fs::copy(path, &backup)
    {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}
