// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Retained foreign trees around Mere's validated AccessKit graft composition.
//! Supplier TreeIds and local NodeIds stay intact. Missing declared subtrees
//! wait for their initial update; malformed known trees withdraw their surface.

use accesskit::{ActionRequest, Node, NodeId, Rect, Role, Tree, TreeId, TreeUpdate};
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;
use uxtree::graft::{ActionTarget, Composition, GraftTable, Grafts, graft_node};

/// Check activation at the platform-serialized application boundary. Call this
/// inside each `update_if_active` closure, never while preparing the batch.
/// A replacement consumer receives safe ROOT; the caller must abort the old
/// batch, reset publication and schedule complete parent-first replay.
pub(crate) fn activation_safe_update(
    replay: &std::sync::atomic::AtomicBool,
    update: TreeUpdate,
    safe_host: &TreeUpdate,
) -> (TreeUpdate, bool) {
    if replay.swap(false, std::sync::atomic::Ordering::AcqRel) {
        (safe_host.clone(), true)
    } else {
        (update, false)
    }
}

#[derive(Clone)]
struct RetainedTree {
    tree: Tree,
    nodes: BTreeMap<NodeId, Node>,
    focus: NodeId,
    publication: Grafts,
}

enum MaterializationError {
    Incomplete(String),
    Invalid(String),
}

impl std::fmt::Display for MaterializationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Incomplete(message) | Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl RetainedTree {
    fn update(previous: Option<&Self>, update: TreeUpdate) -> Result<Self, MaterializationError> {
        let mut retained = match previous {
            Some(previous) => previous.clone(),
            None => Self {
                tree: update.tree.clone().ok_or_else(|| {
                    MaterializationError::Incomplete(
                        "initial foreign update has no tree data".into(),
                    )
                })?,
                nodes: BTreeMap::new(),
                focus: update.focus,
                publication: Grafts::new(),
            },
        };
        if let Some(tree) = update.tree {
            retained.tree = tree;
        }
        retained.focus = update.focus;
        let mut changed = HashSet::new();
        for (id, node) in update.nodes {
            if !changed.insert(id) {
                return Err(MaterializationError::Invalid(format!(
                    "duplicate local node {id:?}"
                )));
            }
            retained.nodes.insert(id, node);
        }
        let mut live = HashSet::new();
        let mut pending = vec![retained.tree.root];
        while let Some(id) = pending.pop() {
            if !live.insert(id) {
                return Err(MaterializationError::Invalid(
                    "local node graph has a cycle or multiple parents".into(),
                ));
            }
            let node = retained.nodes.get(&id).ok_or_else(|| {
                MaterializationError::Incomplete(format!("missing local child {id:?}"))
            })?;
            if node.tree_id().is_some() && !node.children().is_empty() {
                return Err(MaterializationError::Invalid(
                    "foreign graft node also has local children".into(),
                ));
            }
            pending.extend(node.children().iter().rev().copied());
        }
        if retained.nodes[&retained.tree.root].tree_id().is_some() {
            return Err(MaterializationError::Invalid(
                "foreign tree root cannot itself be a graft".into(),
            ));
        }
        if !live.contains(&retained.focus) {
            return Err(MaterializationError::Incomplete(
                "foreign focus is absent or unreachable".into(),
            ));
        }
        if changed.iter().any(|id| !live.contains(id)) {
            return Err(MaterializationError::Invalid(
                "foreign update contains unreachable changed nodes".into(),
            ));
        }
        // Nodes omitted after their parent's child-list change are retired,
        // including their action masks. They are never retained as targets.
        retained.nodes.retain(|id, _| live.contains(id));
        Ok(retained)
    }

    fn children(&self) -> Vec<TreeId> {
        self.nodes.values().filter_map(Node::tree_id).collect()
    }

    fn snapshot(&self, id: TreeId) -> TreeUpdate {
        TreeUpdate {
            nodes: self
                .nodes
                .iter()
                .map(|(id, node)| (*id, node.clone()))
                .collect(),
            tree: Some(self.tree.clone()),
            tree_id: id,
            focus: self.focus,
        }
    }
}

#[derive(Clone)]
struct SurfaceForest {
    root: TreeId,
    trees: HashMap<TreeId, RetainedTree>,
    declared: HashSet<TreeId>,
}

impl SurfaceForest {
    /// Validate ownership edges, including as-yet missing declared children.
    fn reachable(&self) -> Result<HashSet<TreeId>, String> {
        let mut found = HashSet::new();
        let mut pending = vec![self.root];
        while let Some(id) = pending.pop() {
            if id == TreeId::ROOT || !found.insert(id) {
                return Err(
                    "foreign tree graph has a root collision, cycle or multiple parents".into(),
                );
            }
            if let Some(tree) = self.trees.get(&id) {
                pending.extend(tree.children().into_iter().rev());
            }
        }
        Ok(found)
    }

    fn ready(&self) -> bool {
        self.declared.iter().all(|id| self.trees.contains_key(id))
    }

    fn reset_publication(&mut self) {
        for tree in self.trees.values_mut() {
            tree.publication = Grafts::new();
        }
    }

    fn prepare(&mut self, id: TreeId) -> Result<Vec<TreeUpdate>, String> {
        let tree = self.trees.get(&id).ok_or("foreign forest is incomplete")?;
        let snapshot = tree.snapshot(id);
        let children = tree.children();
        let mut frames = HashMap::new();
        let mut guests = Vec::new();
        for child in children {
            let frame = self.prepare(child)?;
            guests.push((child, frame[0].clone()));
            frames.insert(child, frame);
        }
        let composition = Composition::new(snapshot, guests).map_err(|error| error.to_string())?;
        let updates = self
            .trees
            .get_mut(&id)
            .unwrap()
            .publication
            .frame(composition);
        // Each child placeholder is its safe first parent update. Replace it
        // with the child's complete frame, so descendants and their focus are
        // installed before an ancestor's final focus-only move reaches them.
        Ok(expand(updates, &frames))
    }
}

fn expand(
    updates: Vec<TreeUpdate>,
    children: &HashMap<TreeId, Vec<TreeUpdate>>,
) -> Vec<TreeUpdate> {
    let mut result = Vec::new();
    for update in updates {
        match children.get(&update.tree_id) {
            Some(frame) => result.extend(frame.iter().cloned()),
            None => result.push(update),
        }
    }
    result
}

