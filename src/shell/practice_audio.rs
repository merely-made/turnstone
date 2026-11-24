// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Phrase marking and Woodshed transport for recordings opened by the native host.

mod compressed;
mod connection;
#[cfg(any(all(windows, feature = "scry"), test))]
pub(crate) mod page_capture;
mod preview;
mod session;
mod view;

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use cambium::{
    CompositionEvent, DomHandle, GenetAppRunner, Key, KeyEvent, Modifiers, NamedKey, PointerClick,
    TextInput,
};
use document_session_api::{
    SessionButtonState, SessionEffect, SessionFocusDirection, SessionIme, SessionKey,
    SessionModifiers, SessionPendingWork,
};
use genet_scripted_dom::ScriptedDom;
use inker::{
    DocumentSession, SessionClick, SessionEngine, SessionError, SessionScrollKey,
    SessionSpawnRequest,
};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use netrender::Scene;

use crate::ui::{PaneScroll, RetainedLayout};
use connection::{Command, Connection};
use session::{PracticeSession, Runner};

pub(crate) const ENGINE_ID: &str = "turnstone.practice-wav";

pub(crate) fn local_recording(address: &str) -> Option<PathBuf> {
    let url = url::Url::parse(address).ok()?;
    if url.scheme() != "file"
        || url.host_str().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let path = url.to_file_path().ok()?;
    (path.is_absolute() && crate::woodshed_client::is_recording_path(&path)).then_some(path)
}

pub(crate) fn normalize_recording_path(input: &str) -> Option<String> {
    if local_recording(input).is_some() {
        return url::Url::parse(input).ok().map(|url| url.into());
    }
    let raw = input.trim();
    let path = Path::new(
        raw.strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(raw),
    );
    if !crate::woodshed_client::is_recording_path(path) || (!path.is_absolute() && !path.is_file())
    {
        return None;
    }
    let url = url::Url::from_file_path(std::path::absolute(path).ok()?).ok()?;
    local_recording(url.as_str()).map(|_| url.into())
}

pub(crate) fn blocks_content_navigation(address: &str) -> bool {
    local_recording(address).is_some()
        || (Path::new(address).is_absolute()
            && crate::woodshed_client::is_recording_path(Path::new(address)))
}

pub(super) struct PracticeAudioEngine;

impl SessionEngine<Scene> for PracticeAudioEngine {
    fn engine_id(&self) -> &str {
        ENGINE_ID
    }

    fn spawn(
        &self,
        request: &SessionSpawnRequest,
    ) -> Result<Box<dyn DocumentSession<Scene>>, SessionError> {
        let path = local_recording(&request.address).ok_or_else(|| {
            SessionError::SpawnFailed("Choose a local WAV, MP3 or MP4 file.".into())
        })?;
        let compressed = compressed::is_compressed(&path);
        let duration = if compressed {
            0
        } else {
            preview::duration_ms(&path).map_err(SessionError::SpawnFailed)?
        };
        let title = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut state = State {
            path,
            title,
            duration,
            position: 0,
            playing: false,
            device: None,
            preparing: None,
            prepared: None,
            seek: TextInput::new("0"),
            start: TextInput::default(),
            end: TextInput::default(),
            note: TextInput::default(),
            gain: TextInput::new("75"),
            connection: None,
            executable: None,
            attached: false,
            imported: false,
            executable_choices: Vec::new(),
            choose_executable: false,
            remote: None,
            status: String::new(),
        };
        if compressed {
            state.prepare(false).map_err(SessionError::SpawnFailed)?;
        }
        let dom = Rc::new(RefCell::new(ScriptedDom::new()));
        let runner = Runner::new(
            dom.clone(),
            view::render as fn(&State) -> view::PracticeView,
            state,
        );
        Ok(Box::new(PracticeSession {
            address: request.address.clone(),
            size: request.viewport,
            dom,
            runner,
            layout: RetainedLayout::new(),
            scroll: PaneScroll::new(),
        }))
    }
}

struct State {
    path: PathBuf,
    title: String,
    duration: u64,
    position: u64,
    playing: bool,
    device: Option<preview::Device>,
    preparing: Option<Preparing>,
    prepared: Option<Arc<tempfile::TempPath>>,
    seek: TextInput,
    start: TextInput,
    end: TextInput,
    note: TextInput,
    gain: TextInput,
    connection: Option<Connection>,
    executable: Option<PathBuf>,
    attached: bool,
    imported: bool,
    executable_choices: Vec<PathBuf>,
    choose_executable: bool,
    remote: Option<serde_json::Value>,
    status: String,
}

