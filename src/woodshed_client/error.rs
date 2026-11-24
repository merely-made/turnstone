// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0


use serde_json::Value;
use std::{fmt, io, path::PathBuf};

/// Errors suitable for a native pane's status area.
#[derive(Debug)]
pub enum ClientError {
    Io(io::Error),
    Json(serde_json::Error),
    Configuration(String),
    ExecutableNotFound,
    ExecutablePathNotFound(PathBuf),
    ExecutableChoiceRequired(Vec<PathBuf>),
    Launch {
        path: PathBuf,
        source: io::Error,
    },
    StartupTimeout {
        path: PathBuf,
        detail: Option<String>,
    },
    InvalidWav(String),
    InvalidRequest(String),
    Protocol(String),
    IdentityChanged,
    Remote {
        message: String,
        status: Option<Value>,
    },
}

impl fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "Woodshed connection failed: {error}"),
            Self::Json(error) => write!(formatter, "Woodshed handoff response was invalid: {error}"),
            Self::Configuration(message) | Self::InvalidWav(message) | Self::InvalidRequest(message) | Self::Protocol(message) => formatter.write_str(message),
            Self::ExecutableNotFound => formatter.write_str("Woodshed is not running and no Woodshed executable was found. Configure WOODSHED_EXE or choose an executable."),
            Self::ExecutablePathNotFound(path) => write!(formatter, "Woodshed executable does not exist: {}", path.display()),
            Self::ExecutableChoiceRequired(paths) => write!(formatter, "Choose a Woodshed executable: {}", paths.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(", ")),
            Self::Launch { path, source } => write!(formatter, "Could not start Woodshed at {}: {source}", path.display()),
            Self::StartupTimeout { path, detail } => write!(formatter, "Woodshed did not publish its handoff endpoint at {} before the startup timeout{}", path.display(), detail.as_ref().map(|detail| format!(": {detail}")).unwrap_or_default()),
            Self::IdentityChanged => formatter.write_str("The Woodshed process identity changed during this connection."),
            Self::Remote { message, .. } => write!(formatter, "Woodshed: {message}"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) | Self::Launch { source: error, .. } => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl ClientError {
    /// Whether a live stream should be discarded before the next explicit
    /// user action attempts to reconnect.
    pub fn invalidates_session(&self) -> bool {
        matches!(
            self,
            Self::Io(_) | Self::Json(_) | Self::Protocol(_) | Self::IdentityChanged
        )
    }
}

impl From<io::Error> for ClientError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
