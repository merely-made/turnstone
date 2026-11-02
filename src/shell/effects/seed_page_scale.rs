// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// The scale a freshly spawned surface or session must be seeded with: the
/// node's persisted request, when it is not already the engine's own default.
/// A node at the default asks for nothing, so the engine keeps its own.
pub(super) fn seed_page_scale(app: &crate::app::App, node: uuid::Uuid) -> Option<f32> {
    let scale = app.browser.get(node).and_then(|state| state.page_scale)?;
    ((scale - crate::app::PAGE_ZOOM_DEFAULT).abs() >= f32::EPSILON).then_some(scale)
}
