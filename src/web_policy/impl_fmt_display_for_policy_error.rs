// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(reason) => write!(f, "policy registry I/O failed: {reason}"),
            Self::InvalidData(reason) => write!(f, "invalid policy registry: {reason}"),
            Self::ProfileMismatch { expected, found } => {
                write!(
                    f,
                    "policy registry belongs to profile {found}, not {expected}"
                )
            },
            Self::RequestNotPending => f.write_str("user-agent request is no longer pending"),
        }
    }
}
