//! The behavior drain: turning committed deltas into wakes, and running them.
//!
//! `servitor::watch` decides *who* wakes and `servitor::cascade` decides *how
//! far* a wake travels. Neither knows what a mere graph is. This module is the
//! adapter between them and this application: it reads the journal tail, gives
//! each entry the scopes it touched, and runs the woken bodies through the
//! ordinary `RunDenizen` lane.
//!
//! **A node's scope is its containment ancestry** (the region vocabulary ruled
//! 2026-08-13). Mere's nodes are keyed by `Uuid`, which is one opaque segment
//! and so cannot nest on its own, but `EdgeFamily::Containment` already relates
//! them: a node under a folder, a URL path, a domain, a collection. Writing
//! that ancestry as a `ScopePath` of ids (`container/member`) makes
//! segment-prefix coverage mean what it should, with no change to `Cap`.
//!
//! Three properties of that walk, each of which surprised the reading:
//!
//! - **Containment points from the member to the container.** The kernel
//!   asserts `assert_relation(child, parent, Containment)` (see
//!   `rebuild_derived_containment`), so ancestry follows *outgoing* edges.
//!   Walking incoming ones would build the tree upside down and quietly invert
//!   every watch.
//! - **A node has several ancestries, not one.** The same node is related to
//!   both its URL-path parent and its domain anchor, so it belongs to more than
//!   one region at once. Each is its own path, which is why a `WatchEvent`
//!   carries a slice of scopes.
//! - **A container is addressed in directory form.** The kernel's rule names a
//!   parent as `https://host/inbox/`, with the trailing slash, so a folder node
//!   stored as `https://host/inbox` is a *different address* and nothing is
//!   ever contained by it. A watch on the slashless form is inert and says
//!   nothing about why, which is what it cost to find. First thing to check
//!   when a watch looks asleep.
//! - **A removed node has no ancestry left to walk.** Its containment edges
//!   went with it, so it falls back to its bare id: one segment, matching an
//!   exact-node watch or the root and nothing else. Stated rather than hidden,
//!   because "delete stopped waking the folder's watcher" is otherwise a
//!   mystery.

use std::collections::HashSet;

use mere::kernel::graph::Graph;
use mere::kernel::graph::capture::CapturedDelta;
use servitor::cascade::{Cascade, CascadeBudget, CascadeOutcome, CommittedEntry, run_cascade};
use servitor::{ScopePath, Subject};

use crate::action::Effect;
use crate::app::App;
use crate::observe::AppEvent;

/// What woke a body, handed to it as its run's context.
///
/// A digest of the matched entries rather than the deltas themselves: a
/// behavior needs to know *which nodes moved under its watch*, and handing it
/// the raw delta vocabulary would couple every body to the kernel's 44
/// variants and to their evolution. Nodes and attribution are the durable
/// part of the answer.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct TriggerContext {
    /// The entries that matched this body's watch, in journal order.
    pub woken_by: Vec<TriggerEntry>,
}

/// One matched entry, as a body sees it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct TriggerEntry {
    /// Journal position.
    pub seq: u64,
    /// Who committed it. A body can tell a user's edit from another
    /// behavior's, which is what makes "answer people, ignore machines"
    /// expressible.
    pub author: String,
    /// The node ids the entry touched.
    pub nodes: Vec<String>,
}

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

/// How deep a containment walk may go before it stops looking.
///
/// Containment is asserted per relation and nothing forbids a cycle, so the
/// walk carries a visited set; this bound is the second belt, against a chain
/// so long that building its scope costs more than the match is worth.
const MAX_ANCESTRY_DEPTH: usize = 32;

