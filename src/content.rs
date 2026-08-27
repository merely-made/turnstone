//! The content lane's app truth (rung 4, born ahead of its port): per-node
//! document lifecycle only. Charter per the architecture plan's module map:
//! engine registrations, per-node document lifecycle, the verso-tile flip,
//! content frames, and input routing — where the registry itself is
//! Inker's, never a hand-wired lane ladder.
//!
//! What lives HERE is the lifecycle state machine and nothing else. Live
//! document sessions are retained, non-`Send` handles, so the shell's
//! content port owns them (ports are the only owners of handles; `App`
//! holds data) keyed by the same node ids. The shell registers Livery, reader,
//! and native smolweb session engines at composition time; a `Requested` node
//! never silently spins when routing or spawning fails.

use std::collections::HashMap;

use uuid::Uuid;

/// Typed extraction lineage projected on a node rendered through `genet.reader`.
pub const READER_LINEAGE_FACET: &str = "web.reader-lineage";

/// One node's content lifecycle. Absent from the map = no content activity
/// (the at-rest state for every node).
#[derive(Clone, Debug, PartialEq)]
pub enum NodeContent {
    /// A spawn effect is in flight through the content port.
    Requested,
    /// A live session exists shell-side; frames compose into the node.
    Live,
    /// The actor reached a protocol input response and the host is waiting for
    /// a human answer. There is no live document handle yet.
    AwaitingInput,
    /// A Gemini capsule asked for a client certificate and the host is waiting
    /// for explicit approval to derive and present one.
    AwaitingIdentity,
    /// A Gemini server certificate changed and the host is waiting for an
    /// explicit trust decision before replacing the durable pin.
    AwaitingTrust,
    /// The last spawn failed; the reason is surfaced, never swallowed.
    Failed(String),
}

/// App-owned facts about a live session, mirrored out of the content port at
/// spawn (the adapter converts the service's report type at the boundary, so
/// this module stays port-agnostic). Data, not a handle: the Inspector pane
/// and the observation snapshot read these without reaching into the shell.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentFacts {
    /// The engine id the route decision picked (e.g. `genet.livery`).
    pub engine: String,
    /// The structural read, when the lane has one. `None` is reported
    /// honestly (a lane without introspection, not an empty document).
    pub structure: Option<StructureFacts>,
    /// Fleece derivation when the live representation is a reader rendering.
    pub lineage: Option<ExtractionLineageFacts>,
    /// Document controls reported by the owning retained or hosted engine.
    /// Reasons and partial-support details remain intact across the port
    /// boundary so product surfaces never infer support from a method's
    /// existence.
    pub capabilities: DocumentCapabilityFacts,
}

/// App-owned, port-agnostic mirror of one document control's availability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityStatus {
    Supported,
    Unsupported { reason: String },
    Partial { detail: String },
}

impl CapabilityStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Supported | Self::Partial { .. })
    }

    /// Stable person- and probe-facing description.
    pub fn describe(&self) -> String {
        match self {
            Self::Supported => "supported".to_string(),
            Self::Unsupported { reason } => format!("unavailable: {reason}"),
            Self::Partial { detail } => format!("partial: {detail}"),
        }
    }
}

/// The document controls Turnstone can present uniformly across engine kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentCapabilityFacts {
    pub find_in_page: CapabilityStatus,
    pub page_zoom: CapabilityStatus,
    pub page_capture: CapabilityStatus,
    pub navigation: CapabilityStatus,
}

impl Default for DocumentCapabilityFacts {
    fn default() -> Self {
        let unavailable = |feature: &str| CapabilityStatus::Unsupported {
            reason: format!("{feature} capability was not reported"),
        };
        Self {
            find_in_page: unavailable("find in page"),
            page_zoom: unavailable("page zoom"),
            page_capture: unavailable("page capture"),
            navigation: unavailable("navigation"),
        }
    }
}

