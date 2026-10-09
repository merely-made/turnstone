// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Product controls belong to one Surface independently of shown content.
use chartulary::{AcceptAll, FacetId};
use mere::canvas::Canvas;
use mere::kernel::graph::{Graph, NodeFacetStore};
use mere::kernel::persistence::GraphSnapshot;
use serde_json::Value;
use uuid::Uuid;
const LEGACY_LABELS: &str = "turnstone.controls.v1.legacy-labels";
const RECYCLE_LABELS: &str = "turnstone.controls.v1.unqualified-recycle-labels";
#[derive(Clone, Copy, Debug)]
pub(crate) enum Control {
    Keep,
    Feed,
    FeedEntry,
    Unread,
}
impl Control {
    fn facet(self) -> &'static str {
        match self {
            Self::Keep => "turnstone.controls.v1.keep",
            Self::Feed => "turnstone.controls.v1.feed",
            Self::FeedEntry => "turnstone.controls.v1.feed-entry",
            Self::Unread => "turnstone.controls.v1.unread",
        }
    }
    fn legacy_label(self) -> &'static str {
        match self {
            Self::Keep => crate::feed::KEEP_TAG,
            Self::Feed => crate::feed::FEED_TAG,
            Self::FeedEntry => crate::feed::FEED_ENTRY_TAG,
            Self::Unread => crate::feed::UNREAD_TAG,
        }
    }
}
pub(crate) fn is_legacy_label(label: &str) -> bool {
    matches!(label, "keep" | "feed" | "feed-entry" | "unread")
}
pub(crate) fn read(graph: &Graph, member: Uuid, control: Control) -> bool {
    graph.get_node_by_id(member).is_some()
        && graph
            .facets()
            .get(&member, &FacetId::new(control.facet()))
            .and_then(Value::as_bool)
            .unwrap_or(false)
}
/// Host-owned Surface facets only. Malformed existing values remain evidence.
pub(crate) fn set(canvas: &mut Canvas, member: Uuid, control: Control, enabled: bool) -> bool {
    if canvas.graph().get_node_by_id(member).is_none() {
        return false;
    }
    let id = FacetId::new(control.facet());
    if let Some(value) = canvas.facets().get(&member, &id) {
        let Some(old) = value.as_bool() else {
            tracing::warn!(%member,facet=control.facet(),"invalid Surface control retained");
            return false;
        };
        if old == enabled {
            return false;
        }
    }
    // Explicit false prevents stale labels from resurrecting a control.
    canvas
        .facets_mut()
        .set(member, id, Value::Bool(enabled), &AcceptAll)
        .is_ok()
}
/// Capture only old per-Surface columns before resource materialization.
/// Canonical sidecar values override defaults. Resource labels are never read.
pub(crate) fn extract_legacy(snapshot: &mut GraphSnapshot) -> NodeFacetStore {
    let mut facets = NodeFacetStore::new();
    for node in &mut snapshot.nodes {
        let Ok(member) = Uuid::parse_str(&node.node_id) else {
            continue;
        };
        let labels: Vec<String> = node
            .tags
            .iter()
            .filter(|label| is_legacy_label(label))
            .cloned()
            .collect();
        if labels.is_empty() {
            continue;
        }
        for control in [
            Control::Keep,
            Control::Feed,
            Control::FeedEntry,
            Control::Unread,
        ] {
            let enabled = labels.iter().any(|label| label == control.legacy_label());
            facets
                .set(
                    member,
                    FacetId::new(control.facet()),
                    Value::Bool(enabled),
                    &AcceptAll,
                )
                .expect("AcceptAll accepts booleans");
        }
        facets
            .set(
                member,
                FacetId::new(LEGACY_LABELS),
                serde_json::json!(labels),
                &AcceptAll,
            )
            .expect("AcceptAll accepts retained labels");
        node.tags.retain(|label| !is_legacy_label(label));
    }
    facets
}
/// Recycle label vectors lack source/placement proof. Preserve reserved labels
/// without promoting them into Keep/subscription/read state.
pub(crate) fn preserve_recycle_labels(canvas: &mut Canvas, member: Uuid, tags: &[String]) {
    let labels: Vec<&str> = tags
        .iter()
        .filter(|label| is_legacy_label(label))
        .map(String::as_str)
        .collect();
    if !labels.is_empty()
        && canvas
            .facets()
            .get(&member, &FacetId::new(RECYCLE_LABELS))
            .is_none()
    {
        let _ = canvas.facets_mut().set(
            member,
            FacetId::new(RECYCLE_LABELS),
            serde_json::json!(labels),
            &AcceptAll,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use mere::kernel::graph::apply::add_node;
    #[test]
    fn legacy_surface_labels_preserve_evidence_and_canonical_false() {
        let mut graph = Graph::new();
        let first = Uuid::from_u128(601);
        let second = Uuid::from_u128(602);
        for id in [first, second] {
            add_node(
                &mut graph,
                Some(id),
                "https://controls.test/page".into(),
                Default::default(),
            );
        }
        let mut snapshot = graph.to_snapshot();
        snapshot
            .nodes
            .iter_mut()
            .find(|node| node.node_id == first.to_string())
            .unwrap()
            .tags = vec![
            "keep".into(),
            "feed".into(),
            "feed-entry".into(),
            "unread".into(),
            "Paper".into(),
        ];
        let facets = extract_legacy(&mut snapshot);
        assert_eq!(
            facets.get(&first, &FacetId::new(LEGACY_LABELS)).unwrap(),
            &serde_json::json!(["keep", "feed", "feed-entry", "unread"])
        );
        assert_eq!(
            snapshot
                .nodes
                .iter()
                .find(|node| node.node_id == first.to_string())
                .unwrap()
                .tags,
            vec!["Paper"]
        );
        assert!(facets.facets_of(&second).is_none());
        assert!(extract_legacy(&mut snapshot).is_empty());
        let mut graph = Graph::from_snapshot(&snapshot);
        graph.overlay_facets(facets);
        assert!(read(&graph, first, Control::Keep));
        assert!(!read(&graph, second, Control::Keep));
        let mut canonical = NodeFacetStore::new();
        canonical
            .set(
                first,
                FacetId::new(Control::Keep.facet()),
                Value::Bool(false),
                &AcceptAll,
            )
            .unwrap();
        graph.overlay_facets(canonical);
        assert!(!read(&graph, first, Control::Keep));
    }
}
