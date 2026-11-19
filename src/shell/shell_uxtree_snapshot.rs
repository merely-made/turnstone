// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's read-only tooling JSON for a composed primary-window frame.
//! The output carries no stability promise and grants no action authority.
//! Turnstone applies its text policy before reading any snapshot text fields.
//! Ordinary captures omit text. The existing public-fixture opt-in allows
//! text from fixture nodes outside protected subtrees.

use std::io::Write;

use accesskit::{Node, NodeId, TreeId, TreeUpdate};
use serde_json::{Value, json};
use uxtree::envelope::{AddressSpace, Stability};
use uxtree::lint::ADVERTISED_ACTIONS;

use super::{PrimaryA11yFrame, Shell};
use crate::a11y::{ActionEnvelope, ActionEnvelopes};

impl Shell {
    pub(in crate::shell) fn record_uxtree(
        &mut self,
        name: &str,
        public_fixture: bool,
    ) -> Result<(), String> {
        self.sync_content_placements();
        self.drain_surface_accessibility();
        let plan = self.surface_plan();
        let PrimaryA11yFrame {
            update,
            visible,
            guests,
            routes,
            ..
        } = self.primary_a11y_frame(None, &plan);
        let forest = self
            .surface_a11y
            .tooling_forest(update, &visible, &guests)?;
        let snapshot = tooling_snapshot(&forest, &routes, public_fixture);
        let path = self.shared_out_dir.join(format!("{name}.uxtree.json"));
        let bytes = serde_json::to_vec_pretty(&snapshot).map_err(|error| error.to_string())?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                format!("record-uxtree could not create {}: {error}", path.display())
            })?;
        file.write_all(&bytes).map_err(|error| error.to_string())
    }
}

fn tooling_snapshot(
    forest: &[TreeUpdate],
    routes: &ActionEnvelopes,
    public_fixture: bool,
) -> Value {
    let protected = crate::a11y::protected_addresses(forest);
    let trees: Vec<_> = forest
        .iter()
        .map(|tree| {
            let nodes: Vec<_> = tree
                .nodes
                .iter()
                .map(|(id, node)| {
                    let text_allowed = public_fixture && !protected.contains(&(tree.tree_id, *id));
                    // Keep the accessors inside the decision, before any JSON
                    // buffer can receive text, even transiently.
                    let text = text_allowed.then(|| node_text(node));
                    let envelope = (tree.tree_id == TreeId::ROOT)
                        .then(|| routes.get(id))
                        .flatten()
                        .map(|envelope| observed_envelope(envelope, text_allowed));
                    let actions: Vec<_> = ADVERTISED_ACTIONS
                        .iter()
                        .filter(|action| node.supports_action(**action))
                        .map(|action| format!("{action:?}"))
                        .collect();
                    json!({
                        "node_id": id.0.to_string(),
                        "role": format!("{:?}", node.role()),
                        "children": node.children().iter().map(|id| id.0.to_string()).collect::<Vec<_>>(),
                        "graft_tree_id": node.tree_id().map(|id| id.0),
                        "bounds": node.bounds().map(|r| [r.x0, r.y0, r.x1, r.y1]),
                        "transform": node.transform().map(|t| t.as_coeffs()),
                        "actions": actions,
                        "text_redacted": !text_allowed,
                        "text": text,
                        "envelope": envelope,
                    })
                })
                .collect();
            json!({
                "tree_id": tree.tree_id.0,
                "root_node_id": tree.tree.as_ref().map(|tree| tree.root.0.to_string()),
                "focus_node_id": tree.focus.0.to_string(),
                "nodes": nodes,
            })
        })
        .collect();
    json!({
        "kind": "turnstone-uxtree-snapshot",
        "root_tree_id": TreeId::ROOT.0,
        "public_fixture_text": public_fixture,
        "trees": trees,
    })
}

fn node_text(node: &Node) -> Value {
    #[cfg(test)]
    TEXT_READS.with(|reads| reads.set(reads.get() + 1));
    json!({"label": node.label(), "value": node.value(), "description": node.description()})
}

#[cfg(test)]
thread_local! {
    static TEXT_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn observed_envelope(envelope: &ActionEnvelope, text_allowed: bool) -> Value {
    let address = match envelope.address.instance {
        AddressSpace::Producer(NodeId(instance)) => json!({"producer": instance.to_string()}),
        AddressSpace::Grafted(tree) => json!({"tree_id": tree.0}),
    };
    let stability = match envelope.stability {
        Stability::Life => json!({"kind": "life"}),
        Stability::Scoped { revision } => json!({"kind": "scoped", "revision": revision}),
        Stability::Derived { generation } => json!({"kind": "derived", "generation": generation}),
        Stability::Authored => json!({"kind": "authored"}),
    };
    let (action_ids, domain_ref, authored_key) = if text_allowed {
        (
            Some(envelope.action_ids.as_slice()),
            envelope.domain_ref,
            envelope.authored_key.as_deref(),
        )
    } else {
        (None, None, None)
    };
    // Payloads and publication witnesses are process-local. Describe the
    // owner without traversing its route or inventing a generation.
    json!({
        "address": {"instance": address, "node_id": envelope.address.local.0.to_string()},
        "stability": stability,
        "provenance": "turnstone",
        "action_ids": action_ids,
        "domain_ref": domain_ref,
        "authored_key": authored_key,
    })
}

#[cfg(test)]
#[path = "shell_uxtree_snapshot_tests.rs"]
mod tests;
