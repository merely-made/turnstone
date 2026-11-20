// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use inker::{
    DocumentSession, EngineDocument, EngineInput, SessionEngine, SessionError, SessionSpawnRequest,
};
use mere_document_lanes::{SmolwebDocument, SmolwebDocumentSession, SmolwebTheme};
use netrender::Scene;
use serde::Deserialize;

#[path = "../ports/smolweb-pack/protocol.rs"]
mod protocol;

pub(crate) const ENGINE_ID: &str = "nematic.gemtext.pack";
const MANIFEST: &str = "pack.json";
const EXECUTABLE: &str = if cfg!(windows) {
    "turnstone-smolweb-pack.exe"
} else {
    "turnstone-smolweb-pack"
};
const PROCESS_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    engine: String,
    scheme: String,
    content_type: String,
}

fn valid_manifest(directory: &Path) -> io::Result<bool> {
    let mut bytes = Vec::new();
    fs::File::open(directory.join(MANIFEST))?
        .take(1025)
        .read_to_end(&mut bytes)?;
    let expected: Manifest = serde_json::from_str(include_str!("../ports/smolweb-pack/pack.json"))?;
    Ok(bytes.len() <= 1024
        && serde_json::from_slice::<Manifest>(&bytes).is_ok_and(|manifest| manifest == expected))
}

fn pack_parent(root: &Path) -> PathBuf {
    root.join("engine-packs")
}

fn installed(root: &Path) -> PathBuf {
    pack_parent(root).join("smolweb")
}

/// Manage the first-party local Gemini pack before opening the native app.
pub fn command(args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let args: Vec<_> = args.collect();
    let root = crate::session::default_turnstone_root();
    let result = match args.as_slice() {
        [verb, directory] if verb == OsStr::new("install") => install(&root, Path::new(directory)),
        [verb, name] if verb == OsStr::new("remove") && name == OsStr::new("smolweb") => {
            remove(&root)
        },
        _ => return Err("Usage: turnstone engine-pack install <bundle> | remove smolweb".into()),
    };
    result.map_err(|error| format!("Smolweb engine pack: {error}"))
}

fn install(root: &Path, bundle: &Path) -> io::Result<()> {
    if !valid_manifest(bundle)? {
        return Err(io::Error::other("unsupported Smolweb pack manifest"));
    }
    let parent = pack_parent(root);
    fs::create_dir_all(&parent)?;
    let staging = tempfile::tempdir_in(parent)?;
    fs::copy(bundle.join(EXECUTABLE), staging.path().join(EXECUTABLE))?;
    fs::write(
        staging.path().join(MANIFEST),
        include_bytes!("../ports/smolweb-pack/pack.json"),
    )?;
    // Publish the two files together. A second install must remove the first.
    fs::rename(staging.path(), installed(root))
}

fn remove(root: &Path) -> io::Result<()> {
    let staging = tempfile::tempdir_in(pack_parent(root))?;
    fs::rename(installed(root), staging.path().join("removed"))?;
    staging.close()
}

#[derive(Clone)]
pub(crate) struct SmolwebPack {
    directory: PathBuf,
}

impl SmolwebPack {
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            directory: installed(root),
        }
    }

    pub(crate) fn available(&self) -> bool {
        valid_manifest(&self.directory).unwrap_or(false)
            && self.directory.join(EXECUTABLE).is_file()
    }

    pub(crate) fn lower(&self, input: &EngineInput) -> Result<EngineDocument, SessionError> {
        if !protocol::accepts(input) {
            return Err(SessionError::SpawnFailed(
                "Smolweb pack requires a Gemini Gemtext response".into(),
            ));
        }
        if !self.available() {
            return Err(SessionError::SpawnFailed(
                "Smolweb engine pack is unavailable. Choose Auto or install the pack.".into(),
            ));
        }
        self.run(input)
            .map_err(|error| SessionError::SpawnFailed(format!("Smolweb engine pack: {error}")))
    }

    fn run(&self, input: &EngineInput) -> io::Result<EngineDocument> {
        let request = serde_json::to_vec(input)?;
        if request.len() as u64 > protocol::REQUEST_LIMIT {
            return Err(io::Error::other("input exceeds its byte limit"));
        }
        // Anonymous files keep the protocol off the command line and avoid
        // blocking in a pipe operation before the child can be timed out.
        let mut stdin = tempfile::tempfile()?;
        std::io::Write::write_all(&mut stdin, &request)?;
        stdin.rewind()?;
        let mut stdout = tempfile::tempfile()?;
        let mut command = Command::new(fs::canonicalize(self.directory.join(EXECUTABLE))?);
        command
            .arg(protocol::COMMAND)
            .env_clear()
            .stdin(stdin)
            .stdout(stdout.try_clone()?)
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn()?;
        let deadline = Instant::now() + PROCESS_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => break,
                Ok(Some(status)) => {
                    return Err(io::Error::other(format!("worker exited with {status}")));
                },
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5));
                },
                result => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(result
                        .err()
                        .unwrap_or_else(|| io::Error::other("worker timed out")));
                },
            }
        }
        stdout.rewind()?;
        let mut reply = Vec::new();
        stdout
            .take(protocol::REPLY_LIMIT + 1)
            .read_to_end(&mut reply)?;
        if reply.len() as u64 > protocol::REPLY_LIMIT {
            return Err(io::Error::other("output exceeds its byte limit"));
        }
        let document: EngineDocument = serde_json::from_slice(&reply)?;
        if document.address != input.address
            || document.provenance.source_kind.as_deref() != Some("nematic.gemtext")
        {
            return Err(io::Error::other(
                "worker returned a document for the wrong source",
            ));
        }
        Ok(document)
    }
}

impl SessionEngine<Scene> for SmolwebPack {
    fn engine_id(&self) -> &str {
        ENGINE_ID
    }

    fn spawn(
        &self,
        request: &SessionSpawnRequest,
    ) -> Result<Box<dyn DocumentSession<Scene>>, SessionError> {
        let body = request.body.as_ref().ok_or_else(|| {
            SessionError::SpawnFailed("Smolweb engine pack requires a host-fetched body".into())
        })?;
        let mut input = EngineInput::new(&request.address, body);
        input.content_type = request.content_type.clone();
        let document =
            SmolwebDocument::from_document_with_theme(self.lower(&input)?, SmolwebTheme::default());
        Ok(Box::new(SmolwebDocumentSession::new(
            document,
            request.viewport,
        )))
    }
}
