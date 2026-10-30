// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Seed a freshly spawned surface with the page scale its node requested. A
/// node at the default scale asks for nothing, so the engine keeps its own.
pub(super) fn replay_page_scale(
    app: &mut crate::app::App,
    node: uuid::Uuid,
    producer: &mut dyn inker::SurfaceProducer,
) {
    let Some(scale) = seed_page_scale(app, node) else {
        return;
    };
    apply_page_scale(app, node, scale, producer);
}
