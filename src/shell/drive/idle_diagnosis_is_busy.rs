// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl IdleDiagnosis {
    pub(super) fn is_busy(&self) -> bool {
        self.pending_fetches
            || self.requested_content
            || self.graph_settling
            || !self.unsettled_sessions.is_empty()
    }

    pub(super) fn content_ready(&self) -> bool {
        !self.pending_fetches && !self.requested_content && self.unsettled_sessions.is_empty()
    }

    pub(super) fn describe(&self) -> String {
        let sessions = self
            .unsettled_sessions
            .iter()
            .map(uuid::Uuid::to_string)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "busy={} pending_fetches={} requested_content={} graph_settling={} unsettled_sessions=[{}]",
            self.is_busy(),
            self.pending_fetches,
            self.requested_content,
            self.graph_settling,
            sessions
        )
    }
}
