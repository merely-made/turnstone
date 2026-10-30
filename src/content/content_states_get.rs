// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ContentStates {
    pub fn get(&self, node: Uuid) -> Option<&NodeContent> {
        self.states.get(&node)
    }

    /// The mirrored facts for a live node (absent for requested/failed/none).
    pub fn facts(&self, node: Uuid) -> Option<&ContentFacts> {
        self.facts.get(&node)
    }

    pub(crate) fn surface_traversal_pending(&self, node: Uuid) -> bool {
        self.surface_traversals.contains_key(&node)
    }

    pub(crate) fn begin_surface_traversal(
        &mut self,
        node: Uuid,
        from: String,
        target: String,
        back: bool,
    ) {
        self.surface_traversals.insert(
            node,
            SurfaceTraversal {
                from,
                target,
                back,
                request: None,
            },
        );
    }

    pub(crate) fn take_surface_traversal(&mut self, node: Uuid) -> Option<SurfaceTraversal> {
        self.surface_traversals.remove(&node)
    }

    pub(crate) fn bind_surface_traversal_request(
        &mut self,
        node: Uuid,
        request: inker::webcue::RequestKey,
    ) {
        if let Some(traversal) = self.surface_traversals.get_mut(&node) {
            traversal.request = Some(request);
        }
    }

    pub(crate) fn refuse_surface_traversal_request(
        &mut self,
        node: Uuid,
        request: inker::webcue::RequestKey,
    ) -> bool {
        if self
            .surface_traversals
            .get(&node)
            .is_some_and(|traversal| traversal.request == Some(request))
        {
            self.note_surface_stopped(node);
            true
        } else {
            false
        }
    }

    /// Effective shared controls take precedence over the legacy vocabulary.
    /// Host loading/history remain separate from engine-owned navigation.
    pub fn offers(&self, node: Uuid, feature: inker::webcue::Feature) -> bool {
        if let Some(report) = self.cue_report(node) {
            return report.feature(feature).permits();
        }
        let Some(facts) = self.facts(node) else {
            return false;
        };
        use inker::webcue::Feature;
        match feature {
            Feature::Find => facts.capabilities.find_in_page.is_available(),
            Feature::Zoom => facts.capabilities.page_zoom.is_available(),
            Feature::Capture => facts.capabilities.page_capture.is_available(),
            Feature::Navigation | Feature::Reload | Feature::Stop | Feature::History => {
                facts.capabilities.navigation.is_available()
            },
            _ => false,
        }
    }

    pub fn cue_report(&self, node: Uuid) -> Option<&inker::webcue::CapabilityReport> {
        self.cue_reports.get(&node)
    }

    pub fn note_cue_report(&mut self, node: Uuid, report: inker::webcue::CapabilityReport) {
        if matches!(self.states.get(&node), Some(NodeContent::Live)) {
            self.cue_reports.insert(node, report);
        }
    }

    /// What the node's live engine last reported it is presenting at. `None`
    /// where the lane has no read-back at all, which is the honest answer for
    /// every hosted surface today.
    pub fn page_zoom(&self, node: Uuid) -> Option<PageZoomFacts> {
        self.page_zoom.get(&node).copied()
    }

    /// Record one engine's answer to a page-zoom request.
    pub fn note_page_zoom(&mut self, node: Uuid, zoom: PageZoomFacts) {
        self.page_zoom.insert(node, zoom);
    }

    /// Whether a flip intent on `node` should spawn (true) or close (false):
    /// live and in-flight content toggles OFF; empty and failed toggle ON
    /// (a failed node retries — failure is a state, not a latch).
    pub fn flip_spawns(&self, node: Uuid) -> bool {
        !matches!(
            self.states.get(&node),
            Some(
                NodeContent::Live
                    | NodeContent::Requested
                    | NodeContent::AwaitingInput
                    | NodeContent::AwaitingIdentity
                    | NodeContent::AwaitingTrust
            )
        )
    }

    pub fn note_requested(&mut self, node: Uuid) {
        self.states.insert(node, NodeContent::Requested);
        self.facts.remove(&node);
        self.cue_reports.remove(&node);
        self.surface_traversals.remove(&node);
        self.page_zoom.remove(&node);
    }

    /// Begin one actor-backed request and return the exact older request it
    /// supersedes, if any. Network phase is independent of session lifecycle.
    pub fn begin_fetch(
        &mut self,
        node: Uuid,
        request: fetch::FetchRequestId,
    ) -> Option<fetch::FetchRequestId> {
        self.source_admissions.remove(&node);
        self.loads.entry(node).or_default().begin(request)
    }

    pub fn active_fetch(&self, node: Uuid) -> Option<fetch::FetchRequestId> {
        self.loads
            .get(&node)
            .and_then(page_load::PageLoad::active_request)
    }

    pub fn is_active_fetch(&self, node: Uuid, request: fetch::FetchRequestId) -> bool {
        self.active_fetch(node) == Some(request)
    }

    pub fn finish_fetch(&mut self, node: Uuid, request: fetch::FetchRequestId) -> bool {
        self.loads
            .get_mut(&node)
            .is_some_and(|load| load.finish(request))
    }

    pub fn settle_fetch(&mut self, node: Uuid, request: fetch::FetchRequestId) -> bool {
        self.loads
            .get_mut(&node)
            .is_some_and(|load| load.settle(request))
    }

    pub fn stop_fetch(&mut self, node: Uuid, request: fetch::FetchRequestId) -> bool {
        self.loads
            .get_mut(&node)
            .is_some_and(|load| load.stop(request))
    }

    pub fn note_surface_started(&mut self, node: Uuid) {
        self.loads.entry(node).or_default().surface_started();
    }

    pub fn note_surface_progress(&mut self, node: Uuid, value: f32) {
        self.loads.entry(node).or_default().surface_progress(value);
    }

    pub fn note_surface_settled(&mut self, node: Uuid) {
        self.surface_traversals.remove(&node);
        self.loads.entry(node).or_default().surface_settled();
    }

    pub fn note_surface_stopped(&mut self, node: Uuid) {
        self.surface_traversals.remove(&node);
        self.loads.entry(node).or_default().surface_stopped();
    }

    pub fn note_live(&mut self, node: Uuid, facts: Option<ContentFacts>) {
        self.states.insert(node, NodeContent::Live);
        self.cue_reports.remove(&node);
        self.surface_traversals.remove(&node);
        match facts {
            Some(facts) => {
                self.facts.insert(node, facts);
            },
            None => {
                self.facts.remove(&node);
            },
        }
    }

    pub fn note_awaiting_input(&mut self, node: Uuid) {
        self.states.insert(node, NodeContent::AwaitingInput);
        self.facts.remove(&node);
        self.cue_reports.remove(&node);
        self.surface_traversals.remove(&node);
        self.page_zoom.remove(&node);
    }

    pub fn note_awaiting_identity(&mut self, node: Uuid) {
        self.states.insert(node, NodeContent::AwaitingIdentity);
        self.facts.remove(&node);
        self.cue_reports.remove(&node);
        self.surface_traversals.remove(&node);
        self.page_zoom.remove(&node);
    }

    pub fn note_awaiting_trust(&mut self, node: Uuid) {
        self.states.insert(node, NodeContent::AwaitingTrust);
        self.facts.remove(&node);
        self.cue_reports.remove(&node);
        self.surface_traversals.remove(&node);
        self.page_zoom.remove(&node);
    }

    /// Retain the actor-owned response under both member and address. The URL
    /// guard makes a late or superseded body unusable for a later navigation.
    pub fn note_fetched(
        &mut self,
        node: Uuid,
        url: String,
        document: FetchedDocument,
        received_bytes: usize,
    ) {
        self.source_admissions.insert(
            node,
            (
                Uuid::new_v4(),
                crate::source_capture::SourceIdentity::of(&document),
            ),
        );
        self.loads
            .entry(node)
            .or_default()
            .note_fetched(url, document, received_bytes);
    }

    /// Append one exact transport fragment and refresh the decoded document
    /// from the whole prefix, so split UTF-8 code points cannot corrupt later
    /// replacement frames.
    pub fn note_streamed(
        &mut self,
        node: Uuid,
        url: String,
        response_url: String,
        content_type: Option<String>,
        chunk: &[u8],
    ) -> usize {
        self.source_admissions.remove(&node);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        self.loads.entry(node).or_default().note_streamed(
            url,
            response_url,
            content_type,
            chunk,
            now_ms,
        )
    }

    pub fn fetch_phase(&self, node: Uuid) -> Option<&PageFetchPhase> {
        self.loads.get(&node).and_then(page_load::PageLoad::phase)
    }

    pub fn fetch_in_progress(&self, node: Uuid) -> bool {
        self.loads
            .get(&node)
            .is_some_and(page_load::PageLoad::in_progress)
    }

    pub fn fetched(&self, node: Uuid, url: &str) -> Option<&FetchedDocument> {
        self.loads.get(&node).and_then(|load| load.fetched(url))
    }

    /// A source-capture candidate is available only for the exact current
    /// node/address pair. Incidental recall text never enters this seam.
    pub fn source_capture_candidate(&self, node: Uuid, url: &str) -> Option<&FetchedDocument> {
        self.fetched(node, url)
            .filter(|document| {
                document.content_type.as_deref().is_some_and(|type_| {
                    type_
                        .split(';')
                        .next()
                        .is_some_and(|media| media.trim().eq_ignore_ascii_case("text/html"))
                })
            })
            .filter(|document| document.effective_url.is_some() && document.acquired_at_ms != 0)
    }

    /// Forget fetched source bodies and invalidate every outstanding fetch
    /// admission. A terminal answer already queued for an older request then
    /// fails `finish_fetch` and cannot repopulate this cache.
    pub(crate) fn clear_source_documents(&mut self) {
        for load in self.loads.values_mut() {
            load.clear();
        }
        self.source_admissions.clear();
    }

    /// Only a complete, settled source can be offered for a new capture.
    /// Fingerprints are computed at admission rather than on every UI frame.
    pub(crate) fn source_capture_admission(
        &self,
        node: Uuid,
        url: &str,
    ) -> Option<(Uuid, &crate::source_capture::SourceIdentity)> {
        if self.fetch_in_progress(node)
            || !matches!(self.fetch_phase(node), Some(PageFetchPhase::Settled { .. }))
        {
            return None;
        }
        self.source_capture_candidate(node, url)?;
        self.source_admissions
            .get(&node)
            .map(|(token, source)| (*token, source))
    }

    pub fn forget_fetched(&mut self, node: Uuid) {
        self.source_admissions.remove(&node);
        if let Some(load) = self.loads.get_mut(&node) {
            load.forget_fetched();
        }
    }

    /// Remove every transient fact owned by a node that has left the graph.
    pub fn forget_node(&mut self, node: Uuid) {
        self.source_admissions.remove(&node);
        self.note_closed(node);
        self.loads.remove(&node);
    }

    pub fn note_failed(&mut self, node: Uuid, error: String) {
        self.states.insert(node, NodeContent::Failed(error));
        self.facts.remove(&node);
        self.cue_reports.remove(&node);
        self.surface_traversals.remove(&node);
        self.page_zoom.remove(&node);
        if let Some(load) = self.loads.get_mut(&node) {
            load.fail();
        }
    }

    /// The node's content is gone (closed, or the port dropped it).
    pub fn note_closed(&mut self, node: Uuid) {
        self.states.remove(&node);
        self.facts.remove(&node);
        self.cue_reports.remove(&node);
        self.surface_traversals.remove(&node);
        self.page_zoom.remove(&node);
    }

    /// Nodes currently holding live sessions (the shell composes these).
    pub fn live_nodes(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.states
            .iter()
            .filter(|(_, s)| matches!(s, NodeContent::Live))
            .map(|(id, _)| *id)
    }

    /// Whether any spawn is still in flight (the effect is out, no session
    /// yet). The automation lane's quiescence read: the gap between the spawn
    /// effect and the live session must not read as quiet.
    pub fn any_requested(&self) -> bool {
        self.states
            .values()
            .any(|s| matches!(s, NodeContent::Requested))
    }
}
