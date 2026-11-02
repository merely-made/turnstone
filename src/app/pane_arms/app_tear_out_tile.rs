// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    pub(in super::super) fn tear_out_tile(&mut self, member: Uuid) -> Vec<Effect> {
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

    pub(in super::super) fn open_in_workbench(&mut self) -> Vec<Effect> {
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
                                to: workbench::DropTarget::Stack { stack, index: 1 },
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

    pub(in super::super) fn toggle_pane_section(
        &mut self,
        pane: PaneId,
        section: String,
    ) -> Vec<Effect> {
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

    pub(in super::super) fn close_active_pane(&mut self) -> Vec<Effect> {
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

    pub(in super::super) fn workbench_split_beside(
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

    pub(in super::super) fn workbench_split_out(
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

    pub(in super::super) fn workbench_stack_onto(
        &mut self,
        dragged: Uuid,
        target: Uuid,
    ) -> Vec<Effect> {
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

    pub(in super::super) fn close_workbench_tile(&mut self) -> Vec<Effect> {
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
        let stack = workbench
            .slot_views()
            .find(|slot| slot.members.contains(&target.member))
            .map(|slot| slot.members.to_vec())
            .unwrap_or_default();
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
                crate::scope_navigation::ScopeLevel::Neighborhood.into(),
            ))
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

    pub(in super::super) fn toggle_maximize_pane(&mut self) -> Vec<Effect> {
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

    pub(in super::super) fn set_active_pane_divider(&mut self, ratio: f32) -> Vec<Effect> {
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

    pub(in super::super) fn new_window(&mut self) -> Vec<Effect> {
        let ordinal = self.seed_lens_space();
        self.events.push(AppEvent::WindowOpened);
        vec![Effect::OpenWindow { ordinal }, Effect::Redraw]
    }
}