pub(crate) struct ForeignA11y {
    surfaces: HashMap<Uuid, SurfaceForest>,
    owners: GraftTable<Uuid>,
    publication: Grafts,
    mounted: HashSet<Uuid>,
    dirty: bool,
    // Exact full-node snapshots emitted by the last successful composition.
    // This is an in-process publication witness, not platform activation or
    // assistive-technology acceptance. Pending supplier deltas do not alter it.
    published: HashMap<Uuid, PublishedSurface>,
}

struct PublishedSurface {
    root: TreeId,
    trees: BTreeMap<TreeId, TreeUpdate>,
    ordered_updates: Vec<TreeUpdate>,
}

impl Default for ForeignA11y {
    fn default() -> Self {
        Self {
            surfaces: HashMap::new(),
            owners: GraftTable::new(TreeId::ROOT),
            publication: Grafts::new(),
            mounted: HashSet::new(),
            dirty: false,
            published: HashMap::new(),
        }
    }
}

impl ForeignA11y {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn admit(&mut self, surface: Uuid, root: TreeId) -> Result<(), String> {
        if root == TreeId::ROOT || self.owners.get(root).is_some_and(|owner| *owner != surface) {
            return Err("foreign root is owned by the host or another surface".into());
        }
        if let Some(current) = self.surfaces.get(&surface) {
            if current.root == root {
                return Ok(());
            }
            // A root cannot be promoted from its own old nested namespace.
            if current.declared.contains(&root) {
                return Err("replacement root is already an old nested tree".into());
            }
        }
        self.retire(surface);
        self.owners.insert(root, surface);
        self.surfaces.insert(
            surface,
            SurfaceForest {
                root,
                trees: HashMap::new(),
                declared: HashSet::from([root]),
            },
        );
        self.dirty = true;
        Ok(())
    }

    pub(crate) fn retire(&mut self, surface: Uuid) {
        self.published.remove(&surface);
        if let Some(forest) = self.surfaces.remove(&surface) {
            for id in forest.declared {
                self.owners.remove(id);
            }
            self.mounted.remove(&surface);
            self.dirty = true;
        }
    }

    pub(crate) fn clear(&mut self) {
        self.published.clear();
        self.surfaces.clear();
        self.owners = GraftTable::new(TreeId::ROOT);
        self.mounted.clear();
        self.dirty = true;
    }

    pub(crate) fn apply(&mut self, surface: Uuid, update: TreeUpdate) -> Result<(), String> {
        let forest = self
            .surfaces
            .get(&surface)
            .ok_or("foreign surface is not admitted")?;
        // Unknown or stale callbacks cannot damage a current valid forest.
        if !forest.declared.contains(&update.tree_id) {
            return Err("foreign update names an unreferenced or stale tree".into());
        }
        let mut candidate = forest.clone();
        let update_id = update.tree_id;
        let tree = match RetainedTree::update(candidate.trees.get(&update_id), update) {
            Ok(tree) => tree,
            Err(error @ MaterializationError::Incomplete(_))
                if !candidate.trees.contains_key(&update_id) =>
            {
                // A late delta may be re-grafted under an upstream wrapper
                // after reactivation. None or Some(tree) does not prove a
                // complete initial graph. Wait without retaining those nodes
                // or admitting publication/actions until a valid full update.
                // This cannot distinguish a stale full callback from a fresh
                // one when the supplier reuses its document TreeId.
                return Err(error.to_string());
            },
            Err(error) => {
                self.retire(surface);
                return Err(error.to_string());
            },
        };
        let result = (|| {
            candidate.trees.insert(update_id, tree);
            let declared = candidate.reachable()?;
            if declared
                .iter()
                .any(|id| self.owners.get(*id).is_some_and(|owner| *owner != surface))
            {
                return Err("foreign tree is already owned by another surface".into());
            }
            candidate.trees.retain(|id, _| declared.contains(id));
            candidate.declared = declared;
            Ok(())
        })();
        if let Err(error) = result {
            // Malformed known-tree updates withdraw actions and publication.
            // The caller can explicitly resync once using a fresh root.
            self.retire(surface);
            return Err(error);
        }
        for old in &forest.declared {
            if !candidate.declared.contains(old) {
                self.owners.remove(*old);
            }
        }
        for id in &candidate.declared {
            self.owners.insert(*id, surface);
        }
        self.surfaces.insert(surface, candidate);
        self.dirty = true;
        Ok(())
    }

    pub(crate) fn ready(&self, surface: Uuid) -> bool {
        self.surfaces
            .get(&surface)
            .is_some_and(SurfaceForest::ready)
    }

    pub(crate) fn graft(&self, surface: Uuid, rect: Rect) -> Option<Node> {
        let forest = self.surfaces.get(&surface)?;
        if !forest.ready()
            || ![rect.x0, rect.y0, rect.x1, rect.y1]
                .into_iter()
                .all(f64::is_finite)
            || rect.x1 <= rect.x0
            || rect.y1 <= rect.y0
        {
            return None;
        }
        Some(graft_node(
            Role::GenericContainer,
            forest.root,
            (rect.x0, rect.y0),
            (rect.x1 - rect.x0, rect.y1 - rect.y0),
        ))
    }

    pub(crate) fn needs_publish(&self) -> bool {
        self.dirty
    }

    pub(crate) fn reset_publication(&mut self) {
        self.published.clear();
        self.publication = Grafts::new();
        self.mounted.clear();
        for forest in self.surfaces.values_mut() {
            forest.reset_publication();
        }
        self.dirty = true;
    }

    fn prepare(
        &self,
        host: TreeUpdate,
        visible: &[Uuid],
        initial: bool,
    ) -> Result<
        (
            Composition,
            HashMap<TreeId, Vec<TreeUpdate>>,
            HashMap<Uuid, SurfaceForest>,
        ),
        String,
    > {
        if host.tree_id != TreeId::ROOT {
            return Err("foreign composition host must use ROOT".into());
        }
        RetainedTree::update(None, host.clone()).map_err(|error| error.to_string())?;
        let mut surfaces = self.surfaces.clone();
        let mut seen = HashSet::new();
        let mut frames = HashMap::new();
        let mut guests = Vec::new();
        for surface in visible {
            if !seen.insert(*surface) {
                return Err("visible surface occurs more than once".into());
            }
            let Some(forest) = surfaces.get_mut(surface).filter(|forest| forest.ready()) else {
                continue;
            };
            if initial || !self.mounted.contains(surface) {
                forest.reset_publication();
            }
            let frame = forest.prepare(forest.root)?;
            guests.push((forest.root, frame[0].clone()));
            frames.insert(forest.root, frame);
        }
        let composition = Composition::new(host, guests).map_err(|error| error.to_string())?;
        Ok((composition, frames, surfaces))
    }

