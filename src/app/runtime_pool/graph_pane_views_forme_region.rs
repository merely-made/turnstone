// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl GraphPaneViews {
    pub(crate) fn forme_region(&self, pane: PaneId) -> Option<&mere::canvas::FormeRegion> {
        self.forme_regions.get(&pane)
    }

    pub(crate) fn forme_scope(&self, pane: PaneId) -> Option<uuid::Uuid> {
        self.forme_scopes.get(&pane).copied()
    }

    pub(crate) fn set_forme_scope(
        &mut self,
        pane: PaneId,
        id: uuid::Uuid,
        members: Vec<uuid::Uuid>,
    ) {
        self.set_scope(pane, Some(members));
        self.forme_scopes.insert(pane, id);
    }

    pub(crate) fn installed_forme(&self, pane: PaneId) -> Option<uuid::Uuid> {
        self.forme_regions.get(&pane).map(|r| r.id)
    }

    pub(crate) fn set_forme_region(
        &mut self,
        pane: PaneId,
        region: Option<mere::canvas::FormeRegion>,
    ) {
        if let Some(region) = region {
            self.forme_regions.insert(pane, region);
        } else {
            self.forme_regions.remove(&pane);
        }
    }

    pub(crate) fn bind_graph(&mut self, pane: PaneId, graph: GraphId) {
        if self.owners.get(&pane).is_some_and(|owner| *owner != graph) {
            self.remove(pane);
        }
        self.owners.insert(pane, graph);
    }

    pub(crate) fn set_scope(&mut self, pane: PaneId, scope: Option<Vec<uuid::Uuid>>) {
        self.forme_scopes.remove(&pane);
        self.neighborhoods.remove(&pane);
        self.scopes.insert(pane, scope);
    }

    pub(crate) fn set_neighborhood(&mut self, pane: PaneId, root: uuid::Uuid) {
        self.forme_scopes.remove(&pane);
        self.neighborhoods.insert(pane, root);
    }

    pub(crate) fn neighborhood_root(&self, pane: PaneId) -> Option<uuid::Uuid> {
        self.neighborhoods.get(&pane).copied()
    }

    /// Finish an explicit fit after this pane receives its actual render size.
    pub(crate) fn request_fit(&mut self, pane: PaneId) {
        self.pending_fits.insert(pane);
    }

    pub(crate) fn take_fit(&mut self, pane: PaneId) -> bool {
        self.pending_fits.remove(&pane)
    }

    /// Install `pane`'s saved viewport, or capture the runtime's current
    /// viewport for a fresh pane. Call before the pane's input/render pass.
    pub fn install(&mut self, pane: PaneId, canvas: &mut Canvas) {
        if canvas.forme_region() != self.forme_regions.get(&pane) {
            let _ = canvas.set_forme_region(self.forme_regions.get(&pane).cloned());
        }
        self.scopes.entry(pane).or_insert(None);
        if let Some(scope) = self.scopes.get(&pane) {
            if let Some(scope) = scope {
                canvas.scope_to_exact_members(scope.iter().copied());
            } else {
                canvas.clear_scope();
            }
        }
        if let Some(root) = self.neighborhoods.get(&pane) {
            canvas.scope_to_exact_members(
                mere::navigation::neighborhood_members(canvas.graph(), *root).unwrap_or_default(),
            );
        }
        if let Some(viewport) = self.viewports.get(&pane).copied() {
            canvas.set_viewport(viewport);
        } else {
            self.viewports.insert(pane, canvas.viewport());
        }
        if let Some(selection) = self.selections.get(&pane) {
            canvas.set_selected_members(selection);
        } else {
            self.selections.insert(pane, canvas.selected_members());
        }
    }

    /// Save the working viewport after the pane's input/render pass.
    pub fn stash(&mut self, pane: PaneId, canvas: &Canvas) {
        self.scopes.insert(pane, canvas.scope_members());
        self.viewports.insert(pane, canvas.viewport());
        self.selections.insert(pane, canvas.selected_members());
    }

    pub fn viewport(&self, pane: PaneId) -> Option<Viewport> {
        self.viewports.get(&pane).copied()
    }

    pub fn remove(&mut self, pane: PaneId) -> Option<Viewport> {
        self.owners.remove(&pane);
        self.pending_fits.remove(&pane);
        self.selections.remove(&pane);
        self.scopes.remove(&pane);
        self.neighborhoods.remove(&pane);
        self.forme_regions.remove(&pane);
        self.forme_scopes.remove(&pane);
        self.viewports.remove(&pane)
    }

    /// The one selected graph member this pane publishes, if its selection is
    /// unambiguous. Multiple selections deliberately do not manufacture a
    /// focused member for followers.
    pub fn focused_member(&self, pane: PaneId) -> Option<uuid::Uuid> {
        let selection = self.selections.get(&pane)?;
        if selection.len() == 1 {
            Some(selection[0])
        } else {
            None
        }
    }

    pub fn selected_members(&self, pane: PaneId) -> Option<&[uuid::Uuid]> {
        self.selections.get(&pane).map(Vec::as_slice)
    }
}
