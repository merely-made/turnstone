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
    FloatDockTarget, GraphId, InsertSide, PaneContent, PaneId, PaneKindId, PaneMultiplicity,
    PaneNode, RelativeRect, SplitAxis, kind,
};

use super::App;

impl App {
    pub(super) fn float_active_pane(&mut self) -> Vec<Effect> {
        let Some(active) = self.active_pane else {
            return vec![Effect::Redraw];
        };
        let Some(space) = self.space_of(active) else {
            return vec![Effect::Redraw];
        };
        let floated = self.ensure_blueprint_space(space).is_some_and(|blueprint| {
            // A command-created float gets a deterministic free station. A
            // pointer drag replaces this with the user's exact rectangle;
            // the cascade merely prevents a second command float from hiding
            // the first before a drag affordance lands.
            let rect = if blueprint.floating.len() % 2 == 0 {
                RelativeRect {
                    x: 0.54,
                    y: 0.12,
                    width: 0.42,
                    height: 0.48,
                }
            } else {
                RelativeRect {
                    x: 0.08,
                    y: 0.16,
                    width: 0.42,
                    height: 0.48,
                }
            };
            blueprint.float_pane(active, rect)
        });
        if !floated {
            return vec![Effect::Redraw];
        }
        self.maximized = None;
        if let Some(content) = self.pane_content(active) {
            self.events
                .push(AppEvent::PaneFloated(content.tag().to_string()));
        }
        vec![Effect::Redraw]
    }

    pub(super) fn dock_active_pane(&mut self) -> Vec<Effect> {
        let Some(active) = self.active_pane else {
            return vec![Effect::Redraw];
        };
        let Some(space) = self.space_of(active) else {
            return vec![Effect::Redraw];
        };
        let Some(blueprint) = self.ensure_blueprint_space(space) else {
            return vec![Effect::Redraw];
        };
        let docked = match blueprint
            .tiled_panes()
            .into_iter()
            .find(|pane| *pane != active)
        {
            Some(target) => blueprint.dock_floating_pane(
                active,
                FloatDockTarget::Beside {
                    target,
                    axis: SplitAxis::Horizontal,
                    after: true,
                },
            ),
            None => blueprint.dock_floating_pane(active, FloatDockTarget::TiledRoot),
        };
        if !docked {
            return vec![Effect::Redraw];
        }
        if let Some(content) = self.pane_content(active) {
            self.events
                .push(AppEvent::PaneDocked(content.tag().to_string()));
        }
        vec![Effect::Redraw]
    }

    pub(super) fn return_active_pane_to_primary(&mut self) -> Vec<Effect> {
        let Some(active) = self.active_pane else {
            return vec![Effect::Redraw];
        };
        let Some(source @ SpaceRef::Lens(_)) = self.space_of(active) else {
            return vec![Effect::Redraw];
        };
        if !self
            .blueprint_space(source)
            .is_some_and(|blueprint| blueprint.floating.iter().any(|item| item.pane == active))
        {
            return vec![Effect::Redraw];
        }
        // Read before changing either tree. The visual station transfer is
        // transactional, and a failed transfer must not strand the retained
        // renderer in an unreachable legacy tree.
        let Some((pane_id, content, graph_id)) = self.space(source).and_then(|layout| {
            layout
                .iter_leaves()
                .find(|(id, _, _)| *id == active)
                .map(|(id, content, graph)| (id, content.clone(), graph))
        }) else {
            return vec![Effect::Redraw];
        };
        if !self.transfer_floating_blueprint(source, SpaceRef::Primary, active) {
            return vec![Effect::Redraw];
        }
        let removed = self
            .space_mut(source)
            .and_then(|layout| {
                let path = crate::pane::path_of(layout, active)?;
                layout.close_leaf(&path).then_some(())
            })
            .is_some();
        if !removed {
            return vec![Effect::Redraw];
        }
        let Some(primary) = self.space_mut(SpaceRef::Primary) else {
            return vec![Effect::Redraw];
        };
        let anchor = primary.iter_leaves().next().map(|(id, _, _)| id);
        let path = anchor
            .and_then(|id| crate::pane::path_of(primary, id))
            .unwrap_or_default();
        if !primary.summon_leaf(
            &path,
            InsertSide::Right,
            PaneNode::Leaf {
                pane_id,
                content: content.clone(),
                graph_id,
            },
        ) {
            return vec![Effect::Redraw];
        }
        self.active_pane = Some(pane_id);
        self.index_pane_spaces();
        self.events
            .push(AppEvent::PaneReturned(content.tag().to_string()));
        vec![Effect::Redraw]
    }

