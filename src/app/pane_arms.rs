// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Pane-ring arms: arrangement edits.
//!
//! Every one lands on a leaf in whichever space holds it, primary or lens, so
//! the same op works in any window and persists with that space.

use uuid::Uuid;

use crate::action::{Action, Effect, SpaceRef, WbAxis};
use crate::observe::AppEvent;
use crate::panes::{
    GraphId, InsertSide, PaneContent, PaneId, PaneKindId, PaneMultiplicity, PaneNode, kind,
};

use super::App;

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
        if let Some(pane)=self.active_pane.filter(|p|candidates.contains(p)) {return Some(pane);}
        candidates
            .iter()
            .copied()
            .find(|pane| self.forme_for_pane(*pane) == source)
            .or_else(|| candidates.first().copied())
    }

    pub(super) fn set_workspace_map(&mut self, map: bool) -> Vec<Effect> {
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
                    x < 24.0 || y < 24.0
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

    pub(super) fn read_graph_member(&mut self, member: Uuid) -> Vec<Effect> {
        let Some(graph) = self.graph_runtimes.graph_containing_member(member) else {
            return vec![Effect::Redraw];
        };
        self.read_member_in_graph(graph, member)
    }

    pub(crate) fn read_member_in_graph(&mut self, graph: GraphId, member: Uuid) -> Vec<Effect> {
        self.capture_scope_origin();
        if !self.graph_runtimes.canvas(graph).is_some_and(|canvas| canvas.graph().get_node_by_id(member).is_some()) {
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

    pub(super) fn activate_workbench_member(&mut self, member: Uuid) -> Vec<Effect> {
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
                self.reading.trails.entry(owner).or_default()
                    .visit(crate::reading_view::ReadVisit { graph, member });
            }
        }
        vec![Effect::SaveSession, Effect::SaveReadingPreferences, Effect::Redraw]
    }

    pub(crate) fn read_visit_in_pane(&mut self,pane:PaneId,member:Uuid)->Vec<Effect> {
        if let Some(workbench)=self.workbench_for_pane_mut(pane) {
            if !workbench.has_tile(member) {
                let current=workbench.slot_views().find_map(|slot|slot.members.get(slot.active).copied());
                if let Some(current)=current {workbench.open_in_slot_of(member,current);} else {workbench.open_tile(member);}
            }
            workbench.activate(member);
        }
        self.reveal_pane(pane);self.publish_member_context(pane,Some(member));
        self.focus=crate::surface::FocusTarget::Pane(pane);
        vec![Effect::SaveSession,Effect::Redraw]
    }
    pub(crate) fn reveal_reading_on_graph(&mut self,target:crate::reading_view::ReadTarget)->Vec<Effect> {
        let Some(graph)=self.graph_for_pane(target.pane) else {return vec![Effect::Redraw];};
        let Some(pane)=self.graph_pane_for_navigation(graph) else {return vec![Effect::Redraw];};
        self.reveal_pane(pane);self.graph_pane_select_member(pane,target.member);
        self.with_graph_pane(pane,|canvas|canvas.center_on_selected());
        self.focus=crate::surface::FocusTarget::Graph(pane);
        vec![Effect::SaveSession,Effect::Redraw]
    }

    pub(crate) fn reveal_relationship(&mut self,target:crate::relationship_navigation::RelationshipTarget)->Vec<Effect> {
        if !self.valid_relationship(target) {return vec![Effect::Redraw];}
        let Some(pane)=self.graph_pane_for_navigation(target.graph) else {return vec![Effect::Redraw];};
        self.reveal_pane(pane); self.graph_pane_select_member(pane,target.member);
        self.with_graph_pane(pane,|canvas|canvas.center_on_selected());
        self.focus=crate::surface::FocusTarget::Graph(pane);
        vec![Effect::SaveSession,Effect::Redraw]
    }

    pub(super) fn reveal_graph_member(&mut self, member: Uuid) -> Vec<Effect> {
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
    pub(super) fn toggle_tool_pane(&mut self, kind: PaneKindId) -> Vec<Effect> {
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

    pub(super) fn summon_pane(&mut self, kind: PaneKindId) -> Vec<Effect> {
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
            if kind.as_str()==kind::INSPECTOR {
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

    pub(super) fn move_pane_section(
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

    pub(super) fn tear_out_tile(&mut self, member: Uuid) -> Vec<Effect> {
        let Some(owner) = self.workbench_owner_pane() else {
            return vec![Effect::Redraw];
        };
        let graph_id = self.graph_for_pane(owner).unwrap_or_else(GraphId::nil);
        if !self
            .workbench_for_pane_mut(owner)
            .is_some_and(|workbench| workbench.close_tile(member))
        {
            return vec![Effect::Redraw];
        }
        let pane_id = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        let mut effects = self.land_leaf_in_lens(
            PaneNode::Leaf {
                pane_id,
                content: PaneContent::Tile(member),
                graph_id,
            },
            None,
        );
        self.active_pane = Some(pane_id);
        let label = self
            .graph_runtimes
            .canvas(graph_id)
            .and_then(|canvas| {
                canvas
                    .graph()
                    .nodes()
                    .find(|(_, node)| node.id == member)
                    .map(|(_, node)| node.url().to_string())
            })
            .unwrap_or_default();
        self.events.push(AppEvent::TileTornOut(label));
        effects.push(Effect::SaveSession);
        effects.push(Effect::Redraw);
        effects
    }

    pub(super) fn open_in_workbench(&mut self) -> Vec<Effect> {
        let graph_pane = self
            .focused_graph_pane()
            .unwrap_or_else(|| self.default_graph_pane());
        let Some(graph_id) = self.graph_for_pane(graph_pane) else {
            return Vec::new();
        };
        let Some(target) = self
            .graph_pane_focused_member(graph_pane)
            .and_then(|member| {
                self.graph_runtimes.canvas(graph_id).and_then(|canvas| {
                    canvas
                        .graph()
                        .get_node_by_id(member)
                        .map(|(_, node)| (member, node.url().to_string()))
                })
            })
        else {
            return Vec::new();
        };
        let (member, url) = target;
        self.workbench_for_pane_mut(graph_pane)
            .expect("graph pane has an identity forme")
            .ensure_tiled();
        self.workbench_for_pane_mut(graph_pane)
            .expect("graph pane has an identity forme")
            .open_tile(member);
        self.events.push(AppEvent::WorkbenchTileOpened(url.clone()));
        let mut effects = Vec::new();
        let workbench_pane = self
            .frisket
            .iter_leaves()
            .chain(
                self.lenses
                    .iter()
                    .flatten()
                    .flat_map(|space| space.iter_leaves()),
            )
            .find(|(_, content, graph)| {
                matches!(content, PaneContent::Workbench) && *graph == graph_id
            })
            .map(|(pane, _, _)| pane);
        let workbench_pane = match workbench_pane {
            Some(pane) => pane,
            None => {
                // A first Read projection replaces its map's main surface,
                // keeping any adjacent tools. Opening Inspect first must not
                // leave a spare map split beside the same document.
                let reading_stack = self.space_of(graph_pane).filter(|space| {
                    self.space(*space).and_then(|layout| layout.outer_workspace().ok())
                        .is_some_and(|frame| {
                            crate::panes::workspace_bridge::path_for(frame.workspace.tiled(), workbench::TileId(graph_pane.0))
                                .and_then(|path| crate::panes::workspace_bridge::tree_at(frame.workspace.tiled(), &path))
                                .is_some_and(|tree| matches!(tree,workbench::TileTree::Stack(stack) if stack.tabs.len()==1))
                        })
                });
                effects.extend(self.update(Action::SummonPane(PaneKindId::new(kind::WORKBENCH))));
                let pane = self.active_pane.unwrap_or(graph_pane);
                if pane != graph_pane
                    && let Some(space) = reading_stack
                    && let Some(stack) = self
                        .space(space)
                        .and_then(|layout| layout.outer_workspace().ok())
                        .and_then(|frame| {
                            crate::panes::workspace_bridge::path_for(
                                frame.workspace.tiled(),
                                workbench::TileId(graph_pane.0),
                            )
                        })
                        .map(|path| crate::panes::workspace_bridge::path_indices(&path))
                {
                    effects.extend(
                        self.update(Action::OuterWorkspace {
                            space,
                            event: workbench::TileEvent::Dragged {
                                tile: workbench::TileId(pane.0),
                                to: workbench::DropTarget::Stack {
                                    stack,
                                    index: 1,
                                },
                            }
                            .into(),
                        }),
                    );
                }
                pane
            },
        };
        self.reveal_pane(workbench_pane);
        self.focus = crate::surface::FocusTarget::Pane(workbench_pane);
        self.publish_member_context(workbench_pane, Some(member));
        // A tile wants live content; spawn it unless it already has
        // some (live or in flight). Failure surfaces as ever.
        if self.content.flip_spawns(member) {
            self.content.note_requested(member);
            self.events.push(AppEvent::ContentState {
                node: member,
                state: "requested".to_string(),
            });
            effects.push(Effect::SpawnContent { node: member, url });
        }
        effects.push(Effect::SaveSession);
        effects.push(Effect::Redraw);
        effects
    }

    pub(super) fn toggle_pane_section(&mut self, pane: PaneId, section: String) -> Vec<Effect> {
        // Mutate the pane's OWN leaf, in whichever space holds it, so
        // the composition persists with frame.json and travels with a
        // tear-out. Unknown pane / non-composable content: honest no-op.
        let Some(space) = self.space_of(pane) else {
            return vec![Effect::Redraw];
        };
        let Some(layout) = self.space_mut(space) else {
            return vec![Effect::Redraw];
        };
        let mut changed = None;
        if let Some(cfg) = layout.content_mut(pane).and_then(|c| c.composition_mut()) {
            if let Some(pos) = cfg.sections.iter().position(|s| s == &section) {
                cfg.sections.remove(pos);
                changed = Some(false);
            } else {
                cfg.sections.push(section.clone());
                changed = Some(true);
            }
        }
        match changed {
            Some(added) => {
                self.events
                    .push(AppEvent::PaneSectionToggled { section, added });
                vec![Effect::SaveSession, Effect::Redraw]
            },
            None => vec![Effect::Redraw],
        }
    }

    pub(super) fn close_active_pane(&mut self) -> Vec<Effect> {
        // The canvas (no active pane) has nothing to close. The op
        // lands in whichever window's tree holds the pane.
        let Some((active, space)) = self
            .active_pane
            .and_then(|a| self.space_of(a).map(|s| (a, s)))
        else {
            return vec![Effect::Redraw];
        };
        if let Err(reason) = self.ensure_shared_space(space) {
            self.events.push(AppEvent::InteractionMissed {
                what: "close-pane",
                target: reason,
            });
            return vec![Effect::Redraw];
        }
        let Some(layout) = self.space_mut(space) else {
            return vec![Effect::Redraw];
        };
        if layout.close_pane(active) {
            if self.maximized == Some(active) {
                self.maximized = None;
            }
            self.active_pane = None;
            self.index_pane_spaces();
            self.events.push(AppEvent::PaneClosed);
            vec![Effect::SaveSession, Effect::Redraw]
        } else {
            vec![Effect::Redraw]
        }
    }

    pub(super) fn workbench_split_beside(
        &mut self,
        dragged: Uuid,
        target: Uuid,
        axis: WbAxis,
        after: bool,
    ) -> Vec<Effect> {
        // The app vocabulary's axis maps onto Genet's at the platen
        // call (the one place the tile contract is named).
        let axis = match axis {
            crate::action::WbAxis::Row => workbench::SplitAxis::Row,
            crate::action::WbAxis::Column => workbench::SplitAxis::Column,
        };
        let moved = self
            .workbench_owner_pane()
            .and_then(|pane| self.workbench_for_pane_mut(pane))
            .is_some_and(|workbench| workbench.split_beside_axis(dragged, target, axis, after));
        if moved {
            self.events.push(AppEvent::WorkbenchSplit);
            vec![Effect::SaveSession, Effect::Redraw]
        } else {
            vec![Effect::Redraw]
        }
    }

    pub(super) fn workbench_split_out(
        &mut self,
        dragged: Uuid,
        axis: WbAxis,
        after: bool,
    ) -> Vec<Effect> {
        let axis = match axis {
            crate::action::WbAxis::Row => workbench::SplitAxis::Row,
            crate::action::WbAxis::Column => workbench::SplitAxis::Column,
        };
        let moved = self
            .workbench_owner_pane()
            .and_then(|pane| self.workbench_for_pane_mut(pane))
            .is_some_and(|workbench| workbench.split_out(dragged, axis, after));
        if moved {
            self.events.push(AppEvent::WorkbenchSplit);
            vec![Effect::SaveSession, Effect::Redraw]
        } else {
            vec![Effect::Redraw]
        }
    }

    pub(super) fn workbench_stack_onto(&mut self, dragged: Uuid, target: Uuid) -> Vec<Effect> {
        let moved = self
            .workbench_owner_pane()
            .and_then(|pane| self.workbench_for_pane_mut(pane))
            .is_some_and(|workbench| workbench.move_to_slot_of(dragged, target));
        if moved {
            self.events.push(AppEvent::WorkbenchStacked);
            vec![Effect::SaveSession, Effect::Redraw]
        } else {
            vec![Effect::Redraw]
        }
    }

    pub(super) fn close_workbench_tile(&mut self) -> Vec<Effect> {
        let Some(owner) = self.workbench_owner_pane() else {
            return vec![Effect::Redraw];
        };
        let member = self
            .follower_context(owner)
            .and_then(|context| context.member)
            .or_else(|| self.graph_pane_focused_member(owner));
        let Some(member) = member else {
            return vec![Effect::Redraw];
        };
        self.close_reading_view(crate::reading_view::ReadTarget {
            pane: owner,
            member,
        })
    }

    pub(crate) fn close_reading_view(
        &mut self,
        target: crate::reading_view::ReadTarget,
    ) -> Vec<Effect> {
        let Some(workbench) = self.workbench_for_pane(target.pane) else {
            return vec![Effect::Redraw];
        };
        if !workbench.has_tile(target.member) {
            return vec![Effect::Redraw];
        }
        let members = workbench.open_members();
        let stack=workbench.slot_views().find(|slot|slot.members.contains(&target.member)).map(|slot|slot.members.to_vec()).unwrap_or_default();
        let focused = self.follower_context(target.pane).and_then(|c| c.member);
        let next = cambium::scope_navigation::reading_after_close_in_stack(
            &members,
            &stack,
            focused.as_ref(),
            &target.member,
        );
        self.capture_scope_origin();
        let mut effects = if next.is_none() && self.active_pane == Some(target.pane) {
            self.scope_command(crate::scope_navigation::ScopeCommand::To(
                crate::scope_navigation::ScopeLevel::Neighborhood.into()))
        } else {
            Vec::new()
        };
        if self
            .workbench_for_pane_mut(target.pane)
            .is_some_and(|w| w.close_tile(target.member))
        {
            if focused == Some(target.member)
                && let Some(member) = next
            {
                self.workbench_for_pane_mut(target.pane)
                    .unwrap()
                    .activate(member);
                self.publish_member_context(target.pane, Some(member));
                if self.active_pane == Some(target.pane) {
                    self.focus = crate::surface::FocusTarget::Pane(target.pane);
                    self.record_scope_reading(target.pane, member);
                }
            }
            self.events.push(AppEvent::WorkbenchTileClosed);
            effects.extend([
                Effect::SaveSession,
                Effect::SaveReadingPreferences,
                Effect::Redraw,
            ]);
        }
        effects
    }

    pub(super) fn toggle_maximize_pane(&mut self) -> Vec<Effect> {
        // Maximize is a PRIMARY view state (a lens's walk ignores it);
        // a lens pane no-ops honestly instead of setting a flag its
        // window would never show.
        if let Some(active) = self.active_pane
            && self.space_of(active) == Some(SpaceRef::Primary)
        {
            self.maximized = (self.maximized != Some(active)).then_some(active);
        }
        vec![Effect::Redraw]
    }

    pub(super) fn set_active_pane_divider(&mut self, ratio: f32) -> Vec<Effect> {
        let Some((active, space)) = self
            .active_pane
            .and_then(|a| self.space_of(a).map(|s| (a, s)))
        else {
            return vec![Effect::Redraw];
        };
        let Some(layout) = self.space_mut(space) else {
            return vec![Effect::Redraw];
        };
        let Some(mut path) = crate::pane::path_of(layout, active) else {
            return vec![Effect::Redraw];
        };
        // The active leaf's parent split holds the divider.
        path.pop();
        if layout.set_split_ratio(&path, ratio) {
            vec![Effect::SaveSession, Effect::Redraw]
        } else {
            vec![Effect::Redraw]
        }
    }

    pub(super) fn new_window(&mut self) -> Vec<Effect> {
        let ordinal = self.seed_lens_space();
        self.events.push(AppEvent::WindowOpened);
        vec![Effect::OpenWindow { ordinal }, Effect::Redraw]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summoning_a_tool_reveals_it_from_a_maximized_reader_without_growing_on_reentry() {
        let mut app = App::test_stub();
        let graph = app.default_graph_pane();
        app.maximized = Some(graph);
        app.update(Action::SummonPane(PaneKindId::new(kind::INSPECTOR)));
        let inspector = app.active_pane.unwrap();
        assert_ne!(inspector, graph);
        assert_eq!(app.maximized, Some(inspector));
        let count = app.frisket.iter_leaves().count();
        app.reveal_pane(graph);
        app.update(Action::SummonPane(PaneKindId::new(kind::INSPECTOR)));
        assert_eq!(app.active_pane, Some(inspector));
        assert_eq!(app.maximized, Some(inspector));
        assert_eq!(app.frisket.iter_leaves().count(), count);
    }

    #[test]
    fn floating_pane_moves_through_live_primary_and_lens_projections() {
        let mut app = App::test_stub();
        app.update(Action::SummonPane(PaneKindId::new(kind::ROSTER)));
        let roster = app.active_pane.expect("summoned roster is active");

        app.update(Action::FloatActivePane);
        assert!(app.frisket.outer_workspace().ok().is_some_and(|frame| {
            frame
                .workspace
                .floating()
                .iter()
                .any(|f| f.tile.id.0 == roster.0)
        }));

        app.update(Action::DockActivePane);
        assert!(app.frisket.outer_workspace().is_ok_and(|frame| {
            frame.workspace.floating().is_empty()
                && frame
                    .workspace
                    .tiled()
                    .find(workbench::TileId(roster.0))
                    .is_some()
        }));

        app.update(Action::FloatActivePane);
        app.update(Action::TearOutActivePane);
        assert_eq!(app.space_of(roster), Some(SpaceRef::Lens(0)));
        assert!(
            app.space(SpaceRef::Lens(0))
                .and_then(|s| s.outer_workspace().ok())
                .is_some_and(|frame| frame
                    .workspace
                    .floating()
                    .iter()
                    .any(|f| f.tile.id.0 == roster.0))
        );

        app.update(Action::ReturnActivePaneToPrimary);
        assert_eq!(app.space_of(roster), Some(SpaceRef::Primary));
        assert!(app.frisket.outer_workspace().ok().is_some_and(|frame| {
            frame
                .workspace
                .floating()
                .iter()
                .any(|f| f.tile.id.0 == roster.0)
        }));
        let events: Vec<_> = app
            .take_events()
            .into_iter()
            .map(|event| event.describe())
            .collect();
        assert!(events.iter().any(|event| event == "pane-floated roster"));
        assert!(events.iter().any(|event| event == "pane-docked roster"));
        assert!(events.iter().any(|event| event == "pane-torn-out roster"));
        assert!(events.iter().any(|event| event == "pane-returned roster"));
    }
}
