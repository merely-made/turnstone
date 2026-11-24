// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Native client for Woodshed's authenticated, local practice handoff.
//!
//! This module passes a canonical recording path over authenticated loopback.
//! It does not fetch a URL. Callers should only construct a
//! [`LocalWavSelection`] from a native file-open action or file picker, never
//! from web content or a serialized browser request.

use std::{
    env,
    ffi::OsStr,
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

mod discover;
mod error;

pub use discover::discover_executables;
use discover::validate_executable;
#[cfg(test)]
use discover::{development_sibling_executable, executable_name};
pub use error::ClientError;

const ENDPOINT_FILE: &str = "turnstone-handoff.json";
const MAX_ENDPOINT_BYTES: u64 = 16 * 1024;
const MAX_FRAME_BYTES: usize = 16 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(350);
const IO_TIMEOUT: Duration = Duration::from_secs(14);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const STARTUP_RETRY: Duration = Duration::from_millis(100);

/// A WAV, MP3 or MP4 selected through Turnstone's native local-file route.
///
/// This type cannot be deserialized from web content. The path is canonicalized
/// at creation and checked again before it is sent to Woodshed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalWavSelection {
    path: PathBuf,
}

impl LocalWavSelection {
    /// Selects a WAV for the direct WAV import API.
    pub fn from_native_file(path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let recording = Self::from_native_recording(path)?;
        if !recording
            .path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
        {
            return Err(ClientError::InvalidWav("Choose a WAV recording.".into()));
        }
        Ok(recording)
    }

    /// The native practice pane pairs a compressed source with its decoded preview.
    pub(crate) fn from_native_recording(path: impl AsRef<Path>) -> Result<Self, ClientError> {
        Ok(Self {
            path: canonical_recording_path(path.as_ref())?,
        })
    }

    /// Opens the native WAV picker for callers of the direct WAV import API.
    pub fn pick_local_wav() -> Result<Option<Self>, ClientError> {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("WAV audio", &["wav"])
            .pick_file()
        else {
            return Ok(None);
        };
        Self::from_native_file(path).map(Some)
    }

    /// The canonical local path, for native UI display and request ownership.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn checked_path(&self) -> Result<&Path, ClientError> {
        let canonical = canonical_recording_path(&self.path)?;
        if canonical != self.path {
            return Err(ClientError::InvalidWav(
                "The selected recording changed location after selection.".into(),
            ));
        }
        Ok(&self.path)
    }
}

/// The metadata and local recording sent to Woodshed's import command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportWav {
    pub wav: LocalWavSelection,
    pub title: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub note: String,
}

/// Audio captured by Turnstone, with marks on the selected page player's clock.
/// The queued request owns the temporary file until Woodshed acknowledges it.
#[cfg(all(windows, feature = "scry"))]
#[derive(Clone)]
pub(crate) struct PagePhrase {
    pub title: String,
    pub source_page_url: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub note: String,
    pub phrase: std::sync::Arc<tempfile::TempPath>,
}

/// A forward-compatible Woodshed command result.
///
/// `status` is left as JSON because Woodshed may add observation fields without
/// requiring a Turnstone update. Known fields include `view`, `pid`,
/// `stream_id`, `streams`, `gain`, `quiet`, `wav_selected`, and `wav_loop`.
#[derive(Clone, Debug, PartialEq)]
pub struct WoodshedResponse {
    pub status: Option<Value>,
}

/// A live, authenticated Woodshed connection.
///
/// Keep this value alive while the Woodshed pane is attached. Calls block while
/// Woodshed processes each command, so run them on a worker thread. The type is
/// `Send` and owns the stream; dropping it releases the pane lease while audio
/// continues playing in Woodshed.
pub struct WoodshedSession {
    stream: BufReader<TcpStream>,
    token: String,
    pid: u32,
}

impl WoodshedSession {
    /// Imports a local recording and starts its playback in Woodshed.
    pub fn import_wav(&mut self, import: &ImportWav) -> Result<WoodshedResponse, ClientError> {
        self.import_recording(import, None)
    }