/// The node ids a delta touches.
///
/// Exhaustive by construction: a new `CapturedDelta` variant fails to compile
/// here until it is classified, which is the same discipline `ring_of` uses
/// for actions. A variant that silently touched nothing would be a behavior
/// that silently stopped waking.
pub fn touched_ids(delta: &CapturedDelta) -> Vec<&str> {
    use CapturedDelta as D;
    match delta {
        D::ReplayAddNodeWithIdIfMissing { id, .. } => vec![id.as_str()],

        D::ReplayRemoveNodeById { node_id, .. }
        | D::ReplaySetNodeTitleById { node_id, .. }
        | D::ReplaySetNodeUrlById { node_id, .. }
        | D::ReplaySetNodeImageById { node_id, .. }
        | D::ReplaySetNodeThumbnailById { node_id, .. }
        | D::ReplaySetNodeFaviconById { node_id, .. }
        | D::ReplaySetNodeMimeHintById { node_id, .. }
        | D::ReplaySetNodeNestedById { node_id, .. }
        | D::ReplaySetNodePinnedById { node_id, .. }
        | D::ReplaySetNodeFacetById { node_id, .. }
        | D::ReplayRemoveNodeFacetById { node_id, .. }
        | D::ReplayInsertNodeTagById { node_id, .. }
        | D::ReplayRemoveNodeTagById { node_id, .. }
        | D::ReplaySetNodeBodyById { node_id, .. }
        | D::ReplayNavigateNodeById { node_id, .. }
        | D::ReplayNodeHistoryBackById { node_id, .. }
        | D::ReplayNodeHistoryForwardById { node_id, .. }
        | D::ReplayAppendNodePropertyById { node_id, .. }
        | D::ReplayAddNodeClassificationById { node_id, .. }
        | D::ReplayRemoveNodeClassificationById { node_id, .. }
        | D::ReplaySetNodeClassificationStatusById { node_id, .. }
        | D::ReplaySetNodePrimaryClassificationById { node_id, .. }
        | D::ReplayRecordNodeDerivationById { node_id, .. }
        | D::ReplaySetNodeTagIconOverrideById { node_id, .. }
        | D::ReplayAppendFrameLayoutHintById { node_id, .. }
        | D::ReplayRemoveFrameLayoutHintById { node_id, .. }
        | D::ReplayMoveFrameLayoutHintById { node_id, .. }
        | D::ReplaySetFrameSplitOfferSuppressedById { node_id, .. }
        | D::ReplayUpdateNodeHistoryById { node_id, .. }
        | D::ReplayTouchNodeLastVisitedById { node_id, .. } => vec![node_id.as_str()],

        D::ReplayAssertRelationByIds { from_id, to_id, .. }
        | D::ReplayRetractRelationsByIds { from_id, to_id, .. }
        | D::ReplayAppendTraversalByIds { from_id, to_id, .. }
        | D::ReplaySetEdgeSemanticPredicateByIds { from_id, to_id, .. }
        | D::ReplayAssertSemanticPredicateByIds { from_id, to_id, .. } => {
            vec![from_id.as_str(), to_id.as_str()]
        }

        D::ReplayBranchHistoryByIds {
            child_id,
            parent_id,
        } => vec![child_id.as_str(), parent_id.as_str()],

        // Session-level and field/coupling deltas name no node. The physics
        // tier is deliberately outside this vocabulary: spatial influence
        // reaches the graph through fields, never through a petition, so a
        // field change is not a thing a behavior is woken by.
        D::ReplaySetImportRecords { .. }
        | D::ReplayAddField { .. }
        | D::ReplayRetireFieldById { .. }
        | D::ReplayAddCoupling { .. }
        | D::ReplaySetFieldCouplingStrengthByFieldId { .. }
        | D::ReplayActivateFieldById { .. }
        | D::ReplayRetractCouplingById { .. } => Vec::new(),
    }
}