struct Preparing {
    worker: Option<std::thread::JoinHandle<()>>,
    autoplay: bool,
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<tempfile::NamedTempFile, String>>,
}

impl Drop for Preparing {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl State {
    fn update_audio(&mut self) -> Result<(), String> {
        if let Some(device) = &self.device {
            self.position = device.position()?.min(self.duration);
            self.playing = device.playing()?;
        }
        Ok(())
    }

    fn play(&mut self) -> Result<(), String> {
        if self.attached {
            return Err("Detach the Woodshed transport before previewing the source.".into());
        }
        // A WAV already has a marking clock while its optional device conversion runs.
        if self.duration > 0 && self.preparing.take().is_some() {
            self.status.clear();
            return Ok(());
        }
        if let Some(preparing) = &mut self.preparing {
            preparing.autoplay = !preparing.autoplay;
            return Ok(());
        }
        if let Some(device) = self.device.as_ref().filter(|_| self.playing) {
            device.pause()?;
        } else {
            if self.device.is_none() {
                if self.prepared.is_none() && compressed::is_compressed(&self.path) {
                    return self.prepare(true);
                }
                let path = self
                    .prepared
                    .as_ref()
                    .map_or(self.path.as_path(), |preview| preview.as_ref().as_ref());
                match preview::Device::open(path) {
                    Ok(device) => self.device = Some(device),
                    #[cfg(windows)]
                    Err(_) if self.prepared.is_none() => return self.prepare(true),
                    Err(error) => return Err(error),
                }
            }
            self.resume_preview()?;
        }
        self.update_audio()
    }

    fn resume_preview(&mut self) -> Result<(), String> {
        let device = self
            .device
            .as_ref()
            .ok_or("Open the audio preview first.")?;
        if self.position >= self.duration {
            self.position = 0;
        }
        device.seek(self.position)?;
        device.play()
    }

    fn prepare(&mut self, autoplay: bool) -> Result<(), String> {
        let path = self.path.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (sender, result) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("recording-preview".into())
            .spawn(move || {
                let result = if compressed::is_compressed(&path) {
                    compressed::convert(&path, &worker_cancel)
                } else {
                    preview::convert(&path, &worker_cancel)
                };
                let _ = sender.send(result);
            })
            .map_err(|error| error.to_string())?;
        self.preparing = Some(Preparing {
            worker: Some(worker),
            autoplay,
            cancel,
            result,
        });
        self.status = "Preparing audio preview…".into();
        Ok(())
    }

    fn poll_preparation(&mut self) {
        let Some(preparing) = &self.preparing else {
            return;
        };
        let result = match preparing.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("Audio preview stopped.".into()),
        };
        let autoplay = preparing.autoplay;
        self.preparing = None;
        let result = result.and_then(|converted| {
            self.duration = preview::duration_ms(converted.path())?;
            // MCI opens wave files denying shared write. Keep ownership of the
            // temp path after releasing the writable file handle.
            self.prepared = Some(Arc::new(converted.into_temp_path()));
            if autoplay {
                self.play()?;
            }
            Ok(())
        });
        self.status = result.err().unwrap_or_default();
    }

    fn seek(&mut self) -> Result<(), String> {
        let ms = parse_time(&self.seek, self.duration)?;
        if let Some(device) = &self.device {
            device.seek(ms)?;
            if self.playing && ms < self.duration {
                device.play()?;
            }
        }
        self.position = ms;
        self.playing &= ms < self.duration;
        Ok(())
    }

    fn mark(&mut self, start: bool) -> Result<(), String> {
        self.update_audio()?;
        let field = if start {
            &mut self.start
        } else {
            &mut self.end
        };
        *field = TextInput::new(format_time(self.position));
        Ok(())
    }