    pub(super) fn tear_out_active_pane(&mut self) -> Vec<Effect> {
        let Some(active) = self.active_pane else {
            return vec![Effect::Redraw];
        };
        // The pane leaves whichever window's tree holds it (a lens
        // pane tears out onward, not just primary panes out).
        let Some(source) = self.space_of(active) else {
            return vec![Effect::Redraw];
        };
        // Read the leaf wholesale (id + content + graph binding), then choose
        // the destination before either representation changes. An A5 float
        // moves its blueprint station first, retaining its relative geometry
        // and runner key across the OS-window boundary.
        let Some((pane_id, content, graph_id)) = self.space(source).and_then(|layout| {
            layout
                .iter_leaves()
                .find(|(id, _, _)| *id == active)
                .map(|(id, c, g)| (id, c.clone(), g))
        }) else {
            return vec![Effect::Redraw];
        };
        let (destination, mut effects) = self.target_lens(Some(source));
        let destination = SpaceRef::Lens(destination);
        let source_float = self
            .blueprint_space(source)
            .is_some_and(|blueprint| blueprint.floating.iter().any(|item| item.pane == active));
        if source_float && !self.transfer_floating_blueprint(source, destination, active) {
            return vec![Effect::Redraw];
        }
        // Remove the legacy payload only after the blueprint transfer passed.
        let Some(layout) = self.space_mut(source) else {
            return vec![Effect::Redraw];
        };
        let Some(path) = crate::pane::path_of(layout, active) else {
            return vec![Effect::Redraw];
        };
        if !layout.close_leaf(&path) {
            return vec![Effect::Redraw];
        }
        if self.maximized == Some(active) {
            self.maximized = None;
        }
        let SpaceRef::Lens(destination) = destination else {
            unreachable!("tear-out always targets a lens")
        };
        self.land_leaf_at_lens(
            PaneNode::Leaf {
                pane_id,
                content: content.clone(),
                graph_id,
            },
            destination,
        );
        // The moved pane STAYS active: it kept living (same runner,
        // same id), so pane-anchored ops now follow it to its new
        // window — summon-beside lands there, the divider op reweights
        // there (the lens-frisket-ops receipt's hinge).
        self.active_pane = Some(pane_id);
        self.events
            .push(AppEvent::PaneTornOut(content.tag().to_string()));
        // The move is durable structure in TWO trees; persist it (the
        // lens-window sidecar is what makes the window survive a
        // restart).
        self.index_pane_spaces();
        effects.push(Effect::SaveSession);
        effects.push(Effect::Redraw);
        effects
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
            self.active_pane = Some(existing);
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
            self.active_pane = Some(id);
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
                effects.extend(self.update(Action::SummonPane(PaneKindId::new(kind::WORKBENCH))));
                self.active_pane.unwrap_or(graph_pane)
            }
        };
        self.active_pane = Some(workbench_pane);
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
            }
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
        let Some(layout) = self.space_mut(space) else {
            return vec![Effect::Redraw];
        };
        let Some(path) = crate::pane::path_of(layout, active) else {
            return vec![Effect::Redraw];
        };
        if layout.close_leaf(&path) {
            if self.maximized == Some(active) {
                self.maximized = None;
            }
            self.active_pane = None;
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
        if self
            .workbench_for_pane_mut(owner)
            .is_some_and(|workbench| workbench.close_tile(member))
        {
            self.events.push(AppEvent::WorkbenchTileClosed);
            vec![Effect::SaveSession, Effect::Redraw]
        } else {
            vec![Effect::Redraw]
        }
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
    fn floating_pane_moves_through_live_primary_and_lens_projections() {
        let mut app = App::test_stub();
        app.update(Action::SummonPane(PaneKindId::new(kind::ROSTER)));
        let roster = app.active_pane.expect("summoned roster is active");

        app.update(Action::FloatActivePane);
        assert!(
            app.blueprint_space(SpaceRef::Primary)
                .is_some_and(|space| space.floating.iter().any(|item| item.pane == roster))
        );

        app.update(Action::DockActivePane);
        assert!(app.blueprint_space(SpaceRef::Primary).is_some_and(
            |space| space.floating.is_empty() && space.tiled_panes().contains(&roster)
        ));

        app.update(Action::FloatActivePane);
        app.update(Action::TearOutActivePane);
        assert_eq!(app.space_of(roster), Some(SpaceRef::Lens(0)));
        assert!(
            app.blueprint_space(SpaceRef::Lens(0))
                .is_some_and(|space| space.floating.iter().any(|item| item.pane == roster))
        );

        app.update(Action::ReturnActivePaneToPrimary);
        assert_eq!(app.space_of(roster), Some(SpaceRef::Primary));
        assert!(
            app.blueprint_space(SpaceRef::Primary)
                .is_some_and(|space| space.floating.iter().any(|item| item.pane == roster))
        );
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
