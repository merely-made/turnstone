//! The content lane's app truth (rung 4, born ahead of its port): per-node
//! document lifecycle only. Charter per the architecture plan's module map:
//! engine registrations, per-node document lifecycle, the verso-tile flip,
//! content frames, and input routing — where the registry itself is
//! genet/inker's, never a hand-wired lane ladder (the session-engines plan,
//! genet docs 2026-07-10, phase 4 names turnstone as its consumer).
//!
//! What lives HERE is the lifecycle state machine and nothing else. Live
//! document sessions are retained, non-`Send` handles, so the shell's
//! content port owns them (ports are the only owners of handles; `App`
//! holds data) keyed by the same node ids. Until the genet-documents
//! component lands (engines plan phase 2), the shell port answers every
//! spawn with an honest failure naming the gap — a `Requested` node never
//! silently spins (the no-placebo rule).

use std::collections::HashMap;

use uuid::Uuid;

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
    /// The last spawn failed; the reason is surfaced, never swallowed.
    Failed(String),
}

/// App-owned facts about a live session, mirrored out of the content port at
/// spawn (the adapter converts the service's report type at the boundary, so
/// this module stays port-agnostic). Data, not a handle: the Inspector pane
/// and the observation snapshot read these without reaching into the shell.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentFacts {
    /// The engine id the route decision picked (e.g. `genet.web`).
    pub engine: String,
    /// The structural read, when the lane has one. `None` is reported
    /// honestly (a lane without introspection, not an empty document).
    pub structure: Option<StructureFacts>,
}

/// One page body fetched by the shell actor and ready to hand to a document
/// engine. This transient representation prevents a live session from issuing
/// a second network request for bytes the host already owns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchedDocument {
    pub content_type: Option<String>,
    pub body: String,
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
    documents: HashMap<Uuid, (String, FetchedDocument)>,
}

impl ContentStates {
    pub fn get(&self, node: Uuid) -> Option<&NodeContent> {
        self.states.get(&node)
    }

    /// The mirrored facts for a live node (absent for requested/failed/none).
    pub fn facts(&self, node: Uuid) -> Option<&ContentFacts> {
        self.facts.get(&node)
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
            )
        )
    }

    pub fn note_requested(&mut self, node: Uuid) {
        self.states.insert(node, NodeContent::Requested);
        self.facts.remove(&node);
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
    }

    pub fn note_awaiting_identity(&mut self, node: Uuid) {
        self.states.insert(node, NodeContent::AwaitingIdentity);
        self.facts.remove(&node);
    }

    /// Retain the actor-owned response under both member and address. The URL
    /// guard makes a late or superseded body unusable for a later navigation.
    pub fn note_fetched(&mut self, node: Uuid, url: String, document: FetchedDocument) {
        self.documents.insert(node, (url, document));
    }

    pub fn fetched(&self, node: Uuid, url: &str) -> Option<&FetchedDocument> {
        self.documents
            .get(&node)
            .and_then(|(owner, document)| (owner == url).then_some(document))
    }

    pub fn forget_fetched(&mut self, node: Uuid) {
        self.documents.remove(&node);
    }

    /// Remove every transient fact owned by a node that has left the graph.
    pub fn forget_node(&mut self, node: Uuid) {
        self.note_closed(node);
        self.documents.remove(&node);
    }

    pub fn note_failed(&mut self, node: Uuid, error: String) {
        self.states.insert(node, NodeContent::Failed(error));
        self.facts.remove(&node);
    }

    /// The node's content is gone (closed, or the port dropped it).
    pub fn note_closed(&mut self, node: Uuid) {
        self.states.remove(&node);
        self.facts.remove(&node);
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
        states.note_fetched(node, "gemini://example/one".into(), document.clone());
        assert_eq!(
            states.fetched(node, "gemini://example/one"),
            Some(&document)
        );
        assert!(states.fetched(node, "gemini://example/two").is_none());
    }
}
