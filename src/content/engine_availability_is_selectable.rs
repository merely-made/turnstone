// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl EngineAvailability {
    pub fn is_selectable(&self) -> bool {
        matches!(self, Self::Registered | Self::OnDemand { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Registered => "registered".into(),
            Self::Disabled => "disabled".into(),
            Self::OnDemand { detail } => format!("on demand: {detail}"),
            Self::Unavailable { reason } => format!("unavailable: {reason}"),
        }
    }
}
