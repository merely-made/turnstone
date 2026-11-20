// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::saved_sources::{KeptSource, Key};

impl App {
    pub(crate) fn kept_structured_source(&self, member: uuid::Uuid) -> Option<KeptSource> {
        let graph = self.graph_runtimes.graph_containing_member(member)?;
        let runtime = self.graph_runtimes.get(graph)?;
        let url = runtime.canvas.graph().get_node_by_id(member)?.1.url();
        let live = self.content.source_capture_admission(member, url);
        self.source_captures
            .values()
            .filter(|attempt| {
                attempt.request.graph == graph
                    && attempt.request.node == member
                    && attempt.request.url == url
                    && live.is_none_or(|(_, identity)| {
                        attempt.request.source.as_ref() == Some(identity)
                    })
            })
            .filter_map(|attempt| {
                self.kept_structured_target(attempt).map(|target| {
                    let acquired_at = attempt
                        .request
                        .source
                        .as_ref()
                        .map(|source| source.acquired_at_ms);
                    ((acquired_at, attempt.ordinal), target)
                })
            })
            .max_by_key(|(ordinal, _)| *ordinal)
            .map(|(_, target)| target)
    }

    fn kept_structured_target(&self, attempt: &CaptureAttempt) -> Option<KeptSource> {
        let request = &attempt.request;
        if attempt.phase != CapturePhase::Saved
            || !request.keep
            || !self.research.is_available()
            || self.session_load_refused()
            || self
                .saved_capture_owner(request, attempt.stored.as_ref())
                .is_err()
        {
            return None;
        }
        Some(KeptSource {
            key: Key {
                session: request.session,
                graph: request.graph,
                node: request.node,
                annotation_manifest: attempt.stored.as_ref()?.annotation_manifest.clone(),
            },
            request: request.id,
            capture_hash: request.source.as_ref()?.hash.clone(),
        })
    }

    pub(crate) fn admit_structured_validation(
        &self,
        target: &KeptSource,
    ) -> Option<&CaptureAttempt> {
        let attempt = self.source_captures.get(&target.request)?;
        (self.kept_structured_source(target.key.node).as_ref() == Some(target)).then_some(attempt)
    }

    pub(crate) fn admits_neighborhood_validation(
        &self,
        pane: crate::panes::PaneId,
        source: &KeptSource,
    ) -> bool {
        if !self.pane_content(pane).is_some_and(|content| {
            content.kind_id().as_str() == crate::panes::kind::RESOURCE_NEIGHBORHOOD
        }) || self.admit_structured_validation(source).is_none()
        {
            return false;
        }
        let context = self.follower_context(pane);
        let graph = context
            .and_then(|context| context.graph)
            .or_else(|| self.graph_for_pane(pane));
        let member = context
            .and_then(|context| context.member)
            .or_else(|| self.graph_runtimes.canvas(graph?)?.focused_member());
        graph == Some(source.key.graph) && member == Some(source.key.node)
    }

    pub(crate) fn validate_kept_source(
        &self,
        pane: crate::panes::PaneId,
        source: KeptSource,
        shapes: crate::swatch_pane::validation::Shapes,
    ) -> Vec<Effect> {
        if self.admits_neighborhood_validation(pane, &source) {
            vec![Effect::ValidateKeptSource {
                pane,
                source,
                shapes,
            }]
        } else {
            vec![]
        }
    }

    pub(crate) fn reveal_validation_evidence(
        &self,
        pane: crate::panes::PaneId,
        source: KeptSource,
        order: u64,
    ) -> Vec<Effect> {
        if self.admits_neighborhood_validation(pane, &source) {
            vec![Effect::RevealValidationEvidence {
                pane,
                source,
                order,
            }]
        } else {
            vec![]
        }
    }
}
