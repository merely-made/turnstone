// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Transient views over existing members. No pane, reading, or graph authority
//! moves into the card; a runtime incarnation prevents reuse after replacement.
use crate::{action::Effect, app::App, panes::GraphId};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
pub enum Origin {
    Node,
    Forme {
        reader: crate::panes::PaneId,
        forme: mere::forme::FormeRef,
    },
    Scope(crate::scope_navigation::ScopePlace<GraphId, Uuid>),
    Relationship(crate::relationship_navigation::RelationshipTarget),
    Inline(Uuid),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub graph: GraphId,
    pub instance: Uuid,
    pub member: Option<Uuid>,
    pub reader: Option<crate::panes::PaneId>,
    pub address: String,
    pub origin: Origin,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Peek(Target),
    Keep(Target),
    Close(Target),
    Read(Target),
    Map(Target),
    DismissPeek,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CardView {
    pub target: Target,
    pub info: cambium::FocusCardInfo,
    pub kept: bool,
    pub related: Vec<(Target, String, String)>,
    pub swatch: Option<cambium::GraphCanvasSwatch<Uuid, ()>>,
    pub forme: Option<cambium::FormeSwatch<Uuid>>,
}
impl App {
    pub(crate) fn focus_card_target(
        &self,
        graph: GraphId,
        member: Uuid,
        origin: Origin,
    ) -> Option<Target> {
        let runtime = self.graph_runtimes.get(graph)?;
        let (_, node) = runtime.canvas.graph().get_node_by_id(member)?;
        Some(Target {
            graph,
            instance: runtime.instance,
            member: Some(member),
            reader: None,
            address: node.url().into(),
            origin,
        })
    }
    pub(crate) fn valid_focus_card(&self, target: &Target) -> bool {
        let Some(runtime) = self.graph_runtimes.get(target.graph) else {
            return false;
        };
        if matches!(target.origin, Origin::Forme { .. }) {
            return runtime.instance == target.instance && self.valid_forme_card(target);
        }
        if let Origin::Scope(place) = &target.origin {
            if target
                .reader
                .is_some_and(|reader| !self.reader_matches_scope(reader, place))
            {
                return false;
            }
            if place
                .trail()
                .iter()
                .any(|p| matches!(p.level, crate::scope_navigation::ScopeLevel::Forme(_)))
                && self.scope_forme_reader(place).is_none()
            {
                return false;
            }
            return runtime.instance == target.instance
                && place.valid()
                && place.graph == target.graph
                && [place.root, place.focus]
                    .into_iter()
                    .flatten()
                    .all(|m| runtime.canvas.graph().get_node_by_id(m).is_some())
                && (place.level != crate::scope_navigation::ScopeLevel::Document
                    || place
                        .focus
                        .and_then(|m| runtime.canvas.graph().get_node_by_id(m))
                        .is_some_and(|(_, n)| n.url() == target.address));
        }
        runtime.instance == target.instance
            && target.member.is_some()
            && runtime
                .canvas
                .graph()
                .get_node_by_id(target.member.unwrap())
                .is_some_and(|(_, node)| node.url() == target.address)
            && match &target.origin {
                Origin::Relationship(relation) => self.valid_relationship(*relation),
                Origin::Inline(source) => runtime.canvas.graph().get_node_by_id(*source).is_some(),
                Origin::Node => true,
                Origin::Scope(_) | Origin::Forme { .. } => unreachable!(),
            }
    }
    pub(crate) fn lower_scope_card(&self) -> Option<Target> {
        let place = self.scope_place().down()?;
        let runtime = self.graph_runtimes.get(place.graph)?;
        Some(Target {
            graph: place.graph,
            instance: runtime.instance,
            member: place.focus,
            reader: self.scope_forme_reader(&place),
            address: place
                .focus
                .and_then(|m| runtime.canvas.graph().get_node_by_id(m))
                .map(|(_, n)| n.url().into())
                .unwrap_or_default(),
            origin: Origin::Scope(place),
        })
    }
    pub(crate) fn selected_focus_card(&self) -> Option<Target> {
        let place = self.scope_place();
        self.focus_card_target(place.graph, place.focus?, Origin::Node)
    }
    pub(crate) fn peek_inline_reference(&mut self, source: Uuid, address: &str) -> bool {
        let graph = self
            .active_pane
            .and_then(|p| self.graph_for_pane(p))
            .unwrap_or_else(|| self.graph_runtimes.active_graph());
        let member = self.graph_runtimes.canvas(graph).and_then(|c| {
            c.graph()
                .nodes()
                .find_map(|(_, n)| (n.url() == address).then_some(n.id))
        });
        let Some(target) =
            member.and_then(|member| self.focus_card_target(graph, member, Origin::Inline(source)))
        else {
            return false;
        };
        self.focus_card_command(Command::Peek(target));
        true
    }
    pub(crate) fn focus_card_command(&mut self, command: Command) -> Vec<Effect> {
        if command == Command::DismissPeek {
            self.reading.cards.peek = None;
            return vec![Effect::Redraw];
        }
        let target = match &command {
            Command::Peek(t)
            | Command::Keep(t)
            | Command::Close(t)
            | Command::Read(t)
            | Command::Map(t) => t,
            _ => unreachable!(),
        };
        if matches!(command, Command::Close(_)) {
            self.reading.cards.close(target);
            return vec![Effect::Redraw];
        }
        if !self.valid_focus_card(target) {
            self.reading.cards.close(target);
            return vec![Effect::Redraw];
        }
        let shown = self.reading.cards.cards().iter().any(|(t, _)| t == target)
            || self.focus_card_views().iter().any(|card| {
                card.forme.is_some() && card.related.iter().any(|(t, _, _)| t == target)
            });
        match command {
            Command::Peek(target) => self.reading.cards.show(target),
            Command::Keep(target) => self.reading.cards.keep(&target),
            Command::Read(target) if shown => {
                self.reading.cards.peek = None;
                if let Origin::Scope(place) = target.origin {
                    return self.scope_command_for_reader(
                        crate::scope_navigation::ScopeCommand::To(
                            crate::scope_navigation::ScopeDestination::Place(place),
                        ),
                        target.reader,
                    );
                }
                return self.read_member_in_graph(target.graph, target.member.unwrap());
            },
            Command::Map(target) if shown => {
                self.reading.cards.peek = None;
                if let Some(pane) = self.graph_pane_for_navigation(target.graph) {
                    self.reveal_pane(pane);
                    if let Some(member) = target.member {
                        self.graph_pane_select_member(pane, member);
                    }
                    self.with_graph_pane(pane, |c| c.center_on_selected());
                    self.focus = crate::surface::FocusTarget::Graph(pane);
                }
                return vec![Effect::Redraw];
            },
            _ => {},
        }
        vec![Effect::Redraw]
    }
    pub(crate) fn focus_card_views(&self) -> Vec<CardView> {
        self.reading
            .cards
            .cards()
            .into_iter()
            .enumerate()
            .filter_map(|(index, (target, kept))| {
                if !self.valid_focus_card(&target) {
                    return None;
                }
                if matches!(target.origin, Origin::Forme { .. }) {
                    return self.forme_card_view(target, kept);
                }
                let graph = self.graph_runtimes.canvas(target.graph)?.graph();
                if let Origin::Scope(place) = &target.origin {
                    if place.level != crate::scope_navigation::ScopeLevel::Document {
                        let mut members = mere::navigation::scope_members(graph, place)?;
                        let total = members.len();
                        members.sort();
                        if let Some(i) = members.iter().position(|m| Some(*m) == place.focus) {
                            members.swap(0, i);
                        }
                        members.truncate(32);
                        let edges = graph
                            .projected_relations()
                            .filter_map(|(_, r)| {
                                let from = graph.get_node(r.from)?.id;
                                let to = graph.get_node(r.to)?.id;
                                (members.contains(&from) && members.contains(&to))
                                    .then_some((from, to))
                            })
                            .collect();
                        let nodes = members
                            .iter()
                            .filter_map(|m| {
                                graph
                                    .get_node_by_id(*m)
                                    .map(|(k, n)| (*m, graph.node_display_label(k), n.url().into()))
                            })
                            .collect();
                        let title = self.scope_label(place);
                        let swatch = cambium::context_graph_swatch(
                            0x7300_0000 + index as u64,
                            nodes,
                            edges,
                            place.focus,
                            title.clone(),
                        );
                        let info = cambium::FocusCardInfo {
                            title,
                            subtitle: format!("{} objects · {} shown", total, members.len()),
                            context: format!("Scope below · {}", place.level.label()),
                            preview_status: "Live scoped graph".into(),
                            excerpt: None,
                            primary: Some("Enter".into()),
                            secondary: None,
                        };
                        let related = members
                            .iter()
                            .filter_map(|m| {
                                let mut selected = place.clone();
                                selected.select(*m);
                                self.focus_card_target(
                                    target.graph,
                                    *m,
                                    Origin::Scope(selected.down()?),
                                )
                                .map(|t| (t, String::new(), String::new()))
                            })
                            .collect();
                        return Some(CardView {
                            target,
                            kept,
                            info,
                            forme: None,
                            swatch: Some(swatch),
                            related,
                        });
                    }
                }
                let member = target.member?;
                let (key, _) = graph.get_node_by_id(member)?;
                let rows =
                    crate::relationship_navigation::relationships(target.graph, graph, member);
                let excerpt = self
                    .reading
                    .anchors
                    .iter()
                    .find(|(key, anchor)| {
                        anchor.source == target.address
                            && key.member == member
                            && self.graph_for_pane(key.pane) == Some(target.graph)
                    })
                    .map(|(_, anchor)| anchor.exact.chars().take(420).collect());
                let context = match &target.origin {
                    Origin::Node => "Selected object · Document".into(),
                    Origin::Forme { .. } => unreachable!(),
                    Origin::Scope(place) => format!("Scope below · {}", place.level.label()),
                    Origin::Inline(source) => format!(
                        "Reference from {}",
                        graph
                            .get_node_by_id(*source)
                            .map(|(k, _)| graph.node_display_label(k))
                            .unwrap_or_else(|| "document".into())
                    ),
                    Origin::Relationship(relation) => {
                        crate::relationship_navigation::relationships(
                            target.graph,
                            graph,
                            relation.source,
                        )
                        .into_iter()
                        .find(|r| r.target == *relation)
                        .map(|r| r.relationship)
                        .unwrap_or_else(|| "Relationship".into())
                    },
                };
                let related = rows
                    .into_iter()
                    .take(3)
                    .filter_map(|row| {
                        self.focus_card_target(
                            target.graph,
                            row.target.member,
                            Origin::Relationship(row.target),
                        )
                        .map(|t| (t, row.title, row.relationship))
                    })
                    .collect();
                let info = cambium::FocusCardInfo {
                    title: graph.node_display_label(key),
                    subtitle: target.address.clone(),
                    context,
                    preview_status: if excerpt.is_some() {
                        "Last retained reading passage".into()
                    } else {
                        "No retained passage available".into()
                    },
                    excerpt,
                    primary: Some(
                        if matches!(target.origin, Origin::Scope(_)) {
                            "Enter"
                        } else {
                            "Read"
                        }
                        .into(),
                    ),
                    secondary: Some("Show in Map".into()),
                };
                Some(CardView {
                    target,
                    info,
                    kept,
                    related,
                    forme: None,
                    swatch: None,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        action::{Action, Update},
        reading_view::ReadTarget,
    };
    fn fixture() -> (App, GraphId, Uuid, Uuid) {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://card.test/source".into()));
        let pane = app.default_graph_pane();
        let source = app.graph_pane_focused_member(pane).unwrap();
        let member = app
            .with_graph_pane(pane, |c| {
                c.open_member_as_new_node(Some(source), "https://card.test/reference")
            })
            .unwrap();
        let graph = app.graph_for_pane(pane).unwrap();
        app.update(Action::ReadGraphMember(source));
        let reader = app.active_pane.unwrap();
        app.reading.anchors.insert(
            ReadTarget {
                pane: reader,
                member: source,
            },
            tabard::ReadingAnchor {
                source: "https://card.test/source".into(),
                revision: 1,
                text: 2,
                offset: 3,
                exact: "keep this passage".into(),
                prefix: String::new(),
                suffix: String::new(),
                line_offset: 0.,
                fallback_y: 480.,
            },
        );
        (app, graph, source, member)
    }
    #[test]
    fn forme_preview_prefers_the_active_then_retained_reader_over_tree_order() {
        let (mut app, graph, _, _) = fixture();
        let reader = app.active_pane.unwrap();
        let other = crate::panes::PaneId(900);
        app.frisket.root = crate::panes::PaneNode::Split {
            axis: crate::panes::SplitAxis::Horizontal, ratio: 0.5,
            first: Box::new(crate::panes::PaneNode::Leaf {
                pane_id: other, content: crate::panes::PaneContent::Workbench, graph_id: graph,
            }),
            second: Box::new(app.frisket.root.clone()),
        };
        app.index_pane_spaces();
        assert_eq!(app.forme_card_target().unwrap().reader, Some(reader));
        app.update(Action::SetWorkspaceMap(true));
        assert_eq!(app.forme_card_target().unwrap().reader, Some(reader));
    }
    #[test]
    fn forme_peek_enters_existing_access_and_round_trips_without_rearrangement() {
        use crate::scope_navigation::{ScopeCommand, ScopeLevel};
        use cambium::scope_navigation::ScopeDirection;
        let (mut app, graph, source, member) = fixture();
        let reader = app.active_pane.unwrap();
        app.workbench_for_pane_mut(reader)
            .unwrap()
            .open_split(&[source, member]);
        let layout = app.workbench_for_pane(reader).unwrap().to_arrangement().1;
        let anchors = app.reading.anchors.clone();
        app.update(Action::SetWorkspaceMap(true));
        let map = app.active_pane.unwrap();
        let camera = app.with_graph_pane(map, |c| c.camera()).unwrap();
        let history = serde_json::to_value(&app.reading.scope.navigation).unwrap();
        let target = app.forme_card_target().unwrap();
        app.update(Action::FocusCard(Command::Peek(target.clone())));
        app.update(Action::FocusCard(Command::Keep(target.clone())));
        assert_eq!(
            serde_json::to_value(&app.reading.scope.navigation).unwrap(),
            history
        );
        assert_eq!(app.reading.anchors, anchors);
        assert_eq!(
            app.workbench_for_pane(reader).unwrap().to_arrangement().1,
            layout
        );
        let card = app.focus_card_views().pop().unwrap();
        assert_eq!(card.forme.as_ref().unwrap().cells.len(), 2);
        let child = card
            .related
            .iter()
            .find(|(t, _, _)| t.member == Some(source))
            .unwrap()
            .0
            .clone();
        app.update(Action::FocusCard(Command::Read(child)));
        assert_eq!(app.active_pane, Some(reader));
        assert_eq!(app.scope_place().focus, Some(source));
        assert!(matches!(
            app.scope_place().up().unwrap().level,
            ScopeLevel::Forme(_)
        ));
        assert_eq!(app.reading.anchors, anchors);
        app.update(Action::Scope(ScopeCommand::Navigate(ScopeDirection::Up)));
        assert!(matches!(app.scope_place().level, ScopeLevel::Forme(_)));
        app.update(Action::Scope(ScopeCommand::Navigate(ScopeDirection::Up)));
        assert_eq!(app.scope_place().level, ScopeLevel::Graph);
        assert_eq!(app.scope_place().graph, graph);
        assert_eq!(app.with_graph_pane(map, |c| c.camera()).unwrap(), camera);
        app.update(Action::Scope(ScopeCommand::Navigate(ScopeDirection::Back)));
        assert!(matches!(app.scope_place().level, ScopeLevel::Forme(_)));
        app.update(Action::Scope(ScopeCommand::Navigate(ScopeDirection::Back)));
        assert_eq!(app.scope_place().focus, Some(source));
        assert_eq!(app.scope_place().level, ScopeLevel::Document);
        assert_eq!(
            app.workbench_for_pane(reader).unwrap().to_arrangement().1,
            layout
        );
        assert_eq!(app.reading.anchors, anchors);
    }
    #[test]
    fn kept_forme_card_reads_live_layout_and_rejects_a_removed_tile_target() {
        let (mut app, _, source, member) = fixture();
        let reader = app.active_pane.unwrap();
        app.workbench_for_pane_mut(reader)
            .unwrap()
            .open_split(&[source, member]);
        let target = app.forme_card_target().unwrap();
        app.update(Action::FocusCard(Command::Peek(target.clone())));
        app.update(Action::FocusCard(Command::Keep(target)));
        let stale = app.focus_card_views()[0]
            .related
            .iter()
            .find(|(t, _, _)| t.member == Some(member))
            .unwrap()
            .0
            .clone();
        app.workbench_for_pane_mut(reader)
            .unwrap()
            .close_tile(member);
        let before = app.scope_place();
        app.update(Action::FocusCard(Command::Read(stale)));
        assert_eq!(app.scope_place(), before);
        assert!(!app.workbench_for_pane(reader).unwrap().has_tile(member));
        assert_eq!(
            app.focus_card_views()[0]
                .forme
                .as_ref()
                .unwrap()
                .cells
                .len(),
            1
        );
    }

    #[test]
    fn lower_scope_peek_uses_downs_destination_without_visiting_it() {
        use crate::scope_navigation::{ScopeCommand, ScopeLevel};
        use cambium::scope_navigation::ScopeDirection;
        let (mut app, _, _, _) = fixture();
        app.update(Action::SetWorkspaceMap(true));
        let before = app.scope_place();
        let history = serde_json::to_value(&app.reading.scope.navigation).unwrap();
        let anchors = app.reading.anchors.clone();
        let target = app.lower_scope_card().unwrap();
        assert_eq!(target.origin, Origin::Scope(before.down().unwrap()));
        app.update(Action::FocusCard(Command::Peek(target.clone())));
        app.update(Action::FocusCard(Command::Keep(target.clone())));
        let views = app.focus_card_views();
        let swatch = views[0]
            .swatch
            .as_ref()
            .expect("a graph-bearing destination uses a swatch");
        assert_eq!(swatch.graph.nodes.len(), 2);
        for (child, _, _) in &views[0].related {
            let Origin::Scope(place) = &child.origin else {
                panic!("swatch children retain their contextual route")
            };
            assert_eq!(place.level, ScopeLevel::Document);
            assert_eq!(place.root, before.focus);
        }
        assert_eq!(app.scope_place(), before);
        assert_eq!(
            serde_json::to_value(&app.reading.scope.navigation).unwrap(),
            history
        );
        assert_eq!(app.reading.anchors, anchors);
        let pane = app.graph_pane_for_navigation(before.graph).unwrap();
        app.with_graph_pane(pane, |canvas| {
            canvas.open_member_as_new_node(before.focus, "https://card.test/new-neighbor");
        });
        app.graph_pane_select_member(pane, before.focus.unwrap());
        assert_eq!(
            app.focus_card_views()[0]
                .swatch
                .as_ref()
                .unwrap()
                .graph
                .nodes
                .len(),
            3,
            "keeping a scope holds its binding while membership stays live"
        );
        app.update(Action::FocusCard(Command::Read(target)));
        assert_eq!(app.scope_place(), before.down().unwrap());
        assert_eq!(app.scope_place().level, ScopeLevel::Neighborhood);
        let reading = app.lower_scope_card().unwrap();
        app.update(Action::FocusCard(Command::Peek(reading.clone())));
        assert!(app.focus_card_views().last().unwrap().swatch.is_none());
        assert_eq!(app.scope_place().level, ScopeLevel::Neighborhood);
        app.update(Action::FocusCard(Command::Read(reading)));
        assert_eq!(app.scope_place().level, ScopeLevel::Document);
        assert!(app.lower_scope_card().is_none());
        // History is still the ordinary compass history.
        app.update(Action::Scope(ScopeCommand::Navigate(ScopeDirection::Back)));
        assert_eq!(app.scope_place().level, ScopeLevel::Neighborhood);
    }

    #[test]
    fn node_relationship_and_inline_peeks_do_not_navigate_or_rearrange_the_reading() {
        let (mut app, graph, source, member) = fixture();
        let origin = app.scope_place();
        let focus = app.focus;
        let history = serde_json::to_value(&app.reading.scope.navigation).unwrap();
        let anchors = app.reading.anchors.clone();
        let reader = app.active_pane.unwrap();
        let members = app
            .workbench_for_pane(reader)
            .unwrap()
            .slot_views()
            .flat_map(|s| s.members.to_vec())
            .collect::<Vec<_>>();
        let relation = crate::relationship_navigation::relationships(
            graph,
            app.graph_runtimes.canvas(graph).unwrap().graph(),
            source,
        )
        .into_iter()
        .find(|r| r.target.member == member)
        .unwrap()
        .target;
        for origin in [
            Origin::Node,
            Origin::Relationship(relation),
            Origin::Inline(source),
        ] {
            let target = app.focus_card_target(graph, member, origin).unwrap();
            app.update(Action::FocusCard(Command::Peek(target.clone())));
            app.update(Action::FocusCard(Command::Keep(target.clone())));
            assert_eq!(app.focus, focus);
            assert_eq!(app.reading.anchors, anchors);
            assert_eq!(
                serde_json::to_value(&app.reading.scope.navigation).unwrap(),
                history
            );
            assert_eq!(
                app.workbench_for_pane(reader)
                    .unwrap()
                    .slot_views()
                    .flat_map(|s| s.members.to_vec())
                    .collect::<Vec<_>>(),
                members
            );
        }
        assert_eq!(app.scope_place(), origin);
        assert_eq!(app.focus_card_views().len(), 3);
        assert_eq!(
            app.graph_runtimes
                .canvas(graph)
                .unwrap()
                .graph()
                .node_count(),
            2
        );
        let target = app.reading.cards.kept[0].clone();
        app.update(Action::FocusCard(Command::Read(target)));
        assert_eq!(app.scope_place().focus, Some(member));
        assert_eq!(app.reading.anchors, anchors);
    }
    #[test]
    fn a_saved_passage_from_another_address_is_never_presented_as_this_preview() {
        let (mut app, graph, source, _) = fixture();
        let target = app.focus_card_target(graph, source, Origin::Node).unwrap();
        app.update(Action::FocusCard(Command::Peek(target)));
        assert!(app.focus_card_views()[0].info.excerpt.is_some());
        for anchor in app.reading.anchors.values_mut() {
            anchor.source = "https://card.test/another-revision-source".into();
        }
        assert!(app.focus_card_views()[0].info.excerpt.is_none());
    }

    #[test]
    fn kept_cards_survive_other_peeks_but_cannot_act_after_runtime_replacement() {
        let (mut app, graph, source, member) = fixture();
        let kept = app.focus_card_target(graph, member, Origin::Node).unwrap();
        app.update(Action::FocusCard(Command::Peek(kept.clone())));
        app.update(Action::FocusCard(Command::Keep(kept.clone())));
        let peek = app.focus_card_target(graph, source, Origin::Node).unwrap();
        app.update(Action::FocusCard(Command::Peek(peek.clone())));
        app.update(Action::FocusCard(Command::Close(peek)));
        assert_eq!(app.focus_card_views().len(), 1);
        let clone = app.graph_runtimes.canvas(graph).unwrap().graph().clone();
        app.graph_runtimes.set_graph(clone);
        assert!(!app.valid_focus_card(&kept));
        assert!(app.focus_card_views().is_empty());
        let before = app.scope_place();
        app.update(Action::FocusCard(Command::Read(kept)));
        assert_eq!(app.scope_place(), before);
    }
}