    pub(crate) fn initial_host(
        &self,
        host: TreeUpdate,
        visible: &[Uuid],
    ) -> Result<TreeUpdate, String> {
        let (composition, _, _) = self.prepare(host, visible, true)?;
        Ok(composition.initial_host())
    }

    pub(crate) fn frame(
        &mut self,
        host: TreeUpdate,
        visible: &[Uuid],
    ) -> Result<Vec<TreeUpdate>, String> {
        let (composition, frames, surfaces) = self.prepare(host, visible, false)?;
        let mut publication = self.publication.clone();
        let updates = expand(publication.frame(composition), &frames);
        self.surfaces = surfaces;
        self.publication = publication;
        self.mounted = visible
            .iter()
            .copied()
            .filter(|surface| self.ready(*surface))
            .collect();
        self.published = self
            .mounted
            .iter()
            .map(|surface| {
                let forest = &self.surfaces[surface];
                let ordered_updates: Vec<_> = updates
                    .iter()
                    .filter(|update| forest.declared.contains(&update.tree_id))
                    .cloned()
                    .collect();
                let mut trees: BTreeMap<TreeId, TreeUpdate> = BTreeMap::new();
                for update in &ordered_updates {
                    if !update.nodes.is_empty() {
                        trees.insert(update.tree_id, update.clone());
                    } else if let Some(snapshot) = trees.get_mut(&update.tree_id) {
                        snapshot.focus = update.focus;
                    }
                }
                (
                    *surface,
                    PublishedSurface {
                        root: forest.root,
                        trees,
                        ordered_updates,
                    },
                )
            })
            .collect();
        self.dirty = false;
        Ok(updates)
    }

    fn published_surfaces(&self) -> Vec<(Uuid, &PublishedSurface)> {
        let mut surfaces: Vec<_> = self
            .published
            .iter()
            .filter(|(surface, published)| {
                self.mounted.contains(*surface)
                    && self
                        .surfaces
                        .get(*surface)
                        .is_some_and(|forest| forest.ready() && forest.root == published.root)
            })
            .map(|(surface, published)| (*surface, published))
            .collect();
        surfaces.sort_by_key(|(surface, _)| *surface);
        surfaces
    }

    pub(crate) fn published_counts(&self) -> (usize, usize, usize) {
        let surfaces = self.published_surfaces();
        let trees = surfaces
            .iter()
            .map(|(_, surface)| surface.trees.len())
            .sum();
        let nodes = surfaces
            .iter()
            .flat_map(|(_, surface)| surface.trees.values())
            .map(|tree| tree.nodes.len())
            .sum();
        (surfaces.len(), trees, nodes)
    }