/// What a live engine reports it is ACTUALLY presenting a document at, after
/// its own clamping and quantization. App-owned and port-agnostic, mirrored
/// out of the content port the same way the capability facts are.
///
/// This is runtime-only truth and never reaches the sidecar: the persisted
/// value is the node's REQUEST, and an engine's effective level belongs to the
/// live session that computed it. Lanes without a read-back (every hosted
/// surface today) simply have no entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageZoomFacts {
    /// The factor the host asked for, echoed back unchanged.
    pub requested: f32,
    /// The factor the engine is presenting at.
    pub applied: f32,
    /// The engine's own bounds, so a surface can say why a step was refused.
    pub min: f32,
    pub max: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractionLineageFacts {
    pub tool: String,
    pub version: String,
    pub selector: String,
    pub score: Option<i32>,
    pub block_count: usize,
}

/// One page body fetched by the shell actor and ready to hand to a document
/// engine. This transient representation prevents a live session from issuing
/// a second network request for bytes the host already owns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchedDocument {
    pub content_type: Option<String>,
    pub body: String,
}

/// Network progress remains app truth even after the first prefix has become
/// a live document session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageFetchPhase {
    Requested,
    Streaming {
        response_url: String,
        content_type: Option<String>,
        received_bytes: usize,
    },
    Settled {
        received_bytes: usize,
    },
    /// A hosted surface reports normalized load progress rather than bytes.
    Loading {
        progress_millis: Option<u16>,
    },
    /// The human stopped the active transfer before it settled.
    Stopped {
        received_bytes: usize,
    },
}

/// The structural read, mirrored in app-owned terms (the report type itself
/// stays port-side; the app holds what its surfaces present — the Inspector's
/// counts and the a11y projection's outline).
#[derive(Clone, Debug, PartialEq)]
pub struct StructureFacts {
    /// The document's own `<title>`.
    pub title: Option<String>,
    pub headings: usize,
    pub links: usize,
    /// The element outline (painted elements, document order): the a11y
    /// projection's document subtree is built from exactly this.
    pub outline: Vec<OutlineFact>,
}

/// One outline element: nesting depth, a coarse semantic role, and the
/// element's accessible name.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineFact {
    pub depth: usize,
    pub role: &'static str,
    pub name: String,
}

/// The app-truth side of the content lane: node id -> lifecycle, plus the
/// spawn-time facts for live nodes.
#[derive(Debug, Default)]
pub struct ContentStates {
    states: HashMap<Uuid, NodeContent>,
    facts: HashMap<Uuid, ContentFacts>,
    /// The effective page zoom a live engine reported back, per node. Purely
    /// transient: it dies with the session, and the save path never sees it.
    page_zoom: HashMap<Uuid, PageZoomFacts>,
    documents: HashMap<Uuid, (String, FetchedDocument)>,
    stream_bytes: HashMap<Uuid, (String, Vec<u8>)>,
    fetch_phases: HashMap<Uuid, PageFetchPhase>,
    active_fetches: HashMap<Uuid, fetch::FetchRequestId>,
}

impl ContentStates {
    pub fn get(&self, node: Uuid) -> Option<&NodeContent> {
        self.states.get(&node)
    }

    /// The mirrored facts for a live node (absent for requested/failed/none).
    pub fn facts(&self, node: Uuid) -> Option<&ContentFacts> {
        self.facts.get(&node)
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
        self.page_zoom.remove(&node);
    }

    /// Begin one actor-backed request and return the exact older request it
    /// supersedes, if any. Network phase is independent of session lifecycle.
    pub fn begin_fetch(
        &mut self,
        node: Uuid,
        request: fetch::FetchRequestId,
    ) -> Option<fetch::FetchRequestId> {
        self.stream_bytes.remove(&node);
        self.fetch_phases.insert(node, PageFetchPhase::Requested);
        self.active_fetches.insert(node, request)
    }

    pub fn active_fetch(&self, node: Uuid) -> Option<fetch::FetchRequestId> {
        self.active_fetches.get(&node).copied()
    }

    pub fn is_active_fetch(&self, node: Uuid, request: fetch::FetchRequestId) -> bool {
        self.active_fetch(node) == Some(request)
    }

    pub fn finish_fetch(&mut self, node: Uuid, request: fetch::FetchRequestId) -> bool {
        if !self.is_active_fetch(node, request) {
            return false;
        }
        self.active_fetches.remove(&node);
        true
    }