    fn send(&mut self) -> Result<(), String> {
        if compressed::is_compressed(&self.path) && self.prepared.is_none() {
            return Err(if self.preparing.is_some() {
                "Wait for the audio preview to finish preparing.".to_owned()
            } else {
                self.status.clone()
            });
        }
        let start_ms = parse_time(&self.start, self.duration)?;
        let end_ms = parse_time(&self.end, self.duration)?;
        if end_ms <= start_ms {
            return Err("The out point must be later than the in point.".into());
        }
        self.device = None;
        self.preparing = None;
        self.playing = false;
        let argument = crate::woodshed_client::ImportWav {
            wav: crate::woodshed_client::LocalWavSelection::from_native_recording(&self.path)
                .map_err(|error| error.to_string())?,
            title: self.title.clone(),
            start_ms,
            end_ms,
            note: self.note.text().trim().to_owned(),
        };
        self.remote_command(Command::Send {
            request: argument,
            preview: self
                .prepared
                .clone()
                .filter(|_| compressed::is_compressed(&self.path)),
        })
    }

    fn remote_command(&mut self, command: Command) -> Result<(), String> {
        if matches!(command, Command::Attach) {
            self.device = None;
            self.preparing = None;
            self.playing = false;
        }
        if self.connection.is_none() {
            self.connection = Some(Connection::open(self.executable.clone())?);
        }
        self.connection
            .as_mut()
            .expect("connection created")
            .request(command)?;
        self.status = "Contacting Woodshed…".into();
        Ok(())
    }

    fn select_executable(&mut self, path: PathBuf) -> Result<(), String> {
        self.executable = Some(path);
        self.executable_choices.clear();
        self.choose_executable = false;
        self.connection = None;
        self.send()
    }

    fn browse_executable(&mut self) -> Result<(), String> {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Choose Woodshed")
            .pick_file()
        {
            self.select_executable(path)?;
        }
        Ok(())
    }

    fn tick(&mut self) {
        self.poll_preparation();
        if let Err(error) = self.update_audio() {
            self.device = None;
            self.playing = false;
            self.status = error;
        }
        if let Some(connection) = &mut self.connection {
            if let Some(result) = connection.result() {
                match result {
                    Ok(reply) => {
                        self.imported |= reply.imported;
                        if reply.attached == Some(true)
                            && let Some(gain) = reply
                                .status
                                .as_ref()
                                .and_then(|status| status.get("gain"))
                                .and_then(serde_json::Value::as_u64)
                        {
                            self.gain = TextInput::new(gain.to_string());
                        }
                        self.attached = reply
                            .status
                            .as_ref()
                            .and_then(|status| status.get("view"))
                            .and_then(serde_json::Value::as_str)
                            .map_or_else(
                                || reply.attached.unwrap_or(self.attached),
                                |view| view == "pane",
                            );
                        if let Some(status) = reply.status {
                            self.remote = Some(status);
                        }
                        if let Some(message) = reply.message {
                            self.status = message.into();
                        }
                    },
                    Err(connection::Failure::Choice(paths)) => {
                        self.executable_choices = paths;
                        self.choose_executable = true;
                        self.status = "Choose the Woodshed executable.".into();
                    },
                    Err(connection::Failure::Executable(error)) => {
                        self.choose_executable = true;
                        self.status = error;
                    },
                    Err(connection::Failure::Disconnected(error)) => {
                        self.imported = false;
                        self.attached = false;
                        self.remote = None;
                        self.device = None;
                        self.preparing = None;
                        self.playing = false;
                        self.status = error;
                    },
                    Err(connection::Failure::Error(error)) => self.status = error,
                }
            }
            if self.attached {
                let _ = connection.request(Command::Status);
            }
        }
    }
}

fn parse_time(field: &TextInput, duration: u64) -> Result<u64, String> {
    let parse = || {
        let input = field.text().trim();
        let (seconds, fraction) = input.split_once('.').unwrap_or((input, ""));
        if input.is_empty()
            || (seconds.is_empty() && fraction.is_empty())
            || fraction.len() > 3
            || !fraction.bytes().all(|c| c.is_ascii_digit())
            || !seconds.bytes().all(|c| c.is_ascii_digit())
        {
            return None;
        }
        let seconds = if seconds.is_empty() {
            0
        } else {
            seconds.parse::<u64>().ok()?
        };
        let fraction_ms = if fraction.is_empty() {
            0
        } else {
            fraction.parse::<u64>().ok()? * 10u64.pow(3 - fraction.len() as u32)
        };
        seconds
            .checked_mul(1000)?
            .checked_add(fraction_ms)
            .filter(|ms| *ms <= duration)
    };
    parse().ok_or_else(|| format!("Enter seconds from 0 to {}.", format_time(duration)))
}

fn format_time(ms: u64) -> String {
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

#[cfg(test)]
mod tests;
