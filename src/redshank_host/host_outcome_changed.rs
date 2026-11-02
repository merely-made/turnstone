// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl HostOutcome {
    pub(super) fn changed() -> Self {
        Self {
            model_changed: true,
            ..Self::default()
        }
    }

    pub(super) fn notice(message: impl Into<String>) -> Self {
        Self {
            notice: Some(message.into()),
            ..Self::default()
        }
    }
}
