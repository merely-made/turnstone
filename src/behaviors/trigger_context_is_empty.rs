// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl TriggerContext {
    /// Whether anything woke this run. A manually invoked body has an empty
    /// context rather than a missing one, so a script can always ask.
    pub fn is_empty(&self) -> bool {
        self.woken_by.is_empty()
    }

    /// The wire form handed to a body, mirroring how the snapshot travels.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"woken_by\":[]}".to_string())
    }
}
