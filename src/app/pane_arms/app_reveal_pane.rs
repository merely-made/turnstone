// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    /// Named navigation must reveal its destination, including from a maximized
    /// reader. Focus still belongs to the destination's graph or pane surface.
    pub(crate) fn reveal_pane(&mut self, pane: PaneId) {
        if let Some(space) = self.space_of(pane)
            && self.ensure_shared_space(space).is_ok()
            && let Some(layout) = self.space_mut(space)
        {
            let _ = layout.apply_workspace(
                &workbench::TileEvent::Activated(workbench::TileId(pane.0)).into(),
            );
        }
        self.active_pane = Some(pane);
        if self.maximized.is_some() {
            self.maximized = Some(pane);
        }
    }

    pub(crate) fn primary_graph_for_navigation(&self, member: Option<Uuid>) -> Option<PaneId> {
        let graph = member
            .and_then(|node| self.graph_runtimes.graph_containing_member(node))
            .or_else(|| self.active_pane.and_then(|pane| self.graph_for_pane(pane)))
            .unwrap_or_else(|| self.graph_runtimes.active_graph());
        self.graph_pane_for_navigation(graph)
    }

    /// An explicit graph source must survive navigation even when another
    /// loaded graph has members with the same identities.
    pub(crate) fn graph_pane_for_navigation(&self, graph: GraphId) -> Option<PaneId> {
        let source = self.active_pane.and_then(|pane| self.forme_for_pane(pane));
        let candidates = self
            .frisket
            .iter_leaves()
            .chain(
                self.lenses
                    .iter()
                    .flatten()
                    .flat_map(|space| space.iter_leaves()),
            )
            .filter(|(_, content, id)| matches!(content, PaneContent::Orrery) && *id == graph)
            .map(|(pane, _, _)| pane)
            .collect::<Vec<_>>();
        if let Some(pane) = self.active_pane.filter(|p| candidates.contains(p)) {
            return Some(pane);
        }
        candidates
            .iter()
            .copied()
            .find(|pane| self.forme_for_pane(*pane) == source)
            .or_else(|| candidates.first().copied())
    }

    pub(in super::super) fn set_workspace_map(&mut self, map: bool) -> Vec<Effect> {
        let member = if let Some(pane) = self.focused_graph_pane() {
            self.graph_pane_focused_member(pane)
        } else {
            self.browser_command_target().map(|(member, _)| member)
        };
        let Some(pane) = self.primary_graph_for_navigation(member) else {
            return vec![Effect::Redraw];
        };
        self.reveal_pane(pane);
        if let Some(member) = member {
            self.graph_pane_select_member(pane, member);
        }
        self.focus = crate::surface::FocusTarget::Graph(pane);
        if map {
            // Keep a useful framing and its zoom. A newly read member can be
            // outside that view; reveal it without fitting the entire graph.
            self.with_graph_pane(pane, |canvas| {
                let view = canvas.viewport();
                if canvas.focused_screen_position().is_some_and(|(x, y)| {
                    x < 24.0
                        || y < 24.0
                        || x > view.view.0 as f32 - 24.0
                        || y > view.view.1 as f32 - 24.0
                }) {
                    canvas.center_on_selected();
                }
            });
            vec![Effect::SaveSession, Effect::Redraw]
        } else if member.is_some() {
            self.read_graph_member(member.unwrap())
        } else {
            vec![Effect::Redraw]
        }
    }

    pub(in super::super) fn read_graph_member(&mut self, member: Uuid) -> Vec<Effect> {
        let Some(graph) = self.graph_runtimes.graph_containing_member(member) else {
            return vec![Effect::Redraw];
        };
        self.read_member_in_graph(graph, member)
    }

    pub(crate) fn read_member_in_graph(&mut self, graph: GraphId, member: Uuid) -> Vec<Effect> {
        self.capture_scope_origin();
        if !self
            .graph_runtimes
            .canvas(graph)
            .is_some_and(|canvas| canvas.graph().get_node_by_id(member).is_some())
        {
            return vec![Effect::Redraw];
        }
        let Some(pane) = self.graph_pane_for_navigation(graph) else {
            return vec![Effect::Redraw];
        };
        // Read arranges members of the map. Visiting another member uses the
        // current reading stack; an existing member keeps its arranged cell.
        let reading = self
            .frisket
            .iter_leaves()
            .chain(
                self.lenses
                    .iter()
                    .flatten()
                    .flat_map(|space| space.iter_leaves()),
            )
            .find_map(|(id, content, _)| {
                (matches!(content, PaneContent::Workbench)
                    && self.forme_for_pane(id) == self.forme_for_pane(pane))
                .then_some(id)
            });
        let target = reading
            .and_then(|id| {
                self.pane_context.resolve_context(
                    id,
                    &crate::panes::ContextBinding::Own,
                    crate::panes::SourceSelector::Graph,
                )
            })
            .and_then(|context| context.member)
            .or_else(|| {
                self.workbench_for_pane(pane).and_then(|workbench| {
                    workbench
                        .slot_views()
                        .find_map(|slot| slot.members.get(slot.active).copied())
                })
            });
        if let Some(workbench) = self.workbench_for_pane_mut(pane)
            && !workbench.has_tile(member)
            && let Some(target) = target
        {
            workbench.open_in_slot_of(member, target);
        }
        self.reveal_pane(pane);
        self.graph_pane_select_member(pane, member);
        self.focus = crate::surface::FocusTarget::Graph(pane);
        let mut effects = self.open_in_workbench();
        // The origin was captured before revealing the destination. Running the
        // public action wrapper here would capture the newly revealed reading
        // again and overwrite the history entry we are leaving.
        effects.extend(self.activate_workbench_member(member));
        effects
    }

    pub(in super::super) fn activate_workbench_member(&mut self, member: Uuid) -> Vec<Effect> {
        let owner = self.workbench_owner_pane();
        let activated = owner
            .and_then(|pane| self.workbench_for_pane_mut(pane))
            .is_some_and(|workbench| workbench.activate(member));
        if !activated {
            return vec![Effect::Redraw];
        }
        if let Some(owner) = owner {
            self.publish_member_context(owner, Some(member));
            self.record_scope_reading(owner, member);
            if let Some(graph) = self.graph_for_pane(owner) {
                self.reading
                    .trails
                    .entry(owner)
                    .or_default()
                    .visit(crate::reading_view::ReadVisit { graph, member });
            }
        }
        vec![
            Effect::SaveSession,
            Effect::SaveReadingPreferences,
            Effect::Redraw,
        ]
    }

    pub(crate) fn read_visit_in_pane(&mut self, pane: PaneId, member: Uuid) -> Vec<Effect> {
        if let Some(workbench) = self.workbench_for_pane_mut(pane) {
            if !workbench.has_tile(member) {
                let current = workbench
                    .slot_views()
                    .find_map(|slot| slot.members.get(slot.active).copied());
                if let Some(current) = current {
                    workbench.open_in_slot_of(member, current);
                } else {
                    workbench.open_tile(member);
                }
            }
            workbench.activate(member);
        }
        self.reveal_pane(pane);
        self.publish_member_context(pane, Some(member));
        self.focus = crate::surface::FocusTarget::Pane(pane);
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(crate) fn reveal_reading_on_graph(
        &mut self,
        target: crate::reading_view::ReadTarget,
    ) -> Vec<Effect> {
        let Some(graph) = self.graph_for_pane(target.pane) else {
            return vec![Effect::Redraw];
        };
        let Some(pane) = self.graph_pane_for_navigation(graph) else {
            return vec![Effect::Redraw];
        };
        self.reveal_pane(pane);
        self.graph_pane_select_member(pane, target.member);
        self.with_graph_pane(pane, |canvas| canvas.center_on_selected());
        self.focus = crate::surface::FocusTarget::Graph(pane);
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(crate) fn reveal_relationship(
        &mut self,
        target: crate::relationship_navigation::RelationshipTarget,
    ) -> Vec<Effect> {
        if !self.valid_relationship(target) {
            return vec![Effect::Redraw];
        }
        let Some(pane) = self.graph_pane_for_navigation(target.graph) else {
            return vec![Effect::Redraw];
        };
        self.reveal_pane(pane);
        self.graph_pane_select_member(pane, target.member);
        self.with_graph_pane(pane, |canvas| canvas.center_on_selected());
        self.focus = crate::surface::FocusTarget::Graph(pane);
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(in super::super) fn reveal_graph_member(&mut self, member: Uuid) -> Vec<Effect> {
        if self
            .graph_runtimes
            .graph_containing_member(member)
            .is_none()
        {
            return vec![Effect::Redraw];
        }
        let Some(pane) = self.primary_graph_for_navigation(Some(member)) else {
            return vec![Effect::Redraw];
        };
        self.reveal_pane(pane);
        self.graph_pane_select_member(pane, member);
        self.with_graph_pane(pane, |canvas| canvas.center_on_selected());
        self.focus = crate::surface::FocusTarget::Graph(pane);
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(in super::super) fn toggle_tool_pane(&mut self, kind: PaneKindId) -> Vec<Effect> {
        // Main views have a dedicated switch; tool toggling never closes them.
        if matches!(kind.as_str(), kind::GRAPH | kind::WORKBENCH) {
            return vec![Effect::Redraw];
        }
        let existing = self
            .frisket
            .iter_leaves()
            .find_map(|(pane, content, _)| (content.kind_id() == kind).then_some(pane));
        let map = self.map_navigation_active();
        let main = self
            .frisket
            .iter_leaves()
            .find_map(|(pane, content, _)| {
                (if map {
                    matches!(content, PaneContent::Orrery)
                } else {
                    matches!(content, PaneContent::Workbench)
                })
                .then_some(pane)
            })
            .unwrap_or_else(|| self.default_graph_pane());
        let visible = existing.is_some_and(|pane| {
            self.frisket
                .outer_workspace()
                .ok()
                .is_some_and(|frame| frame.active_ids().contains(&workbench::TileId(pane.0)))
        });
        let Some(pane) = existing else {
            self.reveal_pane(main);
            return self.summon_pane(kind);
        };
        let Some(frame) = self.frisket.workspace_mut() else {
            return vec![Effect::Redraw];
        };
        let target = if visible {
            let Some(path) = crate::panes::workspace_bridge::path_for(
                frame.workspace.tiled(),
                workbench::TileId(main.0),
            ) else {
                return vec![Effect::Redraw];
            };
            let index =
                match crate::panes::workspace_bridge::tree_at(frame.workspace.tiled(), &path) {
                    Some(workbench::TileTree::Stack(stack)) => stack.tabs.len(),
                    _ => 0,
                };
            workbench::DropTarget::Stack {
                stack: crate::panes::workspace_bridge::path_indices(&path),
                index,
            }
        } else {
            workbench::DropTarget::Edge {
                tile: workbench::TileId(main.0),
                edge: workbench::Edge::Right,
            }
        };
        let _ = frame.apply(
            &workbench::TileEvent::Dragged {
                tile: workbench::TileId(pane.0),
                to: target,
            }
            .into(),
        );
        self.reveal_pane(if visible { main } else { pane });
        self.index_pane_spaces();
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(in super::super) fn summon_pane(&mut self, kind: PaneKindId) -> Vec<Effect> {
        let Some(definition) = crate::panes::pane_definition(kind.as_str()) else {
            self.events.push(AppEvent::InteractionMissed {
                what: "summon-pane",
                target: kind.0,
            });
            return vec![Effect::Redraw];
        };
        let Some(content) = crate::panes::legacy_pane_content(&kind) else {
            self.events.push(AppEvent::InteractionMissed {
                what: "summon-pane",
                target: definition.display_name.into(),
            });
            return vec![Effect::Redraw];
        };
        let id = PaneId(self.next_pane_id);
        // Anchor on the active pane IN ITS OWN SPACE (a pane torn out
        // to a lens summons its neighbors there — the window as pane
        // host), else the primary Orrery (graph) leaf — meerkat's
        // fixed Right-split off the graph pane, generalized.
        let (space, anchor) = match self
            .active_pane
            .and_then(|a| self.space_of(a).map(|s| (s, a)))
        {
            Some((s, a)) => (s, Some(a)),
            None => (
                SpaceRef::Primary,
                self.frisket
                    .iter_leaves()
                    .find(|(_, c, _)| matches!(c, PaneContent::Orrery))
                    .map(|(id, _, _)| id),
            ),
        };
        if let Err(reason) = self.ensure_shared_space(space) {
            self.events.push(AppEvent::InteractionMissed {
                what: "summon-pane",
                target: reason,
            });
            return vec![Effect::Redraw];
        }
        let existing = (definition.multiplicity != PaneMultiplicity::Many)
            .then(|| {
                self.space(space)?
                    .iter_leaves()
                    .find_map(|(id, existing, _)| {
                        (existing.kind_id().as_str() == kind.as_str()).then_some(id)
                    })
            })
            .flatten();
        if let Some(existing) = existing {
            self.reveal_pane(existing);
            return vec![Effect::Redraw];
        }
        let graph_id = content
            .follows_active_graph()
            .then(|| {
                anchor
                    .and_then(|pane| self.graph_for_pane(pane))
                    .or_else(|| {
                        self.focused_graph_pane()
                            .and_then(|pane| self.graph_for_pane(pane))
                    })
                    .unwrap_or_else(|| self.graph_runtimes.active_graph())
            })
            .unwrap_or_else(GraphId::nil);
        let Some(layout) = self.space_mut(space) else {
            return vec![Effect::Redraw];
        };
        let anchor_path = anchor
            .and_then(|a| crate::pane::path_of(layout, a))
            .unwrap_or_default();
        let new_leaf = PaneNode::Leaf {
            pane_id: id,
            content,
            graph_id,
        };
        if layout.summon_leaf(&anchor_path, InsertSide::Right, new_leaf) {
            self.next_pane_id += 1;
            self.reveal_pane(id);
            if kind.as_str() == kind::INSPECTOR {
                self.set_active_pane_divider(0.70);
            }
            self.index_pane_spaces();
            self.events
                .push(AppEvent::PaneSummoned(definition.display_name));
            vec![Effect::SaveSession, Effect::Redraw]
        } else {
            vec![Effect::Redraw]
        }
    }

    pub(in super::super) fn move_pane_section(
        &mut self,
        pane: PaneId,
        section: String,
        delta: i32,
    ) -> Vec<Effect> {
        // Order IS the config's order, so a move is the same leaf edit
        // as add/remove. Clamped at the ends: a stack has a top and a
        // bottom, and silently wrapping would be a surprise.
        let Some(space) = self.space_of(pane) else {
            return vec![Effect::Redraw];
        };
        let Some(layout) = self.space_mut(space) else {
            return vec![Effect::Redraw];
        };
        let mut moved = false;
        if let Some(cfg) = layout.content_mut(pane).and_then(|c| c.composition_mut())
            && let Some(from) = cfg.sections.iter().position(|s| s == &section)
        {
            let to = (from as i32 + delta).clamp(0, cfg.sections.len() as i32 - 1) as usize;
            if to != from {
                let id = cfg.sections.remove(from);
                cfg.sections.insert(to, id);
                moved = true;
            }
        }
        if moved {
            self.events.push(AppEvent::PaneSectionMoved(section));
            vec![Effect::SaveSession, Effect::Redraw]
        } else {
            vec![Effect::Redraw]
        }
    }
}
