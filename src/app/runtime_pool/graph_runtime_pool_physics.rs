// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The active canvas's physics through its dynamics spec. Mere's canvas
/// exposes physics as a `DynamicsSpec` edited through the flat
/// `PhysicsChoice` view (dynamics grammar plan, F162); these keep Turnstone's
/// one-field reads and edits, each written into the spec as one change.
impl GraphRuntimePool {
    /// What the active canvas runs, as the flat view.
    pub fn physics(&self) -> PhysicsChoice {
        PhysicsChoice::live(self.active_canvas())
    }

    pub fn physics_law(&self) -> PhysicsLaw {
        self.physics().law
    }

    pub fn physics_overlays(&self) -> Vec<PhysicsOverlay> {
        self.physics().overlays
    }

    pub fn physics_kind_source(&self) -> PhysicsKindSource {
        self.physics().kind
    }

    pub fn physics_mass_source(&self) -> PhysicsMassSource {
        self.physics().mass
    }

    pub fn physics_depth_source(&self) -> PhysicsDepthSource {
        self.physics().depth
    }

    pub fn physics_profile_id(&self) -> Option<&'static str> {
        self.physics().profile_id()
    }

    /// Change the flat view and set it as one spec change. Overlays the law
    /// refuses are left out, and the refusal carries its reason.
    pub fn edit_physics(
        &mut self,
        edit: impl FnOnce(&mut PhysicsChoice),
    ) -> Result<(), OverlayRefusal> {
        Self::edit_canvas_physics(self.active_canvas_mut(), edit)
    }

    pub(crate) fn edit_canvas_physics(
        canvas: &mut Canvas,
        edit: impl FnOnce(&mut PhysicsChoice),
    ) -> Result<(), OverlayRefusal> {
        let write = mere::canvas::physics_controls::edit_physics(canvas, edit);
        if let Some(error) = write.error {
            tracing::warn!(%error, "the canvas refused the physics change");
        }
        write.refusal.map_or(Ok(()), Err)
    }

    pub fn set_physics_law(&mut self, law: PhysicsLaw) -> Result<(), OverlayRefusal> {
        self.edit_physics(|choice| choice.law = law)
    }

    pub fn set_physics_overlays(
        &mut self,
        overlays: Vec<PhysicsOverlay>,
    ) -> Result<(), OverlayRefusal> {
        self.edit_physics(|choice| choice.overlays = overlays)
    }

    pub fn set_physics_kind_source(&mut self, source: PhysicsKindSource) {
        let _ = self.edit_physics(|choice| choice.kind = source);
    }

    pub fn set_physics_mass_source(&mut self, source: PhysicsMassSource) {
        let _ = self.edit_physics(|choice| choice.mass = source);
    }

    pub fn set_physics_depth_source(&mut self, source: PhysicsDepthSource) {
        let _ = self.edit_physics(|choice| choice.depth = source);
    }

    /// Apply a catalog profile's law and overlays; `false` for an unknown id.
    pub fn apply_physics_profile(&mut self, id: &str) -> bool {
        let Some(profile) = self.physics().with_profile(id) else {
            return false;
        };
        let _ = self.edit_physics(|choice| *choice = profile);
        true
    }
}
