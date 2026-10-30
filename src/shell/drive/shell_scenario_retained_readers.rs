// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn scenario_retained_readers(
        &mut self,
        selected: &Option<Vec<crate::research_search::Member>>,
    ) -> Vec<serde_json::Value> {
        let mut retained_readers = Vec::new();
        let mut members: Vec<_> = self
            .app
            .content
            .live_nodes()
            .filter(|node| {
                self.app
                    .content
                    .facts(*node)
                    .is_some_and(|f| f.engine == crate::retained_clip::ENGINE_ID)
            })
            .collect();
        members.sort();
        let admissions = self.app.research_clip_admissions();
        for node in members {
            let Some(reader) = self.app.research_clip_readers.get(&node) else {
                continue;
            };
            let source_current = self
                .app
                .graph_runtimes
                .get(reader.target.clip.graph)
                .is_some_and(|runtime| {
                    crate::browse::still_current(
                        &runtime.canvas,
                        node,
                        &crate::retained_clip::address(reader.target.clip.manifest),
                    )
                });
            let scope = crate::research_scenario_privacy::ClipReaderScope {
                selected,
                session: self.app.session_id,
                generation: self.app.source_capture_generation,
                identity: self.app.identity.as_ref(),
                custody_epoch: self
                    .app
                    .research
                    .clip_access()
                    .ok()
                    .and_then(|access| (access.epoch)()),
                admissions: &admissions,
                source_current,
            };
            let Some(target) =
                crate::research_scenario_privacy::clip_reader_target(node, reader, &scope)
            else {
                continue;
            };
            if let Some(session) = self
                .content_sessions
                .session_mut(&node)
                .and_then(|session| {
                    session
                        .as_any()
                        .downcast_mut::<mere_document_lanes::SmolwebDocumentSession>()
                })
            {
                retained_readers.push(serde_json::json!({ "node": node, "engine": crate::retained_clip::ENGINE_ID, "target": target, "rendered_document": session.document_mut().document().clone() }));
            }
        }
        retained_readers
    }
}
