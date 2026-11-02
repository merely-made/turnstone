// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for TranscriptRetention {
    fn default() -> Self {
        Self { max_entries: 200 }
    }
}
