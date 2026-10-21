// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use mere::canvas::controls::CanvasCommand;

impl App {
    /// Arrange controls prefer a graph pane in their own window. A sole
    /// matching graph pane in another window remains the explicit owner
    /// after a tear-out; ambiguous foreign selections are not guessed.
    pub(crate) fn scene_command_pane(&self) -> Option<PaneId> {
        if let Some(pane) = self.graph_command_pane() {
            return Some(pane);
        }
        let pane = self.active_pane?;
        if self.pane_content(pane)?.kind_id().as_str() != crate::panes::kind::ARRANGE {
            return None;
        }
        let graph = self.graph_for_pane(pane)?;
        if let Some(local) = self
            .space(self.space_of(pane)?)?
            .iter_leaves()
            .find(|(_, content, owner)| **content == PaneContent::Orrery && *owner == graph)
            .map(|(id, _, _)| id)
        {
            return Some(local);
        }
        let mut matching = self
            .frisket
            .iter_leaves()
            .chain(
                self.lenses
                    .iter()
                    .flatten()
                    .flat_map(|space| space.iter_leaves()),
            )
            .filter(|(_, content, owner)| **content == PaneContent::Orrery && *owner == graph)
            .map(|(id, _, _)| id);
        let sole = matching.next()?;
        matching.next().is_none().then_some(sole)
    }

    pub(crate) fn mark_graph_scene_changed(&mut self, pane: PaneId) {
        if let Some(runtime) = self
            .graph_for_pane(pane)
            .and_then(|g| self.graph_runtimes.get_mut(g))
        {
            runtime.scene_save_pending = true;
            runtime.scene_restore_pending = false;
        }
    }

    /// Persist only the resolved graph's arrangement and view choice. Graph
    /// truth, browser state and unrelated sessions remain with their owners.
    pub(crate) fn persist_graph_scene(&mut self, graph: GraphId) -> std::io::Result<()> {
        let runtime = self
            .graph_runtimes
            .get_mut(graph)
            .ok_or_else(|| std::io::Error::other("graph runtime absent"))?;
        let session = runtime
            .session
            .ok_or_else(|| std::io::Error::other("scene has no saved session"))?;
        let directory = crate::session::session_dir(&self.data_root, session);
        runtime.persistence.placement_for(&directory)?;
        let geometry = runtime.canvas.cartography_geometry();
        let container = Some(*graph.as_uuid());
        let facets = runtime.canvas.facets_mut();
        pandect::write_arrangement_positions(facets, geometry.iter());
        crate::graph_scenes::write(facets, container, &geometry);
        pandect::save_node_facets(&directory, facets)?;
        let canvas = &runtime.canvas;
        let physics = mere::canvas::PhysicsChoice::live(canvas);
        let mut intent = crate::session::load_view_intent(&directory).unwrap_or_default();
        intent.layout_strategy = canvas.layout_strategy().map(str::to_owned);
        intent.physics_paused = Some(canvas.physics_paused());
        intent.physics_law = Some(physics.law.id().into());
        intent.physics_overlays = physics.overlays.iter().map(|o| o.id().into()).collect();
        intent.physics_kind_source = Some(physics.kind.id().into());
        intent.physics_mass_source = Some(physics.mass.id().into());
        intent.physics_depth_source = Some(physics.depth.id().into());
        crate::session::try_save_view_intent(&directory, &intent)
    }

    pub(super) fn edit_graph_physics(&mut self, action: Action) -> Vec<Effect> {
        let Some(pane) = self.scene_command_pane() else {
            return vec![];
        };
        if action == Action::TogglePhysics {
            if let Some(graph) = self.graph_for_pane(pane) {
                self.graph_runtimes.get_mut(graph).unwrap().scene_transition = None;
            }
        }
        let result = self.with_graph_pane(pane, |canvas| {
            if action == Action::TogglePhysics {
                canvas.take_stop_return();
                canvas.toggle_physics_paused();
                return Ok(());
            }
            GraphRuntimePool::edit_canvas_physics(canvas, |choice| match action {
                Action::SetPhysicsLaw(id) => {
                    if let Some(law) = mere::canvas::PhysicsLaw::parse(id) {
                        choice.law = law;
                    }
                },
                Action::SetPhysicsOverlay(id, on) => {
                    if let Some(overlay) = mere::canvas::PhysicsOverlay::parse(id) {
                        choice.overlays.retain(|o| *o != overlay);
                        if on {
                            choice.overlays.push(overlay);
                        }
                    }
                },
                Action::SetPhysicsKindSource(id) => {
                    if let Some(source) = mere::canvas::PhysicsKindSource::parse(id) {
                        choice.kind = source;
                    }
                },
                Action::SetPhysicsMassSource(id) => {
                    if let Some(source) = mere::canvas::PhysicsMassSource::parse(id) {
                        choice.mass = source;
                    }
                },
                Action::SetPhysicsDepthSource(id) => {
                    if let Some(source) = mere::canvas::PhysicsDepthSource::parse(id) {
                        choice.depth = source;
                    }
                },
                Action::ApplyPhysicsProfile(id) => {
                    if let Some(profile) = choice.clone().with_profile(id) {
                        *choice = profile;
                    }
                },
                _ => {},
            })
        });
        if let Some(result) = result {
            self.record_physics_result(result);
        }
        self.mark_graph_scene_changed(pane);
        vec![Effect::Redraw]
    }

    pub(super) fn edit_graph_scene(&mut self, action: Action) -> Vec<Effect> {
        let pane = self.scene_command_pane().or_else(|| {
            matches!(action, Action::ToggleTerritory)
                .then(|| {
                    let member = self.browser_command_target().map(|(member, _)| member);
                    self.primary_graph_for_navigation(member)
                })
                .flatten()
        });
        let Some(pane) = pane else {
            return vec![];
        };
        if matches!(action, Action::ToggleTerritory) {
            self.with_graph_pane(pane, |canvas| {
                canvas.set_territory_field(!canvas.territory_field());
            });
            return vec![Effect::Redraw];
        }
        if let Some(runtime) = self
            .graph_for_pane(pane)
            .and_then(|graph| self.graph_runtimes.get_mut(graph))
        {
            runtime.scene_restore_pending = false;
        }
        let durable = !matches!(
            action,
            Action::ToggleForegroundPin | Action::DismissRetained
        );
        if matches!(action, Action::GrowTiles | Action::ShrinkTiles) {
            if let Some(graph) = self.graph_for_pane(pane) {
                self.graph_runtimes.get_mut(graph).unwrap().scene_transition = None;
            }
            self.with_graph_pane(pane, |canvas| {
                canvas.take_stop_return();
            });
        }
        let command = match action {
            Action::ResetToSeed => CanvasCommand::ResetToSeed,
            Action::SetSeed => CanvasCommand::SetSeed,
            Action::RestoreRecipeSeed => CanvasCommand::RestoreRecipeSeed,
            Action::ToggleForegroundPin => CanvasCommand::ToggleForegroundPin,
            Action::DismissRetained => CanvasCommand::DismissRetained,
            Action::GrowTiles => CanvasCommand::GrowTiles,
            Action::ShrinkTiles => CanvasCommand::ShrinkTiles,
            _ => return vec![],
        };
        self.with_graph_pane(pane, |canvas| {
            command.apply(canvas, (1, 1));
        });
        if durable {
            self.mark_graph_scene_changed(pane);
        }
        vec![Effect::Redraw]
    }
}
