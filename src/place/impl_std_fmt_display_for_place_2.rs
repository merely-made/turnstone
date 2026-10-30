// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl std::fmt::Display for PlaceBindingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported place binding version {version}")
            },
            Self::EmptyDefaultChannel => write!(formatter, "default channel is empty"),
            Self::DefaultChannelTooLong(length) => {
                write!(
                    formatter,
                    "default channel is {length} bytes; maximum is 128"
                )
            },
            Self::DefaultChannelHasControl => {
                write!(formatter, "default channel contains a control character")
            },
        }
    }
}
