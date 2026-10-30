// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for ShellTranscript {
    fn default() -> Self {
        Self {
            next_id: 1,
            retention: TranscriptRetention::default(),
            entries: VecDeque::new(),
        }
    }
}
