// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// The surface lane of each node's content: the live producer, for the host
/// work the content does not wrap (pointer streams, drag, accessibility,
/// capture, imported frames).
pub(crate) trait NodeSurfaces {
    fn surface_mut(&mut self, node: &uuid::Uuid) -> Option<&mut dyn inker::SurfaceProducer>;
    fn has_surface(&self, node: &uuid::Uuid) -> bool;
    /// Whether `node`'s content is a document, for controller input admission.
    fn has_document(&self, node: &uuid::Uuid) -> bool;
    fn surfaces_mut(
        &mut self,
    ) -> impl Iterator<Item = (uuid::Uuid, &mut dyn inker::SurfaceProducer)>;
    fn surface_count(&self) -> usize;
}
