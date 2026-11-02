// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl CapabilityStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Supported | Self::Partial { .. })
    }

    /// Stable person- and probe-facing description.
    pub fn describe(&self) -> String {
        match self {
            Self::Supported => "supported".to_string(),
            Self::Unsupported { reason } => format!("unavailable: {reason}"),
            Self::Partial { detail } => format!("partial: {detail}"),
        }
    }
}