    pub fn settle_fetch(&mut self, node: Uuid, request: fetch::FetchRequestId) -> bool {
        if !self.finish_fetch(node, request) {
            return false;
        }
        let received_bytes = match self.fetch_phases.get(&node) {
            Some(PageFetchPhase::Streaming { received_bytes, .. }) => *received_bytes,
            Some(PageFetchPhase::Settled { received_bytes })
            | Some(PageFetchPhase::Stopped { received_bytes }) => *received_bytes,
            _ => 0,
        };
        self.fetch_phases
            .insert(node, PageFetchPhase::Settled { received_bytes });
        true
    }

    pub fn stop_fetch(&mut self, node: Uuid, request: fetch::FetchRequestId) -> bool {
        if !self.finish_fetch(node, request) {
            return false;
        }
        let received_bytes = match self.fetch_phases.get(&node) {
            Some(PageFetchPhase::Streaming { received_bytes, .. }) => *received_bytes,
            Some(PageFetchPhase::Settled { received_bytes })
            | Some(PageFetchPhase::Stopped { received_bytes }) => *received_bytes,
            _ => 0,
        };
        self.stream_bytes.remove(&node);
        self.fetch_phases
            .insert(node, PageFetchPhase::Stopped { received_bytes });
        true
    }

    pub fn note_surface_started(&mut self, node: Uuid) {
        self.fetch_phases.insert(
            node,
            PageFetchPhase::Loading {
                progress_millis: None,
            },
        );
    }

    pub fn note_surface_progress(&mut self, node: Uuid, value: f32) {
        let progress_millis = (value.clamp(0.0, 1.0) * 1000.0).round() as u16;
        self.fetch_phases.insert(
            node,
            PageFetchPhase::Loading {
                progress_millis: Some(progress_millis),
            },
        );
    }

    pub fn note_surface_settled(&mut self, node: Uuid) {
        self.fetch_phases
            .insert(node, PageFetchPhase::Settled { received_bytes: 0 });
    }

    pub fn note_surface_stopped(&mut self, node: Uuid) {
        self.fetch_phases
            .insert(node, PageFetchPhase::Stopped { received_bytes: 0 });
    }

    pub fn note_live(&mut self, node: Uuid, facts: Option<ContentFacts>) {
        self.states.insert(node, NodeContent::Live);
        match facts {
            Some(facts) => {
                self.facts.insert(node, facts);
            }
            None => {
                self.facts.remove(&node);
            }
        }
    }

    pub fn note_awaiting_input(&mut self, node: Uuid) {
        self.states.insert(node, NodeContent::AwaitingInput);
        self.facts.remove(&node);
        self.page_zoom.remove(&node);
    }

    pub fn note_awaiting_identity(&mut self, node: Uuid) {
        self.states.insert(node, NodeContent::AwaitingIdentity);
        self.facts.remove(&node);
        self.page_zoom.remove(&node);
    }

    pub fn note_awaiting_trust(&mut self, node: Uuid) {
        self.states.insert(node, NodeContent::AwaitingTrust);
        self.facts.remove(&node);
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
        self.documents.insert(node, (url, document));
        self.stream_bytes.remove(&node);
        self.fetch_phases
            .insert(node, PageFetchPhase::Settled { received_bytes });
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
        let entry = self
            .stream_bytes
            .entry(node)
            .or_insert_with(|| (url.clone(), Vec::new()));
        if entry.0 != url {
            *entry = (url.clone(), Vec::new());
        }
        entry.1.extend_from_slice(chunk);
        let received_bytes = entry.1.len();
        self.documents.insert(
            node,
            (
                url,
                FetchedDocument {
                    content_type: content_type.clone(),
                    body: String::from_utf8_lossy(&entry.1).into_owned(),
                },
            ),
        );
        self.fetch_phases.insert(
            node,
            PageFetchPhase::Streaming {
                response_url,
                content_type,
                received_bytes,
            },
        );
        received_bytes
    }

    pub fn fetch_phase(&self, node: Uuid) -> Option<&PageFetchPhase> {
        self.fetch_phases.get(&node)
    }