    /// Actual labels/values from the successful composed frame. The caller
    /// must opt into known public fixture content before observing these.
    pub(crate) fn published_text(&self) -> String {
        self.published_surfaces()
            .iter()
            .flat_map(|(_, surface)| surface.trees.values())
            .flat_map(|tree| {
                let protected = protected_nodes(tree);
                tree.nodes
                    .iter()
                    .filter(move |(id, _)| !protected.contains(id))
                    .flat_map(|(_, node)| [node.label(), node.value()])
                    .flatten()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Persist identities and topology by default; page text needs an explicit
    /// public-fixture opt-in. Password nodes and their descendants stay redacted.
    pub(crate) fn publication_diagnostic(&self, public_fixture: bool) -> serde_json::Value {
        let (surface_count, tree_count, node_count) = self.published_counts();
        let surfaces = self.published_surfaces().iter().map(|(surface, published)| {
            let trees = published.trees.values().map(|tree| {
                let protected = protected_nodes(tree);
                let nodes = tree.nodes.iter().map(|(id, node)| {
                    let text_allowed = public_fixture && !protected.contains(id);
                    let actions = ADVERTISED_ACTIONS.iter().filter(|action| node.supports_action(**action))
                        .map(|action| format!("{action:?}")).collect::<Vec<_>>();
                    serde_json::json!({
                        "node_id": id.0, "role": format!("{:?}", node.role()),
                        "children": node.children().iter().map(|id| id.0).collect::<Vec<_>>(),
                        "graft_tree_id": node.tree_id().map(|id| id.0.to_string()),
                        "label": text_allowed.then(|| node.label()).flatten(),
                        "value": text_allowed.then(|| node.value()).flatten(),
                        "text_redacted": !text_allowed,
                        "actions": actions,
                        "bounds": node.bounds().map(|bounds| [bounds.x0, bounds.y0, bounds.x1, bounds.y1]),
                        "transform": node.transform().map(|transform| transform.as_coeffs()),
                    })
                }).collect::<Vec<_>>();
                serde_json::json!({"tree_id": tree.tree_id.0.to_string(),
                    "root_node_id": tree.tree.as_ref().map(|tree| tree.root.0),
                    "focus_node_id": tree.focus.0, "nodes": nodes})
            }).collect::<Vec<_>>();
            let ordered_updates = published.ordered_updates.iter().map(|update| {
                serde_json::json!({"tree_id": update.tree_id.0.to_string(),
                    "node_ids": update.nodes.iter().map(|(id, _)| id.0).collect::<Vec<_>>(),
                    "focus_node_id": update.focus.0})
            }).collect::<Vec<_>>();
            serde_json::json!({"surface": surface.to_string(), "root_tree_id": published.root.0.to_string(),
                "trees": trees, "ordered_updates": ordered_updates})
        }).collect::<Vec<_>>();
        serde_json::json!({
            "scope": "successful in-process composed foreign frame; OS activation and AT acceptance unqualified",
            "public_fixture_text": public_fixture,
            "surface_count": surface_count, "tree_count": tree_count, "node_count": node_count,
            "surfaces": surfaces,
        })
    }

    pub(crate) fn action_target(&self, request: &ActionRequest) -> Result<Uuid, String> {
        let ActionTarget::Guest(surface) = self.owners.route(request) else {
            return Err("action names a host, unknown or retired foreign tree".into());
        };
        if !self.mounted.contains(surface) || !self.ready(*surface) {
            return Err("action names a hidden or incomplete foreign surface".into());
        }
        let node = self.surfaces[surface]
            .trees
            .get(&request.target_tree)
            .and_then(|tree| tree.nodes.get(&request.target_node))
            .ok_or("action names an absent or retired foreign node")?;
        if !node.supports_action(request.action) {
            return Err("foreign node does not advertise the requested action".into());
        }
        Ok(*surface)
    }
}

// AccessKit 0.24's complete public action enum, without requiring an optional
// enum-conversion feature or assuming Click/Focus are the only possible masks.
const ADVERTISED_ACTIONS: &[accesskit::Action] = &[
    accesskit::Action::Click,
    accesskit::Action::Focus,
    accesskit::Action::Blur,
    accesskit::Action::Collapse,
    accesskit::Action::Expand,
    accesskit::Action::CustomAction,
    accesskit::Action::Decrement,
    accesskit::Action::Increment,
    accesskit::Action::HideTooltip,
    accesskit::Action::ShowTooltip,
    accesskit::Action::ReplaceSelectedText,
    accesskit::Action::ScrollDown,
    accesskit::Action::ScrollLeft,
    accesskit::Action::ScrollRight,
    accesskit::Action::ScrollUp,
    accesskit::Action::ScrollIntoView,
    accesskit::Action::ScrollToPoint,
    accesskit::Action::SetScrollOffset,
    accesskit::Action::SetTextSelection,
    accesskit::Action::SetSequentialFocusNavigationStartingPoint,
    accesskit::Action::SetValue,
    accesskit::Action::ShowContextMenu,
];

fn protected_nodes(tree: &TreeUpdate) -> HashSet<NodeId> {
    let nodes: HashMap<_, _> = tree.nodes.iter().map(|(id, node)| (*id, node)).collect();
    let mut pending: Vec<_> = tree
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == Role::PasswordInput)
        .map(|(id, _)| *id)
        .collect();
    let mut protected = HashSet::new();
    while let Some(id) = pending.pop() {
        if protected.insert(id) {
            if let Some(node) = nodes.get(&id) {
                pending.extend(node.children().iter().copied());
            }
        }
    }
    protected
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Action, Uuid as TreeUuid};
    use accesskit_consumer::{Node as ConsumerNode, Tree as ConsumerTree, TreeChangeHandler};

    fn tree(id: u64) -> TreeId {
        TreeId(TreeUuid::from_u64_pair(0, id))
    }

    fn surface(id: u128) -> Uuid {
        Uuid::from_u128(id)
    }

    fn wrapper(id: TreeId, child: TreeId) -> TreeUpdate {
        let mut root = Node::new(Role::ScrollView);
        root.set_children(vec![NodeId(1)]);
        let mut graft = Node::new(Role::GenericContainer);
        graft.set_tree_id(child);
        TreeUpdate {
            nodes: vec![(NodeId(0), root), (NodeId(1), graft)],
            tree: Some(Tree::new(NodeId(0))),
            tree_id: id,
            focus: NodeId(1),
        }
    }

    fn button(label: &str) -> Node {
        let mut button = Node::new(Role::Button);
        button.set_label(label);
        button.add_action(Action::Click);
        button
    }

    fn document(id: TreeId, label: &str) -> TreeUpdate {
        let mut root = Node::new(Role::Document);
        root.set_children(vec![NodeId(1)]);
        TreeUpdate {
            nodes: vec![(NodeId(0), root), (NodeId(1), button(label))],
            tree: Some(Tree::new(NodeId(0))),
            tree_id: id,
            focus: NodeId(1),
        }
    }

    fn host(foreign: &ForeignA11y, visible: &[Uuid], focus: u64) -> TreeUpdate {
        let mut root = Node::new(Role::Window);
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        for (index, surface) in visible.iter().enumerate() {
            if let Some(graft) = foreign.graft(
                *surface,
                Rect::new(
                    index as f64 * 100.0,
                    0.0,
                    index as f64 * 100.0 + 100.0,
                    100.0,
                ),
            ) {
                let id = NodeId(10 + index as u64);
                children.push(id);
                nodes.push((id, graft));
            }
        }
        root.set_children(children);
        nodes.insert(0, (NodeId(0), root));
        TreeUpdate {
            nodes,
            tree: Some(Tree::new(NodeId(0))),
            tree_id: TreeId::ROOT,
            focus: NodeId(focus),
        }
    }

    fn request(id: TreeId, node: u64, action: Action) -> ActionRequest {
        ActionRequest {
            action,
            target_tree: id,
            target_node: NodeId(node),
            data: None,
        }
    }

    #[derive(Default)]
    struct Changes {
        focused: Vec<(NodeId, TreeId)>,
    }

    impl TreeChangeHandler for Changes {
        fn node_added(&mut self, _: &ConsumerNode<'_>) {}
        fn node_updated(&mut self, _: &ConsumerNode<'_>, _: &ConsumerNode<'_>) {}
        fn node_removed(&mut self, _: &ConsumerNode<'_>) {}
        fn focus_moved(&mut self, _: Option<&ConsumerNode<'_>>, new: Option<&ConsumerNode<'_>>) {
            if let Some(node) = new {
                self.focused
                    .push(node.tree_state.locate_node(node.id()).unwrap());
            }
        }
    }

    fn send(consumer: &mut ConsumerTree, updates: Vec<TreeUpdate>) {
        for update in updates {
            consumer.update_and_process_changes(update, &mut Changes::default());
        }
    }

    fn nested() -> ForeignA11y {
        let mut foreign = ForeignA11y::new();
        foreign.admit(surface(1), tree(101)).unwrap();
        foreign
            .apply(surface(1), wrapper(tree(101), tree(102)))
            .unwrap();
        assert!(!foreign.ready(surface(1)));
        foreign
            .apply(surface(1), wrapper(tree(102), tree(103)))
            .unwrap();
        foreign
            .apply(surface(1), document(tree(103), "first"))
            .unwrap();
        foreign
    }

    fn activate(foreign: &mut ForeignA11y, visible: &[Uuid], focus: u64) -> ConsumerTree {
        foreign.reset_publication();
        let initial = foreign
            .initial_host(host(foreign, visible, focus), visible)
            .unwrap();
        assert_eq!(initial.focus, NodeId(0));
        let mut consumer = ConsumerTree::new(initial, true);
        send(
            &mut consumer,
            foreign
                .frame(host(foreign, visible, focus), visible)
                .unwrap(),
        );
        consumer
    }

    #[test]
    fn inactive_adapter_does_not_consume_pending_activation() {
        use std::sync::atomic::{AtomicBool, Ordering};

        fn update_if_active(
            active: bool,
            updater: impl FnOnce() -> TreeUpdate,
        ) -> Option<TreeUpdate> {
            active.then(updater)
        }
        let foreign = nested();
        let visible = [surface(1)];
        let initial = foreign
            .initial_host(host(&foreign, &visible, 10), &visible)
            .unwrap();
        let replay = AtomicBool::new(true);
        // Early cadence inspection must leave the notification pending.
        assert!(replay.load(Ordering::Acquire));
        let mut called = false;
        let result = update_if_active(false, || {
            called = true;
            activation_safe_update(&replay, initial.clone(), &initial).0
        });
        assert!(result.is_none());
        assert!(!called);
        assert!(replay.load(Ordering::Acquire));
    }

    #[test]
    fn nested_focus_and_two_panes_keep_identical_local_ids_distinct() {
        let mut foreign = nested();
        foreign.admit(surface(2), tree(201)).unwrap();
        foreign
            .apply(surface(2), document(tree(201), "second"))
            .unwrap();
        let visible = [surface(1), surface(2)];
        foreign.reset_publication();
        let initial = foreign
            .initial_host(host(&foreign, &visible, 10), &visible)
            .unwrap();
        let mut consumer = ConsumerTree::new(initial, true);
        let mut changes = Changes::default();
        for update in foreign
            .frame(host(&foreign, &visible, 10), &visible)
            .unwrap()
        {
            consumer.update_and_process_changes(update, &mut changes);
        }
        assert_eq!(changes.focused, vec![(NodeId(1), tree(103))]);
        let focus = consumer.state().focus_id().unwrap();
        assert_eq!(
            consumer.state().locate_node(focus),
            Some((NodeId(1), tree(103)))
        );
        assert!(
            consumer
                .state()
                .node_by_tree_local_id(NodeId(1), tree(201))
                .is_some()
        );
        assert_eq!(
            foreign
                .action_target(&request(tree(103), 1, Action::Click))
                .unwrap(),
            surface(1)
        );
        assert_eq!(
            foreign
                .action_target(&request(tree(201), 1, Action::Click))
                .unwrap(),
            surface(2)
        );
        assert!(
            foreign
                .action_target(&request(tree(103), 1, Action::Focus))
                .is_err()
        );
        assert!(
            foreign
                .action_target(&request(tree(103), 99, Action::Click))
                .is_err()
        );
        assert!(
            foreign
                .action_target(&request(TreeId::ROOT, 0, Action::Click))
                .is_err()
        );
    }

    // Exercise this host's recursive expansion against each platform's actual
    // consumer. The older Windows consumer lacks node_by_tree_local_id, so
    // resolve its global nodes through the admitted subtree and locate_node.
    macro_rules! platform_recursive_consumer {
        ($module:ident, $consumer:ident) => {
            mod $module {
                use super::*;
                use $consumer::{
                    Node as PlatformNode, Tree as PlatformTree,
                    TreeChangeHandler as PlatformChangeHandler,
                };

                #[derive(Default)]
                struct FocusChanges(Vec<(NodeId, TreeId)>);

                impl PlatformChangeHandler for FocusChanges {
                    fn node_added(&mut self, _: &PlatformNode<'_>) {}
                    fn node_updated(&mut self, _: &PlatformNode<'_>, _: &PlatformNode<'_>) {}
                    fn node_removed(&mut self, _: &PlatformNode<'_>) {}
                    fn focus_moved(
                        &mut self,
                        _: Option<&PlatformNode<'_>>,
                        new: Option<&PlatformNode<'_>>,
                    ) {
                        if let Some(node) = new {
                            self.0.push(node.tree_state.locate_node(node.id()).unwrap());
                        }
                    }
                }

                #[test]
                fn activation_at_every_batch_boundary_restarts_nested_replay() {
                    use std::sync::atomic::{AtomicBool, Ordering};

                    // Cover initial replay (including final focus-only updates)
                    // and an unchanged frame whose guests were already live.
                    for already_live in [false, true] {
                        let mut template = nested();
                        let visible = [surface(1)];
                        if already_live {
                            let _ = activate(&mut template, &visible, 10);
                        }
                        let batch = template
                            .frame(host(&template, &visible, 10), &visible)
                            .unwrap();
                        if !already_live {
                            assert!(batch.iter().any(|update| update.tree_id == TreeId::ROOT
                                && update.nodes.is_empty()
                                && update.focus == NodeId(10)));
                        }
                        for activation_index in 0..batch.len() {
                            let mut foreign = nested();
                            let initial = foreign
                                .initial_host(host(&foreign, &visible, 10), &visible)
                                .unwrap();
                            let mut consumer = PlatformTree::new(initial.clone(), true);
                            let mut changes = FocusChanges::default();
                            if already_live {
                                for update in foreign
                                    .frame(host(&foreign, &visible, 10), &visible)
                                    .unwrap()
                                {
                                    consumer.update_and_process_changes(update, &mut changes);
                                }
                            }
                            let updates = foreign
                                .frame(host(&foreign, &visible, 10), &visible)
                                .unwrap();
                            assert_eq!(updates.len(), batch.len());
                            let replay = AtomicBool::new(false);
                            let mut aborted_at = None;
                            for (index, update) in updates.into_iter().enumerate() {
                                if index == activation_index {
                                    // Platform activation replaces the consumer
                                    // immediately before this guarded closure.
                                    consumer = PlatformTree::new(initial.clone(), true);
                                    replay.store(true, Ordering::Release);
                                }
                                let (update, abort) =
                                    activation_safe_update(&replay, update, &initial);
                                // Actual platform consumers validate every
                                // intermediate update, including the substitute.
                                consumer.update_and_process_changes(update, &mut changes);
                                if abort {
                                    aborted_at = Some(index);
                                    break;
                                }
                            }
                            assert_eq!(aborted_at, Some(activation_index));
                            assert!(!replay.load(Ordering::Acquire));
                            assert_eq!(
                                consumer
                                    .state()
                                    .locate_node(consumer.state().focus_id().unwrap()),
                                Some((NodeId(0), TreeId::ROOT))
                            );

                            foreign.reset_publication();
                            assert!(foreign.needs_publish());
                            for update in foreign
                                .frame(host(&foreign, &visible, 10), &visible)
                                .unwrap()
                            {
                                let (update, abort) =
                                    activation_safe_update(&replay, update, &initial);
                                assert!(!abort);
                                consumer.update_and_process_changes(update, &mut changes);
                            }
                            let state = consumer.state();
                            for id in [tree(101), tree(102), tree(103)] {
                                assert!(state.subtree_root(id).is_some());
                            }
                            let focus = state.focus_id().unwrap();
                            assert_eq!(state.locate_node(focus), Some((NodeId(1), tree(103))));
                            assert_eq!(
                                state.node_by_id(focus).unwrap().label().as_deref(),
                                Some("first")
                            );
                        }
                    }
                }

                #[test]
                fn recursive_frame_orders_descendants_and_preserves_two_pane_ids() {
                    let mut foreign = nested();
                    foreign.admit(surface(2), tree(201)).unwrap();
                    foreign
                        .apply(surface(2), document(tree(201), "second pane"))
                        .unwrap();
                    let visible = [surface(1), surface(2)];
                    foreign.reset_publication();
                    let initial = foreign
                        .initial_host(host(&foreign, &visible, 10), &visible)
                        .unwrap();
                    assert_eq!(initial.focus, NodeId(0));
                    let mut consumer = PlatformTree::new(initial, true);
                    let mut changes = FocusChanges::default();
                    for update in foreign
                        .frame(host(&foreign, &visible, 10), &visible)
                        .unwrap()
                    {
                        // The consumer validates every intermediate parent,
                        // subtree and focus transition, not only the final tree.
                        consumer.update_and_process_changes(update, &mut changes);
                    }
                    assert_eq!(changes.0, vec![(NodeId(1), tree(103))]);
                    let state = consumer.state();
                    for id in [tree(101), tree(102), tree(103), tree(201)] {
                        assert!(state.subtree_root(id).is_some());
                    }
                    let first = state.focus_id().unwrap();
                    let second = state
                        .node_by_id(state.subtree_root(tree(201)).unwrap())
                        .unwrap()
                        .children()
                        .next()
                        .unwrap()
                        .id();
                    assert_ne!(first, second);
                    assert_eq!(state.locate_node(first), Some((NodeId(1), tree(103))));
                    assert_eq!(state.locate_node(second), Some((NodeId(1), tree(201))));
                    assert_eq!(
                        state.node_by_id(first).unwrap().label().as_deref(),
                        Some("first")
                    );
                    assert_eq!(
                        state.node_by_id(second).unwrap().label().as_deref(),
                        Some("second pane")
                    );
                }
            }
        };
    }

    platform_recursive_consumer!(windows_consumer, accesskit_consumer_windows);
    platform_recursive_consumer!(unix_consumer, accesskit_consumer_unix);

    #[test]
    fn idle_delta_reaches_consumer_without_a_pixel_frame() {
        let mut foreign = nested();
        let visible = [surface(1)];
        let mut consumer = activate(&mut foreign, &visible, 10);
        assert!(!foreign.needs_publish());
        foreign
            .apply(
                surface(1),
                TreeUpdate {
                    nodes: vec![(NodeId(1), button("idle title"))],
                    tree: None,
                    tree_id: tree(103),
                    focus: NodeId(1),
                },
            )
            .unwrap();
        assert!(foreign.needs_publish());
        send(
            &mut consumer,
            foreign
                .frame(host(&foreign, &visible, 10), &visible)
                .unwrap(),
        );
        assert_eq!(
            consumer
                .state()
                .node_by_tree_local_id(NodeId(1), tree(103))
                .unwrap()
                .label()
                .as_deref(),
            Some("idle title")
        );
        assert!(!foreign.needs_publish());
    }

    #[test]
    fn navigation_prunes_old_nested_trees_and_ignores_stale_callbacks() {
        let mut foreign = nested();
        let visible = [surface(1)];
        let mut consumer = activate(&mut foreign, &visible, 10);
        foreign
            .apply(surface(1), wrapper(tree(101), tree(104)))
            .unwrap();
        assert!(!foreign.ready(surface(1)));
        assert!(
            foreign
                .action_target(&request(tree(103), 1, Action::Click))
                .is_err()
        );
        assert!(
            foreign
                .apply(surface(1), document(tree(103), "stale"))
                .is_err()
        );
        foreign
            .apply(surface(1), document(tree(104), "replacement"))
            .unwrap();
        send(
            &mut consumer,
            foreign
                .frame(host(&foreign, &visible, 10), &visible)
                .unwrap(),
        );
        assert!(consumer.state().subtree_root(tree(102)).is_none());
        assert!(consumer.state().subtree_root(tree(103)).is_none());
        assert_eq!(
            consumer
                .state()
                .locate_node(consumer.state().focus_id().unwrap()),
            Some((NodeId(1), tree(104)))
        );
    }

    #[test]
    fn hidden_remount_and_adapter_reactivation_replay_all_descendants() {
        let mut foreign = nested();
        let visible = [surface(1)];
        let mut consumer = activate(&mut foreign, &visible, 10);
        send(
            &mut consumer,
            foreign.frame(host(&foreign, &[], 0), &[]).unwrap(),
        );
        assert!(consumer.state().subtree_root(tree(101)).is_none());
        assert!(
            foreign
                .action_target(&request(tree(103), 1, Action::Click))
                .is_err()
        );
        send(
            &mut consumer,
            foreign
                .frame(host(&foreign, &visible, 10), &visible)
                .unwrap(),
        );
        assert_eq!(
            consumer
                .state()
                .locate_node(consumer.state().focus_id().unwrap()),
            Some((NodeId(1), tree(103)))
        );
        let consumer = activate(&mut foreign, &visible, 10);
        assert!(consumer.state().subtree_root(tree(103)).is_some());
    }

    #[test]
    fn removed_local_nodes_retire_their_actions() {
        let mut foreign = nested();
        let visible = [surface(1)];
        let mut consumer = activate(&mut foreign, &visible, 10);
        foreign
            .apply(
                surface(1),
                TreeUpdate {
                    nodes: vec![(NodeId(0), Node::new(Role::Document))],
                    tree: None,
                    tree_id: tree(103),
                    focus: NodeId(0),
                },
            )
            .unwrap();
        assert!(
            foreign
                .action_target(&request(tree(103), 1, Action::Click))
                .is_err()
        );
        send(
            &mut consumer,
            foreign
                .frame(host(&foreign, &visible, 10), &visible)
                .unwrap(),
        );
        assert!(
            consumer
                .state()
                .node_by_tree_local_id(NodeId(1), tree(103))
                .is_none()
        );
    }

    #[test]
    fn malformed_known_tree_withdraws_but_unknown_tree_cannot_corrupt() {
        let mut foreign = nested();
        let visible = [surface(1)];
        let mut consumer = activate(&mut foreign, &visible, 10);
        assert!(
            foreign
                .apply(surface(1), document(tree(999), "unknown"))
                .is_err()
        );
        assert!(foreign.ready(surface(1)));
        assert_eq!(
            foreign
                .action_target(&request(tree(103), 1, Action::Click))
                .unwrap(),
            surface(1)
        );
        let mut invalid = Node::new(Role::Document);
        invalid.set_children(vec![NodeId(999)]);
        assert!(
            foreign
                .apply(
                    surface(1),
                    TreeUpdate {
                        nodes: vec![(NodeId(0), invalid)],
                        tree: None,
                        tree_id: tree(103),
                        focus: NodeId(0),
                    }
                )
                .is_err()
        );
        assert!(!foreign.ready(surface(1)));
        assert!(foreign.needs_publish());
        assert!(
            foreign
                .graft(surface(1), Rect::new(0.0, 0.0, 100.0, 100.0))
                .is_none()
        );
        assert!(
            foreign
                .action_target(&request(tree(103), 1, Action::Click))
                .is_err()
        );
        send(
            &mut consumer,
            foreign.frame(host(&foreign, &[], 0), &[]).unwrap(),
        );
        assert!(consumer.state().subtree_root(tree(101)).is_none());
    }

    #[test]
    fn duplicate_ownership_cycles_invalid_focus_and_local_graphs_fail_closed() {
        let mut foreign = nested();
        assert!(foreign.admit(surface(2), tree(101)).is_err());
        assert!(foreign.admit(surface(2), TreeId::ROOT).is_err());
        assert!(
            foreign
                .apply(surface(2), document(tree(103), "foreign"))
                .is_err()
        );
        assert!(foreign.ready(surface(1)));

        foreign.admit(surface(2), tree(201)).unwrap();
        foreign
            .apply(surface(2), document(tree(201), "owned elsewhere"))
            .unwrap();
        foreign.admit(surface(3), tree(301)).unwrap();
        assert!(
            foreign
                .apply(surface(3), wrapper(tree(301), tree(201)))
                .is_err()
        );
        assert!(!foreign.ready(surface(3)));
        assert!(foreign.ready(surface(2)));

        let mut duplicate_grafts = wrapper(tree(301), tree(302));
        duplicate_grafts.nodes[0]
            .1
            .set_children(vec![NodeId(1), NodeId(2)]);
        duplicate_grafts
            .nodes
            .push((NodeId(2), duplicate_grafts.nodes[1].1.clone()));
        let mut local_cycle = document(tree(301), "cycle");
        local_cycle.nodes[1].1.set_children(vec![NodeId(0)]);
        let mut invalid_focus = document(tree(301), "focus");
        invalid_focus.focus = NodeId(999);
        let mut duplicate_node = document(tree(301), "duplicate");
        duplicate_node.nodes.push((NodeId(1), button("duplicate")));
        let mut unreachable = document(tree(301), "unreachable");
        unreachable.nodes.push((NodeId(9), button("unreachable")));
        let mut graft_with_children = wrapper(tree(301), tree(302));
        graft_with_children.nodes[1].1.set_children(vec![NodeId(0)]);
        for update in [
            wrapper(tree(301), tree(301)),
            duplicate_grafts,
            local_cycle,
            invalid_focus,
            duplicate_node,
            unreachable,
            graft_with_children,
        ] {
            foreign.admit(surface(3), tree(301)).unwrap();
            assert!(foreign.apply(surface(3), update).is_err());
            assert!(!foreign.ready(surface(3)));
            assert!(foreign.ready(surface(1)));
        }
    }

    #[test]
    fn incomplete_tree_waits_and_invalid_host_frame_keeps_publication_pending() {
        let mut foreign = ForeignA11y::new();
        foreign.admit(surface(1), tree(101)).unwrap();
        foreign
            .apply(surface(1), wrapper(tree(101), tree(102)))
            .unwrap();
        assert!(!foreign.ready(surface(1)));
        // A late delta after upstream reactivation is not an initial tree.
        // AAC can carry Some(Tree) while sending only changed nodes; another
        // producer can omit Tree entirely. Neither should poison admission.
        for tree_data in [None, Some(Tree::new(NodeId(0)))] {
            assert!(
                foreign
                    .apply(
                        surface(1),
                        TreeUpdate {
                            nodes: vec![(NodeId(1), button("unusable late delta"))],
                            tree: tree_data,
                            tree_id: tree(102),
                            focus: NodeId(1),
                        }
                    )
                    .is_err()
            );
            assert!(foreign.surfaces.contains_key(&surface(1)));
            assert!(!foreign.ready(surface(1)));
            assert_eq!(foreign.published_counts(), (0, 0, 0));
            assert!(
                foreign
                    .action_target(&request(tree(102), 1, Action::Click))
                    .is_err()
            );
        }
        assert!(
            foreign
                .graft(surface(1), Rect::new(0.0, 0.0, 100.0, 100.0))
                .is_none()
        );
        let visible = [surface(1)];
        let initial = foreign
            .initial_host(host(&foreign, &visible, 0), &visible)
            .unwrap();
        let mut consumer = ConsumerTree::new(initial, true);
        send(
            &mut consumer,
            foreign
                .frame(host(&foreign, &visible, 0), &visible)
                .unwrap(),
        );
        assert!(consumer.state().subtree_root(tree(101)).is_none());
        foreign
            .apply(surface(1), document(tree(102), "now ready"))
            .unwrap();
        assert!(foreign.needs_publish());
        // A host cannot publish a guest without the matching graft.
        assert!(foreign.frame(host(&foreign, &[], 0), &visible).is_err());
        assert!(foreign.needs_publish());
        assert!(
            foreign
                .action_target(&request(tree(102), 1, Action::Click))
                .is_err()
        );
        send(
            &mut consumer,
            foreign
                .frame(host(&foreign, &visible, 10), &visible)
                .unwrap(),
        );
        assert!(consumer.state().subtree_root(tree(102)).is_some());
    }

    #[test]
    fn retirement_and_new_root_reject_old_generation_actions_and_updates() {
        let mut foreign = nested();
        let visible = [surface(1)];
        let mut consumer = activate(&mut foreign, &visible, 10);
        foreign.retire(surface(1));
        foreign.admit(surface(1), tree(401)).unwrap();
        assert!(
            foreign
                .apply(surface(1), document(tree(103), "old generation"))
                .is_err()
        );
        foreign
            .apply(surface(1), document(tree(401), "new generation"))
            .unwrap();
        send(
            &mut consumer,
            foreign
                .frame(host(&foreign, &visible, 10), &visible)
                .unwrap(),
        );
        assert!(
            foreign
                .action_target(&request(tree(103), 1, Action::Click))
                .is_err()
        );
        assert_eq!(
            foreign
                .action_target(&request(tree(401), 1, Action::Click))
                .unwrap(),
            surface(1)
        );
        foreign.clear();
        assert!(!foreign.ready(surface(1)));
        assert!(
            foreign
                .action_target(&request(tree(401), 1, Action::Click))
                .is_err()
        );
    }

    #[test]
    fn publication_diagnostic_uses_emitted_frame_not_pending_supplier_delta() {
        let mut foreign = nested();
        let visible = [surface(1)];
        let mut consumer = activate(&mut foreign, &visible, 10);
        assert_eq!(foreign.published_counts(), (1, 3, 6));
        let diagnostic = foreign.publication_diagnostic(true);
        assert_eq!(
            diagnostic["surfaces"][0]["root_tree_id"],
            tree(101).0.to_string()
        );
        let trees = diagnostic["surfaces"][0]["trees"].as_array().unwrap();
        assert_eq!(trees.len(), 3);
        assert!(trees.iter().all(|tree| tree["root_node_id"] == 0));
        assert!(trees.iter().all(|tree| tree["focus_node_id"] == 1));
        foreign
            .apply(
                surface(1),
                TreeUpdate {
                    nodes: vec![(NodeId(1), button("pending supplier label"))],
                    tree: None,
                    tree_id: tree(103),
                    focus: NodeId(1),
                },
            )
            .unwrap();
        assert!(!foreign.published_text().contains("pending supplier label"));
        send(
            &mut consumer,
            foreign
                .frame(host(&foreign, &visible, 10), &visible)
                .unwrap(),
        );
        assert!(foreign.published_text().contains("pending supplier label"));
        assert_eq!(foreign.published_counts(), (1, 3, 6));
    }

    #[test]
    fn publication_diagnostic_withdraws_hidden_retired_malformed_and_reset_trees() {
        let mut foreign = nested();
        let visible = [surface(1)];
        let mut consumer = activate(&mut foreign, &visible, 10);
        send(
            &mut consumer,
            foreign.frame(host(&foreign, &[], 0), &[]).unwrap(),
        );
        assert_eq!(foreign.published_counts(), (0, 0, 0));
        send(
            &mut consumer,
            foreign
                .frame(host(&foreign, &visible, 10), &visible)
                .unwrap(),
        );
        assert_eq!(foreign.published_counts(), (1, 3, 6));
        foreign.reset_publication();
        assert_eq!(foreign.published_counts(), (0, 0, 0));
        let _ = activate(&mut foreign, &visible, 10);
        assert_eq!(foreign.published_counts(), (1, 3, 6));
        let mut invalid = Node::new(Role::Document);
        invalid.set_children(vec![NodeId(999)]);
        assert!(
            foreign
                .apply(
                    surface(1),
                    TreeUpdate {
                        nodes: vec![(NodeId(0), invalid)],
                        tree: None,
                        tree_id: tree(103),
                        focus: NodeId(0),
                    }
                )
                .is_err()
        );
        assert_eq!(foreign.published_counts(), (0, 0, 0));
        assert!(
            foreign.publication_diagnostic(true)["surfaces"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        foreign.admit(surface(1), tree(501)).unwrap();
        foreign
            .apply(surface(1), document(tree(501), "replacement"))
            .unwrap();
        let _ = activate(&mut foreign, &visible, 10);
        assert_eq!(foreign.published_counts(), (1, 1, 2));
        foreign.retire(surface(1));
        assert_eq!(foreign.published_counts(), (0, 0, 0));
    }

    #[test]
    fn fixture_text_optin_never_exposes_password_nodes_or_descendants() {
        let mut foreign = ForeignA11y::new();
        foreign.admit(surface(1), tree(501)).unwrap();
        let mut root = Node::new(Role::Document);
        root.set_label("public fixture marker");
        root.set_children(vec![NodeId(1)]);
        let mut password = Node::new(Role::PasswordInput);
        password.set_label("secret label");
        password.set_value("secret value");
        password.add_action(Action::SetValue);
        password.set_children(vec![NodeId(2)]);
        let mut text = Node::new(Role::TextRun);
        text.set_value("secret descendant");
        foreign
            .apply(
                surface(1),
                TreeUpdate {
                    nodes: vec![(NodeId(0), root), (NodeId(1), password), (NodeId(2), text)],
                    tree: Some(Tree::new(NodeId(0))),
                    tree_id: tree(501),
                    focus: NodeId(0),
                },
            )
            .unwrap();
        let _ = activate(&mut foreign, &[surface(1)], 10);
        let redacted = foreign.publication_diagnostic(false).to_string();
        assert!(!redacted.contains("public fixture marker"));
        assert!(!redacted.contains("secret"));
        let public = foreign.publication_diagnostic(true).to_string();
        assert!(public.contains("public fixture marker"));
        assert!(public.contains("SetValue"));
        assert!(!public.contains("secret"));
        assert_eq!(foreign.published_text(), "public fixture marker");
        assert_eq!(foreign.published_counts(), (1, 1, 3));
    }
}
