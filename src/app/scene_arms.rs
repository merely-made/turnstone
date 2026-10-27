// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use mere::canvas::controls::CanvasCommand;
use mere::canvas::physics_form::{FieldId, PhysicsForm};
use mere::canvas::{PhysicsChoice, PhysicsOverlay};

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
        crate::scene_state::stage_scene(&mut runtime.canvas, *graph.as_uuid(), &runtime.view);
        pandect::save_node_facets(&directory, runtime.canvas.facets_mut())?;
        crate::scene_state::save_view(
            &directory,
            &runtime.canvas,
            &self.command_choices,
            &runtime.view,
        )
        .map_err(std::io::Error::other)
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

            let mut form = PhysicsForm::new(canvas);
            let original_overlays = PhysicsChoice::live(canvas).overlays;
            let overlay_toggle = match &action {
                Action::SetPhysicsOverlay(id, on) => PhysicsOverlay::ALL
                    .into_iter()
                    .find(|overlay| overlay.id() == *id)
                    .map(|overlay| (overlay, *on)),
                _ => None,
            };
            let changed = match &action {
                Action::SetPhysicsLaw(id) => form.choose(FieldId::PhysicsLaw, id),
                Action::SetPhysicsOverlay(id, on) => PhysicsOverlay::ALL
                    .iter()
                    .find(|overlay| overlay.id() == *id)
                    .is_some_and(|overlay| form.tick(FieldId::PhysicsOverlay(*overlay), *on)),
                Action::SetPhysicsKindSource(id) => form.choose(FieldId::PhysicsKind, id),
                Action::SetPhysicsMassSource(id) => form.choose(FieldId::PhysicsMass, id),
                Action::SetPhysicsDepthSource(id) => form.choose(FieldId::PhysicsDepth, id),
                Action::ApplyPhysicsProfile(id) if *id == "custom" => false,
                Action::ApplyPhysicsProfile(id) => form.choose(FieldId::PhysicsProfile, id),
                _ => false,
            };
            if !changed {
                return Ok(());
            }
            let mut selected = form.physics_choice();
            if let Some((overlay, on)) = overlay_toggle {
                // Preserve the existing remove-then-append order policy for
                // an explicit toggle while the form validates the field.
                selected.overlays = original_overlays;
                selected.overlays.retain(|known| *known != overlay);
                if on {
                    selected.overlays.push(overlay);
                }
            } else if !matches!(&action, Action::ApplyPhysicsProfile(_)) {
                // A source or law pick carries authored overlay order
                // unchanged; write_into uses that identity to retain roots.
                selected.overlays = original_overlays;
            }
            GraphRuntimePool::edit_canvas_physics(canvas, |choice| *choice = selected)
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
        let Some(command) = action.canvas_verb() else {
            return vec![];
        };
        if command == CanvasCommand::ToggleTerritory {
            self.with_graph_pane(pane, |canvas| {
                let viewport = canvas.view_size();
                command.apply(canvas, viewport);
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
        self.with_graph_pane(pane, |canvas| {
            let viewport = canvas.view_size();
            command.apply(canvas, viewport);
        });
        if durable {
            self.mark_graph_scene_changed(pane);
        }
        vec![Effect::Redraw]
    }
}