    pub fn fetch_in_progress(&self, node: Uuid) -> bool {
        self.active_fetches.contains_key(&node)
            || matches!(
                self.fetch_phases.get(&node),
                Some(PageFetchPhase::Loading { .. })
            )
    }

    pub fn fetched(&self, node: Uuid, url: &str) -> Option<&FetchedDocument> {
        self.documents
            .get(&node)
            .and_then(|(owner, document)| (owner == url).then_some(document))
    }

    pub fn forget_fetched(&mut self, node: Uuid) {
        self.documents.remove(&node);
        self.stream_bytes.remove(&node);
        self.fetch_phases.remove(&node);
    }

    /// Remove every transient fact owned by a node that has left the graph.
    pub fn forget_node(&mut self, node: Uuid) {
        self.note_closed(node);
        self.documents.remove(&node);
        self.stream_bytes.remove(&node);
        self.fetch_phases.remove(&node);
        self.active_fetches.remove(&node);
    }

    pub fn note_failed(&mut self, node: Uuid, error: String) {
        self.states.insert(node, NodeContent::Failed(error));
        self.facts.remove(&node);
        self.page_zoom.remove(&node);
        self.stream_bytes.remove(&node);
        self.fetch_phases.remove(&node);
        self.active_fetches.remove(&node);
    }

    /// The node's content is gone (closed, or the port dropped it).
    pub fn note_closed(&mut self, node: Uuid) {
        self.states.remove(&node);
        self.facts.remove(&node);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flip_toggles_and_failure_retries() {
        let node = Uuid::new_v4();
        let mut states = ContentStates::default();
        assert!(states.flip_spawns(node), "empty flips ON");
        states.note_requested(node);
        assert!(
            !states.flip_spawns(node),
            "in-flight flips OFF, not double-spawns"
        );
        states.note_live(node, None);
        assert!(!states.flip_spawns(node), "live flips OFF");
        states.note_closed(node);
        assert!(states.flip_spawns(node), "closed flips ON again");
        states.note_awaiting_input(node);
        assert!(
            !states.flip_spawns(node),
            "an input conversation toggles OFF"
        );
        states.note_closed(node);
        states.note_requested(node);
        states.note_failed(node, "no port".into());
        assert!(states.flip_spawns(node), "failed retries on the next flip");
    }

    #[test]
    fn live_nodes_lists_only_live() {
        let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let mut states = ContentStates::default();
        states.note_live(a, None);
        states.note_requested(b);
        states.note_failed(c, "x".into());
        let live: Vec<_> = states.live_nodes().collect();
        assert_eq!(live, vec![a]);
    }

    #[test]
    fn fetched_documents_are_member_and_address_scoped() {
        let node = Uuid::new_v4();
        let document = FetchedDocument {
            content_type: Some("text/gemini".into()),
            body: "# Capsule".into(),
        };
        let mut states = ContentStates::default();
        states.note_fetched(node, "gemini://example/one".into(), document.clone(), 9);
        assert_eq!(
            states.fetched(node, "gemini://example/one"),
            Some(&document)
        );
        assert!(states.fetched(node, "gemini://example/two").is_none());
    }

    #[test]
    fn streamed_documents_accumulate_exact_bytes_before_decoding() {
        let node = Uuid::new_v4();
        let mut states = ContentStates::default();
        states.note_requested(node);
        states.note_streamed(
            node,
            "gemini://example/live".into(),
            "gemini://example/live".into(),
            Some("text/gemini".into()),
            &[b'#', b' ', 0xc3],
        );
        let received = states.note_streamed(
            node,
            "gemini://example/live".into(),
            "gemini://example/live".into(),
            Some("text/gemini".into()),
            &[0xa9, b'\n'],
        );
        assert_eq!(received, 5);
        assert_eq!(
            states
                .fetched(node, "gemini://example/live")
                .map(|document| document.body.as_str()),
            Some("# é\n")
        );
        assert!(matches!(
            states.fetch_phase(node),
            Some(PageFetchPhase::Streaming {
                received_bytes: 5,
                ..
            })
        ));
    }
}