    pub(crate) fn import_recording(
        &mut self,
        import: &ImportWav,
        preview: Option<&Path>,
    ) -> Result<WoodshedResponse, ClientError> {
        let wav_path = import.wav.checked_path()?;
        validate_phrase(&import.title, import.start_ms, import.end_ms)?;
        let source_page_url = url::Url::from_file_path(wav_path).map_err(|_| {
            ClientError::InvalidWav("Could not form a local file URL for the recording.".into())
        })?;
        self.command(WireCommand::ImportWav {
            source: WireWavSource {
                title: import.title.clone(),
                source_page_url: source_page_url.to_string(),
                start_ms: import.start_ms,
                end_ms: import.end_ms,
                note: import.note.clone(),
            },
            wav_path: wav_path.to_path_buf(),
            preview_path: preview.map(Path::to_path_buf),
        })
    }

    #[cfg(all(windows, feature = "scry"))]
    pub(crate) fn import_page_phrase(
        &mut self,
        phrase: &PagePhrase,
    ) -> Result<WoodshedResponse, ClientError> {
        validate_phrase(&phrase.title, phrase.start_ms, phrase.end_ms)?;
        let url = url::Url::parse(&phrase.source_page_url)
            .ok()
            .filter(|url| {
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
            })
            .ok_or_else(|| {
                ClientError::InvalidRequest("Choose media on an HTTP or HTTPS page.".into())
            })?;
        self.command(WireCommand::ImportPagePhrase {
            source: WireWavSource {
                title: phrase.title.clone(),
                source_page_url: url.into(),
                start_ms: phrase.start_ms,
                end_ms: phrase.end_ms,
                note: phrase.note.clone(),
            },
            phrase_path: phrase.phrase.canonicalize()?,
        })
    }

    /// Reads Woodshed's current practice and native-window status.
    pub fn status(&mut self) -> Result<WoodshedResponse, ClientError> {
        self.command(WireCommand::Status)
    }

    /// Leases Woodshed's window presentation to the Turnstone pane.
    pub fn attach(&mut self) -> Result<WoodshedResponse, ClientError> {
        self.command(WireCommand::Attach)
    }

    /// Restores Woodshed's own window while retaining the connection.
    pub fn detach(&mut self) -> Result<WoodshedResponse, ClientError> {
        self.command(WireCommand::Detach)
    }

    pub fn play(&mut self) -> Result<WoodshedResponse, ClientError> {
        self.command(WireCommand::Play)
    }

    pub fn pause(&mut self) -> Result<WoodshedResponse, ClientError> {
        self.command(WireCommand::Pause)
    }

    /// Sets output gain from 0 to 100 percent.
    pub fn set_gain(&mut self, value: u8) -> Result<WoodshedResponse, ClientError> {
        if value > 100 {
            return Err(ClientError::InvalidRequest(
                "Woodshed gain must be between 0 and 100.".into(),
            ));
        }
        self.command(WireCommand::Gain { value })
    }

    /// Toggles Woodshed's quiet level.
    pub fn quiet(&mut self) -> Result<WoodshedResponse, ClientError> {
        self.command(WireCommand::Quiet)
    }

    /// Releases the pane lease, restores Woodshed's window, and closes the
    /// retained connection. Audio continues in Woodshed.
    pub fn release(&mut self) -> Result<WoodshedResponse, ClientError> {
        let result = self.command(WireCommand::Release);
        let _ = self.stream.get_mut().shutdown(std::net::Shutdown::Both);
        result
    }

