// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Read-only relationship navigation over the kernel's canonical projection.
//! Direction and kind are retained even when several relations share a member.
use crate::{action::Effect, app::App, panes::GraphId};
use mere::kernel::graph::{Graph, RelationKind, SemanticSubKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RelationshipTarget {
    pub graph: GraphId,
    pub source: uuid::Uuid,
    pub member: uuid::Uuid,
    pub kind: RelationKind,
    pub incoming: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationshipRow {
    pub target: RelationshipTarget,
    pub title: String,
    pub relationship: String,
    pub address: String,
}

pub fn relationships(graph_id: GraphId, graph: &Graph, source: uuid::Uuid) -> Vec<RelationshipRow> {
    let Some((key, _)) = graph.get_node_by_id(source) else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut rows = Vec::new();
    for (_, relation) in graph.projected_relations() {
        let incoming = relation.to == key;
        let other = if relation.from == key {
            relation.to
        } else if incoming {
            relation.from
        } else {
            continue;
        };
        let Some(node) = graph.get_node(other) else {
            continue;
        };
        if node.id == source {
            continue;
        }
        let target = RelationshipTarget {
            graph: graph_id,
            source,
            member: node.id,
            kind: relation.kind,
            incoming: relation.from != key,
        };
        if seen.insert(target) {
            rows.push(RelationshipRow {
                target,
                title: document_label(graph, other),
                address: node.url().to_string(),
                relationship: relationship_label(relation.kind, target.incoming),
            });
        }
    }
    rows.sort_by(|a, b| {
        a.relationship
            .cmp(&b.relationship)
            .then(a.title.cmp(&b.title))
            .then(a.target.member.cmp(&b.target.member))
    });
    rows
}

/// Keep an unloaded or host-only title useful when several members share a
/// domain. Loaded document titles retain the kernel's ordinary presentation.
pub(crate) fn document_label(graph: &Graph, key: mere::kernel::graph::NodeKey) -> String {
    let Some(node) = graph.get_node(key) else {
        return "Document unavailable".into();
    };
    let title = node.title.trim();
    let host_only = url::Url::parse(node.url())
        .ok()
        .and_then(|url| url.host_str().map(|host| host == title))
        .unwrap_or(false);
    if title.is_empty() || title == node.url() || host_only {
        node.url().into()
    } else {
        graph.node_display_label(key)
    }
}

fn relationship_label(kind: RelationKind, incoming: bool) -> String {
    use SemanticSubKind::*;
    let (out, into) = match kind {
        RelationKind::Traversal => ("Went to", "Reached from"),
        RelationKind::Semantic(Hyperlink) => ("Links to", "Linked from"),
        RelationKind::Semantic(Cites) => ("Cites", "Cited by"),
        RelationKind::Semantic(Quotes) => ("Quotes", "Quoted by"),
        RelationKind::Semantic(Summarizes) => ("Summarizes", "Summarized by"),
        RelationKind::Semantic(Elaborates) => ("Elaborates on", "Elaborated by"),
        RelationKind::Semantic(ExampleOf) => ("Example of", "Has example"),
        RelationKind::Semantic(Supports) => ("Supports", "Supported by"),
        RelationKind::Semantic(Contradicts) => ("Contradicts", "Contradicted by"),
        RelationKind::Semantic(Questions) => ("Questions", "Questioned by"),
        RelationKind::Semantic(SameEntityAs) => ("Same entity as", "Same entity as"),
        RelationKind::Semantic(DuplicateOf) => ("Duplicate of", "Has duplicate"),
        RelationKind::Semantic(CanonicalMirrorOf) => {
            ("Canonical mirror of", "Has canonical mirror")
        },
        RelationKind::Semantic(DependsOn) => ("Depends on", "Required by"),
        RelationKind::Semantic(Blocks) => ("Blocks", "Blocked by"),
        RelationKind::Semantic(NextStep) => ("Next step", "Previous step"),
        RelationKind::Semantic(UserGrouped) => ("Grouped with", "Grouped with"),
        RelationKind::Semantic(AgentDerived) => ("Derived relation to", "Derived relation from"),
        RelationKind::Containment(_) => ("Contains", "Contained by"),
        RelationKind::Arrangement(_) => ("Arranged with", "Arranged with"),
        RelationKind::Imported(_) => ("Import relation to", "Import relation from"),
        RelationKind::Provenance(_) => ("Derived from", "Source for"),
        RelationKind::OpenPredicate => ("Relation to", "Relation from"),
    };
    let label = if incoming { into } else { out };
    match kind {
        RelationKind::Containment(sub) => format!("{label} · {}", words(&format!("{sub:?}"))),
        RelationKind::Arrangement(sub) => format!("{label} · {}", words(&format!("{sub:?}"))),
        RelationKind::Imported(sub) => format!("{label} · {}", words(&format!("{sub:?}"))),
        RelationKind::Provenance(sub) => format!("{label} · {}", words(&format!("{sub:?}"))),
        _ => label.into(),
    }
}

fn words(value: &str) -> String {
    let mut result = String::new();
    for (i, c) in value.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push(' ');
        }
        result.extend(c.to_lowercase());
    }
    result
}

