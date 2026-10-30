// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Read-only relationship navigation over the kernel's canonical projection.
//! Direction and kind are retained even when several relations share a member.
use crate::{action::Effect, app::App, panes::GraphId};
#[cfg(test)]
use mere::kernel::graph::{RelationKind, SemanticSubKind};

pub type RelationshipTarget = mere::navigation::RelationshipTarget<GraphId>;
pub type RelationshipRow = mere::navigation::RelationshipRow<GraphId>;
pub use mere::navigation::relationships;

impl App {
    pub(crate) fn valid_relationship(&self, target: RelationshipTarget) -> bool {
        self.graph_runtimes
            .canvas(target.graph)
            .is_some_and(|canvas| {
                relationships(target.graph, canvas.graph(), target.source)
                    .iter()
                    .any(|row| row.target == target)
            })
    }
    pub(crate) fn navigate_relationship(&mut self, target: RelationshipTarget) -> Vec<Effect> {
        if !self.valid_relationship(target) {
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
