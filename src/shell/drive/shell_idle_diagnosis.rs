// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn idle_diagnosis(&mut self) -> IdleDiagnosis {
        let _ = self.pump_visible_documents();
        let mut unsettled_sessions = self
            .content_sessions
            .pool
            .pending_contents()
            .map(|key: &super::super::content_pool::ContentKey| match *key {
                super::super::content_pool::ContentKey::Node(node)
                | super::super::content_pool::ContentKey::Reader { node, .. } => node,
            })
            .collect::<Vec<_>>();
        unsettled_sessions.sort_unstable();
        unsettled_sessions.dedup();
        IdleDiagnosis {
            pending_fetches: self.pending_fetches.any_in_flight(),
            requested_content: self.app.content.any_requested(),
            graph_settling: self.app.graph_runtimes.is_settling(),
            unsettled_sessions,
        }
    }
}
