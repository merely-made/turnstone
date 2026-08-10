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
    pub(super) fn tear_out_active_pane(&mut self) -> Vec<Effect> {
        let Some(active) = self.active_pane else {
            return vec![Effect::Redraw];
        };
        // The pane leaves whichever window's tree holds it (a lens
        // pane tears out onward, not just primary panes out).
        let Some(source) = self.space_of(active) else {
            return vec![Effect::Redraw];
        };
        // Read the leaf wholesale (id + content + graph binding), then
        // remove it from its source tree.
        let Some(layout) = self.space_mut(source) else {
            return vec![Effect::Redraw];
        };
        let Some((pane_id, content, graph_id)) = layout
            .iter_leaves()
            .find(|(id, _, _)| *id == active)
            .map(|(id, c, g)| (id, c.clone(), g))
        else {
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
        let mut effects = self.land_leaf_in_lens(
            PaneNode::Leaf {
                pane_id,
                content: content.clone(),
                graph_id,
            },
            Some(source),
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
            crate::action::WbAxis::Row => genet_host_api::tile::SplitAxis::Row,
            crate::action::WbAxis::Column => genet_host_api::tile::SplitAxis::Column,
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
            crate::action::WbAxis::Row => genet_host_api::tile::SplitAxis::Row,
            crate::action::WbAxis::Column => genet_host_api::tile::SplitAxis::Column,
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