impl App {
    pub(crate) fn navigate_relationship(&mut self, target: RelationshipTarget) -> Vec<Effect> {
        // A disclosure captures the exact graph, endpoints, direction and kind.
        // Removal or retargeting cannot turn its stale row into another action.
        let valid = self
            .graph_runtimes
            .canvas(target.graph)
            .is_some_and(|canvas| {
                relationships(target.graph, canvas.graph(), target.source)
                    .iter()
                    .any(|r| r.target == target)
            });
        if !valid {
            return vec![Effect::Redraw];
        }
        self.read_member_in_graph(target.graph, target.member)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use mere::kernel::graph::EdgeAssertion;

    fn setup() -> (App, GraphId, uuid::Uuid, uuid::Uuid) {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/a".into()));
        let pane = app.default_graph_pane();
        let a = app.graph_pane_focused_member(pane).unwrap();
        let b = app
            .with_graph_pane(pane, |canvas| {
                canvas.open_member_as_new_node(None, "https://example.test/b")
            })
            .unwrap();
        app.with_graph_pane(pane, |canvas| {
            canvas.ingest_graph(|graph| {
                let ak = graph.get_node_by_id(a).unwrap().0;
                let bk = graph.get_node_by_id(b).unwrap().0;
                for (from, to, kind) in [
                    (ak, bk, SemanticSubKind::Hyperlink),
                    (ak, bk, SemanticSubKind::Cites),
                    (bk, ak, SemanticSubKind::Quotes),
                ] {
                    mere::kernel::graph::apply::assert_relation(
                        graph,
                        from,
                        to,
                        EdgeAssertion::Semantic {
                            sub_kind: kind,
                            label: None,
                        },
                    )
                    .unwrap();
                }
                true
            });
        });
        let graph = app.graph_for_pane(pane).unwrap();
        (app, graph, a, b)
    }

    #[test]
    fn same_neighbor_preserves_each_predicate_and_direction_without_mutating_truth() {
        let (app, id, a, b) = setup();
        let graph = app.graph_runtimes.canvas(id).unwrap().graph();
        let count = graph.projected_relations().count();
        let rows = relationships(id, graph, a);
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| r.target.member == b));
        for label in ["Links to", "Cites", "Quoted by"] {
            assert!(rows.iter().any(|r| r.relationship == label));
        }
        let reverse = relationships(id, graph, b);
        for label in ["Linked from", "Cited by", "Quotes"] {
            assert!(reverse.iter().any(|r| r.relationship == label));
        }
        assert_eq!(graph.projected_relations().count(), count);
        assert_eq!(graph.nodes().count(), 2);
    }

    #[test]
    fn relationship_target_keeps_its_graph_when_member_ids_exist_in_two_graphs() {
        use crate::panes::{PaneContent, PaneId, PaneNode, SplitAxis};
        let (mut app, first, a, _) = setup();
        let second = GraphId::from_uuid(uuid::Uuid::new_v4());
        let mut clone = mere::canvas::Canvas::new();
        clone.set_graph(app.graph_runtimes.canvas(first).unwrap().graph().clone());
        app.graph_runtimes.activate_or_insert(second, None, clone);
        app.frisket.root = PaneNode::Split {
            axis: SplitAxis::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneNode::Leaf {
                pane_id: PaneId(0),
                content: PaneContent::Orrery,
                graph_id: first,
            }),
            second: Box::new(PaneNode::Leaf {
                pane_id: PaneId(100),
                content: PaneContent::Orrery,
                graph_id: second,
            }),
        };
        app.index_pane_spaces();
        for graph in [first, second] {
            let row = relationships(graph, app.graph_runtimes.canvas(graph).unwrap().graph(), a)[0]
                .clone();
            app.update(Action::NavigateRelationship(row.target));
            let owner = app.active_pane.unwrap();
            assert_eq!(app.graph_for_pane(owner), Some(graph));
            assert!(
                app.workbench_for_pane(owner)
                    .unwrap()
                    .has_tile(row.target.member)
            );
        }
    }

    #[test]
    fn relationship_navigation_reuses_member_and_refuses_wrong_graph_or_predicate() {
        let (mut app, id, a, b) = setup();
        let row = relationships(id, app.graph_runtimes.canvas(id).unwrap().graph(), a)[0].clone();
        let mut wrong = row.target;
        wrong.graph = GraphId::nil();
        assert!(
            !app.update(Action::NavigateRelationship(wrong))
                .contains(&Effect::SaveSession)
        );
        let mut wrong = row.target;
        wrong.kind = RelationKind::Semantic(SemanticSubKind::Blocks);
        assert!(
            !app.update(Action::NavigateRelationship(wrong))
                .contains(&Effect::SaveSession)
        );
        app.update(Action::NavigateRelationship(row.target));
        assert!(app.active_workbench().unwrap().has_tile(b));
        assert_eq!(
            app.graph_runtimes
                .canvas(id)
                .unwrap()
                .graph()
                .nodes()
                .count(),
            2
        );
    }
}