/// Every containment ancestry of `id`, each as a root-first scope path ending
/// in the node itself.
///
/// A node with no container is its own single-segment scope, which is also
/// what a node the graph no longer holds falls back to.
pub fn ancestry_scopes(graph: &Graph, id: &str) -> Vec<ScopePath> {
    let Ok(uuid) = id.parse::<uuid::Uuid>() else {
        return Vec::new();
    };
    let Some(key) = graph.get_node_key_by_id(uuid) else {
        // Removed, or never present: the bare id is all there is to say.
        return ScopePath::parse(id).into_iter().collect();
    };

    // Containment runs member -> container, so a node's containers are its
    // outgoing containment targets.
    let mut paths: Vec<Vec<String>> = Vec::new();
    let mut frontier: Vec<(Vec<String>, mere::kernel::graph::NodeKey, HashSet<String>)> = {
        let mut seen = HashSet::new();
        seen.insert(id.to_string());
        vec![(vec![id.to_string()], key, seen)]
    };

    while let Some((path, at, seen)) = frontier.pop() {
        let containers: Vec<_> = graph
            .containment_edges()
            .filter(|edge| edge.from == at)
            .map(|edge| edge.to)
            .collect();
        let mut grew = false;
        for container in containers {
            let Some(node) = graph.get_node(container) else {
                continue;
            };
            let container_id = node.id.to_string();
            if seen.contains(&container_id) || path.len() >= MAX_ANCESTRY_DEPTH {
                // A cycle, or deeper than a scope is worth. The path so far is
                // still a real region, so it is kept rather than dropped.
                continue;
            }
            let mut next = path.clone();
            next.push(container_id.clone());
            let mut seen = seen.clone();
            seen.insert(container_id);
            frontier.push((next, container, seen));
            grew = true;
        }
        if !grew {
            paths.push(path);
        }
    }

    // Built leaf-first while walking; a scope reads outermost-first.
    paths
        .into_iter()
        .filter_map(|mut path| {
            path.reverse();
            ScopePath::parse(&path.join("/")).ok()
        })
        .collect()
}

/// The journal entries after `cursor`, as cascade inputs.
pub fn entries_since(app: &App, cursor: u64) -> Vec<CommittedEntry> {
    let journal = match app.journal.lock() {
        Ok(journal) => journal,
        Err(poisoned) => poisoned.into_inner(),
    };
    let graph = app.graph_runtimes.graph();
    journal
        .entries()
        .iter()
        .enumerate()
        .map(|(index, entry)| (index as u64 + 1, entry))
        .filter(|(seq, _)| *seq > cursor)
        .map(|(seq, entry)| {
            let scopes = touched_ids(&entry.delta)
                .into_iter()
                .flat_map(|id| ancestry_scopes(graph, id))
                .collect();
            CommittedEntry::new(seq, entry.author.clone(), scopes)
        })
        .collect()
}

/// Run the behavior cascade for whatever has been committed since last time.
///
/// The after-dispatch drain: called once per action, after the action's own
/// effects are decided, so a woken body sees the world the action left rather
/// than the one it found.
pub fn drain(app: &mut App) -> Vec<Effect> {
    if app.denizens.is_empty() || app.watches.is_empty() {
        return Vec::new();
    }
    let entries = entries_since(app, app.behavior_cursor);
    if entries.is_empty() {
        return Vec::new();
    }
    app.behavior_cursor = entries
        .iter()
        .map(|entry| entry.seq)
        .max()
        .unwrap_or_default();

    let budget = CascadeBudget::new(app.cascade_budget);
    let mut effects: Vec<Effect> = Vec::new();
    let mut watches = std::mem::take(&mut app.watches);
    let mut round_entries: Vec<CommittedEntry> = entries.clone();
    let cascade = run_cascade(&mut watches, budget, entries, |wakes| {
        let mut produced = Vec::new();
        for wake in wakes {
            let Some(member) = member_of(app, wake.subject) else {
                continue;
            };
            let context = context_for(&round_entries, wake);
            let before = journal_len(app);
            effects.extend(app.run_denizen_for_cascade(member, &context));
            produced.extend(entries_since(app, before));
        }
        // What the round's bodies committed becomes the next round's input,
        // and the next round's digest.
        if let Some(highest) = produced.iter().map(|entry| entry.seq).max() {
            app.behavior_cursor = app.behavior_cursor.max(highest);
        }
        round_entries = produced.clone();
        produced
    });
    app.watches = watches;

    report(app, &cascade);
    effects
}

/// The digest of what woke one body: the entries its wake named, in order.
pub fn context_for(entries: &[CommittedEntry], wake: &servitor::Wake) -> TriggerContext {
    let woken_by = wake
        .matched
        .iter()
        .filter_map(|seq| entries.iter().find(|entry| entry.seq == *seq))
        .map(|entry| TriggerEntry {
            seq: entry.seq,
            author: entry.author.clone(),
            // The scope's last segment is the node itself: ancestry is written
            // outermost-first, so the tail is what actually changed.
            nodes: entry
                .scopes
                .iter()
                .filter_map(|scope| scope.segments().last().cloned())
                .collect(),
        })
        .collect();
    TriggerContext { woken_by }
}

