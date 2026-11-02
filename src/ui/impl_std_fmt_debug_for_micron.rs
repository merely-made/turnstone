// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl std::fmt::Debug for MicronFormPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MicronFormPrompt")
            .field("source", &self.source)
            .field("source_url", &self.source_url)
            .field("form", &self.form)
            .field("action", &self.action)
            .field("resolved_target", &self.resolved_target)
            .field("stage", &self.stage)
            .finish()
    }
}