    fn command(&mut self, command: WireCommand) -> Result<WoodshedResponse, ClientError> {
        let request = WireRequest {
            token: self.token.clone(),
            command,
        };
        let mut bytes = serde_json::to_vec(&request)?;
        if bytes.len() + 1 > MAX_FRAME_BYTES {
            return Err(ClientError::InvalidRequest(
                "Woodshed handoff request exceeds the protocol size limit.".into(),
            ));
        }
        bytes.push(b'\n');
        self.stream.get_mut().write_all(&bytes)?;
        self.stream.get_mut().flush()?;

        let response: WireResponse = read_frame(&mut self.stream)?;
        if response.token != self.token {
            return Err(ClientError::IdentityChanged);
        }
        if !response.ok {
            return Err(ClientError::Remote {
                message: response
                    .error
                    .unwrap_or_else(|| "Woodshed rejected the command.".into()),
                status: response.status,
            });
        }
        if let Some(status_pid) = response
            .status
            .as_ref()
            .and_then(|status| status.get("pid"))
            .and_then(Value::as_u64)
        {
            if status_pid != u64::from(self.pid) {
                return Err(ClientError::IdentityChanged);
            }
        }
        Ok(WoodshedResponse {
            status: response.status,
        })
    }
}

fn validate_phrase(title: &str, start_ms: u64, end_ms: u64) -> Result<(), ClientError> {
    if title.trim().is_empty() || title.len() > 160 || title.chars().any(char::is_control) {
        return Err(ClientError::InvalidRequest(
            "Use a title of up to 160 UTF-8 bytes.".into(),
        ));
    }
    if start_ms >= end_ms || end_ms > (1_u64 << 53) - 1 {
        return Err(ClientError::InvalidRequest(
            "The end time must be after the start time and within the supported range.".into(),
        ));
    }
    Ok(())
}

/// Connects to the current Woodshed owner, starting one only when needed.
///
/// `executable` is an explicit user selection and overrides automatic
/// discovery. An already-running Woodshed owner is always reused. If PATH or a
/// configured repository exposes several same-priority binaries, the caller
/// must present [`ClientError::ExecutableChoiceRequired`] to the user.
pub fn connect_or_launch(executable: Option<PathBuf>) -> Result<WoodshedSession, ClientError> {
    let root = data_root()?;
    let endpoint_path = root.join(ENDPOINT_FILE);
    let mut last_error = match connect_existing(&endpoint_path) {
        Ok(Some(session)) => return Ok(session),
        Ok(None) => None,
        Err(error) => Some(error),
    };

    let launch_path = if let Some(path) = executable {
        validate_executable(path)?
    } else {
        let candidates = discover_executables();
        match candidates.as_slice() {
            [one] => validate_executable(one.clone())?,
            [] => return Err(ClientError::ExecutableNotFound),
            many => return Err(ClientError::ExecutableChoiceRequired(many.to_vec())),
        }
    };

    Command::new(&launch_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|source| ClientError::Launch {
            path: launch_path,
            source,
        })?;

    let deadline = Instant::now() + STARTUP_TIMEOUT;
    while Instant::now() < deadline {
        match connect_existing(&endpoint_path) {
            Ok(Some(session)) => return Ok(session),
            Ok(None) => {},
            Err(error) => last_error = Some(error),
        }
        thread::sleep(STARTUP_RETRY);
    }
    Err(ClientError::StartupTimeout {
        path: endpoint_path,
        detail: last_error.map(|error| error.to_string()),
    })
}

#[derive(Deserialize)]
struct Rendezvous {
    version: u8,
    pid: u32,
    port: u16,
    token: String,
}

#[derive(Serialize)]
struct WireRequest {
    token: String,
    command: WireCommand,
}

#[derive(Serialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
enum WireCommand {
    ImportWav {
        source: WireWavSource,
        wav_path: PathBuf,
        #[serde(skip_serializing_if = "Option::is_none")]
        preview_path: Option<PathBuf>,
    },
    #[cfg(all(windows, feature = "scry"))]
    ImportPagePhrase {
        source: WireWavSource,
        phrase_path: PathBuf,
    },
    Status,
    Attach,
    Detach,
    Play,
    Pause,
    Gain {
        value: u8,
    },
    Quiet,
    Release,
}

#[derive(Serialize)]
struct WireWavSource {
    title: String,
    source_page_url: String,
    start_ms: u64,
    end_ms: u64,
    note: String,
}

#[derive(Deserialize)]
struct WireResponse {
    token: String,
    ok: bool,
    error: Option<String>,
    status: Option<Value>,
}

