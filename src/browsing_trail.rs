// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Explicit local file exchange for a saved session's browsing trail.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::str::FromStr;

use pandect::GraphSessionManifest;
use uuid::Uuid;

use crate::panes::{GraphId, SessionId};
use crate::session::manifest_store::ManifestStore;
use crate::session::{load_manifests, session_dir, sessions_root};

const ORIGIN: &str = "turnstone.rdf-trail-origin";
const MAX_TRAIL_BYTES: u64 = 32 * 1024 * 1024;

pub fn command(args: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let args: Vec<_> = args.into_iter().collect();
    match args.as_slice() {
        [verb, profile, session, file] if verb == "export" => {
            let id = SessionId::from_uuid(
                Uuid::from_str(session.to_str().ok_or("invalid session identifier")?)
                    .map_err(|error| error.to_string())?,
            );
            export(Path::new(profile), id, Path::new(file))
        },
        [verb, file, profile] if verb == "reopen" => {
            let id = reopen(Path::new(file), Path::new(profile))?;
            println!("{}", id.as_uuid());
            Ok(())
        },
        _ => Err("Usage: turnstone browsing-trail export PROFILE SESSION_ID FILE.trig\n       turnstone browsing-trail reopen FILE.trig PROFILE".into()),
    }
}

fn export(profile: &Path, session: SessionId, file: &Path) -> Result<(), String> {
    let manifests = load_manifests(profile);
    let manifest = manifests.get(session).ok_or("unknown session")?;
    let origin = manifest.extension_get(ORIGIN).unwrap_or(*session.as_uuid());
    let directory = session_dir(profile, session);
    let graph =
        crate::session_graph_file::load(&directory.join(pandect::session_graph_store::GRAPH_FILE))
            .map_err(|error| error.to_string())?
            .ok_or("the session has no saved graph")?;
    let rdf = graphshell_rdf_trail::export(&graph, origin)?;
    if rdf.len() as u64 > MAX_TRAIL_BYTES {
        return Err("the browsing trail exceeds the 32 MiB exchange limit".into());
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options.open(file).map_err(|error| error.to_string())?;
    output
        .write_all(rdf.as_bytes())
        .and_then(|()| output.sync_all())
        .map_err(|error| error.to_string())
}

fn reopen(file: &Path, profile: &Path) -> Result<SessionId, String> {
    let mut rdf = String::new();
    File::open(file)
        .map_err(|error| error.to_string())?
        .take(MAX_TRAIL_BYTES + 1)
        .read_to_string(&mut rdf)
        .map_err(|error| error.to_string())?;
    if rdf.len() as u64 > MAX_TRAIL_BYTES {
        return Err("the browsing trail exceeds the 32 MiB exchange limit".into());
    }
    let reopened = graphshell_rdf_trail::reopen(&rdf)?;

    let session = SessionId::new();
    let directory = session_dir(profile, session);
    fs::create_dir_all(sessions_root(profile)).map_err(|error| error.to_string())?;
    fs::create_dir(&directory).map_err(|error| error.to_string())?;

    // The manifest is the catalog admission. Publish it after the whole graph
    // is stored. The original workspace ID is attribution, not a local parent.
    crate::session_graph_file::save(
        &directory.join(pandect::session_graph_store::GRAPH_FILE),
        &reopened.graph,
    )
    .map_err(|error| error.to_string())?;
    let mut manifest = GraphSessionManifest::new(session, GraphId::new());
    manifest.display_name = Some("Reopened browsing trail".into());
    manifest
        .extension
        .insert(ORIGIN.into(), serde_json::json!(reopened.origin));
    let mut manifests = ManifestStore::with_root(sessions_root(profile));
    manifests.insert(manifest);
    manifests.flush_dirty().map_err(|error| error.to_string())?;
    Ok(session)
}

#[cfg(test)]
mod tests;
