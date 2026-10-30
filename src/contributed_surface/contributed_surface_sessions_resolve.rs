// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ContributedSurfaceSessions {
    pub fn resolve(
        &mut self,
        spec: &PaneSpec,
        registry: &SurfaceProviderRegistry,
    ) -> Result<&mut ContributedSurfacePane, SurfaceAdmissionError> {
        let replace = self
            .panes
            .get(&spec.id)
            .is_none_or(|pane| !pane.matches(&spec.kind, &spec.source));
        if replace {
            let generation = self
                .next_generation
                .checked_add(1)
                .ok_or(SurfaceAdmissionError::GenerationExhausted)?;
            let mut admitted = registry.admit(&spec.kind, &spec.source)?;
            admitted.generation = generation;
            self.next_generation = generation;
            if let Some(old) = self.panes.get_mut(&spec.id) {
                old.mouse_cancel(0.0, 0.0);
            }
            self.panes.insert(spec.id, admitted);
        }
        Ok(self
            .panes
            .get_mut(&spec.id)
            .expect("a contributed pane was admitted above"))
    }

    pub fn get(&self, pane: PaneId) -> Option<&ContributedSurfacePane> {
        self.panes.get(&pane)
    }

    pub fn get_mut(&mut self, pane: PaneId) -> Option<&mut ContributedSurfacePane> {
        self.panes.get_mut(&pane)
    }

    pub fn remove(&mut self, pane: PaneId) -> Option<ContributedSurfacePane> {
        let mut removed = self.panes.remove(&pane)?;
        removed.mouse_cancel(0.0, 0.0);
        Some(removed)
    }

    pub fn len(&self) -> usize {
        self.panes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.panes.is_empty()
    }

    pub fn any_bars_visible(&mut self) -> bool {
        self.panes
            .values_mut()
            .any(ContributedSurfacePane::bars_visible)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (PaneId, &ContributedSurfacePane)> {
        self.panes.iter().map(|(pane, session)| (*pane, session))
    }
}