fn connect_existing(endpoint_path: &Path) -> Result<Option<WoodshedSession>, ClientError> {
    let endpoint = match read_endpoint(endpoint_path) {
        Ok(endpoint) => endpoint,
        Err(ClientError::Io(error)) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, endpoint.port);
    let stream = match TcpStream::connect_timeout(&address.into(), CONNECT_TIMEOUT) {
        Ok(stream) => stream,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionRefused
                    | io::ErrorKind::TimedOut
                    | io::ErrorKind::AddrNotAvailable
            ) =>
        {
            return Ok(None);
        },
        Err(error) => return Err(error.into()),
    };
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let mut session = WoodshedSession {
        stream: BufReader::new(stream),
        token: endpoint.token,
        pid: endpoint.pid,
    };
    // A successful TCP connect alone does not prove this endpoint belongs to
    // the advertised Woodshed owner. The authenticated status reply checks the
    // token and PID before exposing a session to UI code.
    session.status()?;
    Ok(Some(session))
}

fn read_endpoint(path: &Path) -> Result<Rendezvous, ClientError> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    Read::take(file, MAX_ENDPOINT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_ENDPOINT_BYTES {
        return Err(ClientError::Protocol(
            "Woodshed endpoint file is too large.".into(),
        ));
    }
    let endpoint: Rendezvous = serde_json::from_slice(&bytes)?;
    if endpoint.version != 1 || endpoint.pid == 0 || endpoint.port == 0 || endpoint.token.is_empty()
    {
        return Err(ClientError::Protocol(
            "Woodshed endpoint file is invalid or unsupported.".into(),
        ));
    }
    Ok(endpoint)
}

fn read_frame<T: for<'de> Deserialize<'de>>(reader: &mut impl BufRead) -> Result<T, ClientError> {
    let mut bytes = Vec::new();
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Err(ClientError::Protocol(
                "Woodshed closed without a response.".into(),
            ));
        }
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(buffer.len(), |index| index + 1);
        if bytes.len() + consumed > MAX_FRAME_BYTES {
            return Err(ClientError::Protocol(
                "Woodshed response frame is too large.".into(),
            ));
        }
        bytes.extend_from_slice(&buffer[..consumed]);
        reader.consume(consumed);
        if newline.is_some() {
            break;
        }
    }
    serde_json::from_slice(&bytes).map_err(Into::into)
}

pub(crate) fn is_recording_path(path: &Path) -> bool {
    path.extension().and_then(OsStr::to_str).is_some_and(|ext| {
        ["wav", "mp3", "mp4"]
            .iter()
            .any(|supported| ext.eq_ignore_ascii_case(supported))
    })
}

fn canonical_recording_path(path: &Path) -> Result<PathBuf, ClientError> {
    if !path.is_absolute() {
        return Err(ClientError::InvalidWav(
            "Choose a local recording with an absolute path.".into(),
        ));
    }
    let canonical = fs::canonicalize(path)?;
    let metadata = fs::metadata(&canonical)?;
    if !metadata.is_file() || !is_recording_path(&canonical) {
        return Err(ClientError::InvalidWav(
            "Choose a local WAV, MP3 or MP4 recording.".into(),
        ));
    }
    Ok(canonical)
}

fn data_root() -> Result<PathBuf, ClientError> {
    let root = if let Some(path) = env::var_os("WOODSHED_DATA_DIR") {
        if path.is_empty() {
            return Err(ClientError::Configuration(
                "WOODSHED_DATA_DIR is empty.".into(),
            ));
        }
        PathBuf::from(path)
    } else {
        default_data_root()?
    };
    if root.is_absolute() {
        Ok(root)
    } else {
        Ok(env::current_dir()?.join(root))
    }
}

fn default_data_root() -> Result<PathBuf, ClientError> {
    directories::ProjectDirs::from("dev", "Woodshed", "Woodshed")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .ok_or_else(|| {
            ClientError::Configuration(
                "Set WOODSHED_DATA_DIR to the Woodshed app-data directory.".into(),
            )
        })
}

#[cfg(test)]
mod tests;
