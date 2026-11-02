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
    EnterLayout(Target),
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
            | Command::EnterLayout(t)
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
            Command::EnterLayout(target) => {
                if let Some(place) = self.forme_field_scope(&target) {
                    self.reading.cards.peek = None;
                    return self.scope_command_for_reader(
                        crate::scope_navigation::ScopeCommand::To(
                            crate::scope_navigation::ScopeDestination::Place(place),
                        ),
                        target.reader,
                    );
                }
            },
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
                        "No preview available".into()
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
#[path = "focus_card/tests.rs"]
mod tests;
