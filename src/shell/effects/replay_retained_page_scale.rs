// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Seed a freshly spawned retained session with the page scale its node
/// requested — the retained twin of [`replay_page_scale`], and the one seed
/// point every retained spawn (first open, restart replay, engine switch)
/// funnels through.
pub(super) fn replay_retained_page_scale(
    app: &mut crate::app::App,
    node: uuid::Uuid,
    session: &mut dyn inker::DocumentSession<netrender::Scene>,
) {
    let Some(scale) = seed_page_scale(app, node) else {
        return;
    };
    apply_retained_page_scale(app, node, scale, session);
}
