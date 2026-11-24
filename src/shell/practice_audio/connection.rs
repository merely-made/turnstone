// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::{path::PathBuf, sync::mpsc, thread};

use crate::woodshed_client::{ClientError, WoodshedSession, connect_or_launch};

pub(super) enum Command {
    Send(super::clip::RecordingImport),
    Status,
    Attach,
    Detach,
    Play,
    Pause,
    Gain(u8),
}

pub(super) struct Reply {
    pub(super) status: Option<serde_json::Value>,
    pub(super) attached: Option<bool>,
    pub(super) imported: bool,
    pub(super) message: Option<&'static str>,
}

pub(super) enum Failure {
    Choice(Vec<PathBuf>),
    Executable(String),
    Disconnected(String),
    Error(String),
}

type Answer = Result<Reply, Failure>;

pub(super) struct Connection {
    commands: mpsc::Sender<Command>,
    results: mpsc::Receiver<Answer>,
    pending: u32,
}

impl Connection {
    pub(super) fn open(executable: Option<PathBuf>) -> Result<Self, String> {
        let (commands, incoming) = mpsc::channel();
        let (outgoing, results) = mpsc::channel();
        thread::Builder::new()
            .name("woodshed-transport".into())
            .spawn(move || {
                let mut session = None;
                for command in incoming {
                    if outgoing
                        .send(execute(&mut session, executable.clone(), command))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            commands,
            results,
            pending: 0,
        })
    }

    pub(super) fn request(&mut self, command: Command) -> Result<(), String> {
        if matches!(command, Command::Status) && self.pending != 0 {
            return Ok(());
        }
        if self.pending >= 3 {
            return Err("Woodshed is still processing the previous controls.".into());
        }
        self.commands
            .send(command)
            .map_err(|_| "Woodshed connection stopped.".to_string())?;
        self.pending += 1;
        Ok(())
    }

    pub(super) fn busy(&self) -> bool {
        self.pending != 0
    }

    pub(super) fn result(&mut self) -> Option<Answer> {
        match self.results.try_recv() {
            Ok(result) => {
                self.pending = self.pending.saturating_sub(1);
                Some(result)
            },
            Err(mpsc::TryRecvError::Disconnected) if self.pending != 0 => {
                self.pending = 0;
                Some(Err(Failure::Error("Woodshed connection stopped.".into())))
            },
            Err(_) => None,
        }
    }
}

pub(super) fn execute(
    session: &mut Option<WoodshedSession>,
    executable: Option<PathBuf>,
    command: Command,
) -> Answer {
    if session.is_none() {
        match connect_or_launch(executable) {
            Ok(connected) => *session = Some(connected),
            Err(ClientError::ExecutableChoiceRequired(paths)) => {
                return Err(Failure::Choice(paths));
            },
            Err(
                error @ (ClientError::ExecutableNotFound | ClientError::ExecutablePathNotFound(_)),
            ) => {
                return Err(Failure::Executable(error.to_string()));
            },
            Err(error) => return Err(Failure::Error(error.to_string())),
        }
    }
    let connected = session.as_mut().expect("connection established");
    let (response, attached, imported, message) = match command {
        Command::Send(import) if import.selected.is_some() => {
            let phrase = import.prepare().map_err(Failure::Error)?;
            (
                connected.import_page_phrase(&phrase),
                None,
                true,
                Some("Matched passage sent to Woodshed."),
            )
        },
        Command::Send(import) => (
            connected.import_recording(
                &import.request,
                import.preview.as_deref().map(AsRef::as_ref),
            ),
            None,
            true,
            Some("Phrase sent to Woodshed."),
        ),
        Command::Status => (connected.status(), None, false, None),
        Command::Attach => (
            connected.attach(),
            Some(true),
            false,
            Some("Woodshed transport attached."),
        ),
        Command::Detach => (
            connected.detach(),
            Some(false),
            false,
            Some("Woodshed transport detached."),
        ),
        Command::Play => (connected.play(), None, false, Some("Loop playing.")),
        Command::Pause => (connected.pause(), None, false, Some("Loop paused.")),
        Command::Gain(value) => (connected.set_gain(value), None, false, Some("Gain set.")),
    };
    match response {
        Ok(reply) => Ok(Reply {
            status: reply.status,
            attached,
            imported,
            message,
        }),
        Err(error) if error.invalidates_session() => {
            *session = None;
            Err(Failure::Disconnected(error.to_string()))
        },
        Err(error) => Err(Failure::Error(error.to_string())),
    }
}
