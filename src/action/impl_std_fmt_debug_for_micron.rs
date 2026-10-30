// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl std::fmt::Debug for MicronSubmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MicronSubmission")
            .field("target", &self.target)
            .field("field_count", &self.values.len())
            .finish()
    }
}