/// Which resident node holds `subject`.
fn member_of(app: &App, subject: Subject) -> Option<uuid::Uuid> {
    app.denizens
        .residents
        .iter()
        .find(|(_, resident)| resident.subject == subject)
        .map(|(member, _)| *member)
}

fn journal_len(app: &App) -> u64 {
    match app.journal.lock() {
        Ok(journal) => journal.entries().len() as u64,
        Err(poisoned) => poisoned.into_inner().entries().len() as u64,
    }
}

/// Say what the cascade did, loudly when it hit the budget.
fn report(app: &mut App, cascade: &Cascade) {
    if let CascadeOutcome::BudgetExhausted { still_waking } = &cascade.outcome {
        let names: Vec<String> = still_waking
            .iter()
            .filter_map(|subject| member_of(app, *subject))
            .filter_map(|member| {
                app.denizens
                    .residents
                    .get(&member)
                    .map(|resident| resident.label.clone())
            })
            .collect();
        let named = if names.is_empty() {
            "unnamed behaviors".to_string()
        } else {
            names.join(", ")
        };
        tracing::warn!(
            rounds = cascade.rounds.len(),
            %named,
            "behavior cascade hit its budget"
        );
        app.record_event(AppEvent::CascadeExhausted(named));
    } else if !cascade.rounds.is_empty() {
        tracing::debug!(rounds = cascade.rounds.len(), "behavior cascade settled");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_delta_naming_one_node_yields_that_node() {
        let delta = CapturedDelta::ReplaySetNodeTitleById {
            node_id: "n1".into(),
            title: "t".into(),
        };
        assert_eq!(touched_ids(&delta), vec!["n1"]);
    }

    #[test]
    fn a_relation_delta_yields_both_ends() {
        let delta = CapturedDelta::ReplayBranchHistoryByIds {
            child_id: "child".into(),
            parent_id: "parent".into(),
        };
        assert_eq!(touched_ids(&delta), vec!["child", "parent"]);
    }

    #[test]
    fn a_field_delta_names_no_node() {
        // The physics tier is outside the behavior vocabulary on purpose.
        let delta = CapturedDelta::ReplaySetImportRecords {
            import_records: Vec::new(),
        };
        assert!(touched_ids(&delta).is_empty());
    }

    #[test]
    fn a_node_the_graph_does_not_hold_falls_back_to_its_bare_id() {
        let graph = Graph::new();
        let id = uuid::Uuid::new_v4().to_string();
        let scopes = ancestry_scopes(&graph, &id);
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].segments(), &[id]);
    }

    #[test]
    fn a_digest_names_the_nodes_that_changed_not_their_containers() {
        // Ancestry is written outermost-first, so the tail of each scope is
        // the node that actually moved. A body wants that, not the folder.
        let entries = vec![
            CommittedEntry::new(4, "user", vec![ScopePath::parse("folder/leaf").unwrap()]),
            CommittedEntry::new(5, "user", vec![ScopePath::parse("elsewhere").unwrap()]),
        ];
        let wake = servitor::Wake {
            subject: Subject::new([1; 32]),
            matched: vec![4],
        };
        let context = context_for(&entries, &wake);
        assert_eq!(context.woken_by.len(), 1, "only the matched entry");
        assert_eq!(context.woken_by[0].seq, 4);
        assert_eq!(context.woken_by[0].nodes, vec!["leaf".to_string()]);
        assert_eq!(context.woken_by[0].author, "user");
    }

    #[test]
    fn an_unwoken_context_is_empty_rather_than_absent() {
        let context = TriggerContext::default();
        assert!(context.is_empty());
        assert_eq!(context.to_json(), r#"{"woken_by":[]}"#);
    }

    #[test]
    fn a_malformed_id_yields_no_scope_rather_than_a_bogus_one() {
        let graph = Graph::new();
        assert!(ancestry_scopes(&graph, "not-a-uuid").is_empty());
    }
}
