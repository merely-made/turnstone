// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The contributed branch of the ordered native drain, shared by delivery controls.
pub(super) fn dispatch_contributed_action(
    app: &mut App,
    sessions: &mut crate::contributed_surface::ContributedSurfaceSessions,
    published: &crate::contributed_a11y::PublishedRoute,
    request: &accesskit::ActionRequest,
    plan: &[crate::surface::Surface],
    spec: Option<&crate::panes::PaneSpec>,
) -> bool {
    let route = published.route;
    let rect = plan.iter().find_map(|surface| {
        (surface.kind == crate::surface::SurfaceKind::Pane(route.pane)).then_some(surface.rect)
    });
    let current = crate::contributed_a11y::published_route_is_current(
        sessions,
        published,
        rect,
        request.target_node,
        request.action,
    );
    let landed =
        current && crate::contributed_a11y::apply_route(sessions, spec, route, request.action);
    if landed && request.action == accesskit::Action::Focus {
        app.focus = crate::surface::FocusTarget::Pane(route.pane);
        app.active_pane = Some(route.pane);
        app.raise_floating_pane(route.pane);
    }
    landed
}
