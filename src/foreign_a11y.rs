// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Retained foreign trees around Mere's validated AccessKit graft composition.
//! Supplier TreeIds and local NodeIds stay intact. Missing declared subtrees
//! wait for their initial update; malformed known trees withdraw their surface.

use accesskit::{ActionRequest, Node, NodeId, Rect, Role, Tree, TreeId, TreeUpdate};
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;
use uxtree::graft::{ActionTarget, Composition, GraftTable, Grafts, graft_node};

#[derive(Clone)]
struct RetainedTree {
    tree: Tree,
    nodes: std::sync::Arc<BTreeMap<NodeId, Node>>,
    focus: NodeId,
    publication: Grafts,
}

enum MaterializationError {
    Incomplete(String),
    Invalid(String),
}

#[derive(Clone)]
struct SurfaceForest {
    root: TreeId,
    trees: HashMap<TreeId, RetainedTree>,
    declared: HashSet<TreeId>,
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
    published_forest: Vec<TreeUpdate>,
}

struct PublishedSurface {
    root: TreeId,
    trees: BTreeMap<TreeId, TreeUpdate>,
    ordered_updates: Vec<TreeUpdate>,
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

#[cfg(test)]
#[path = "foreign_a11y/tests.rs"]
mod tests;

#[path = "foreign_a11y/activation_safe_update.rs"]
mod activation_safe_update;
pub(crate) use activation_safe_update::activation_safe_update;
#[path = "foreign_a11y/expand.rs"]
mod expand;
#[path = "foreign_a11y/impl_std_fmt_display_for_materialization.rs"]
mod impl_std_fmt_display_for_materialization;
#[path = "foreign_a11y/retained_tree_update.rs"]
mod retained_tree_update;
#[path = "foreign_a11y/surface_forest_reachable.rs"]
mod surface_forest_reachable;
use expand::expand;
#[path = "foreign_a11y/foreign_a11y_new.rs"]
mod foreign_a11y_new;
#[path = "foreign_a11y/foreign_a11y_publication_diagnostic.rs"]
mod foreign_a11y_publication_diagnostic;
#[path = "foreign_a11y/impl_default_for_foreign_a11y.rs"]
mod impl_default_for_foreign_a11y;
#[path = "foreign_a11y/protected_nodes.rs"]
mod protected_nodes;
use protected_nodes::protected_nodes;
