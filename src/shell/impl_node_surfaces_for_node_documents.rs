// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl NodeSurfaces for NodeDocuments {
    fn surface_mut(&mut self, node: &uuid::Uuid) -> Option<&mut dyn inker::SurfaceProducer> {
        self.get_mut(node)
            .and_then(pelt_core::PeltContent::surface_producer_mut)
    }

    fn has_surface(&self, node: &uuid::Uuid) -> bool {
        self.get(node)
            .is_some_and(|content| content.lane() == pelt_core::PeltLane::Surface)
    }

    fn has_document(&self, node: &uuid::Uuid) -> bool {
        self.get(node)
            .is_some_and(|content| content.lane() == pelt_core::PeltLane::Document)
    }

    fn surfaces_mut(
        &mut self,
    ) -> impl Iterator<Item = (uuid::Uuid, &mut dyn inker::SurfaceProducer)> {
        self.pool.surfaces_mut().filter_map(|(key, content)| {
            let content_pool::ContentKey::Node(node) = key else {
                return None;
            };
            content
                .surface_producer_mut()
                .map(|producer| (*node, producer))
        })
    }

    fn surface_count(&self) -> usize {
        self.values()
            .filter(|content| content.lane() == pelt_core::PeltLane::Surface)
            .count()
    }
}
