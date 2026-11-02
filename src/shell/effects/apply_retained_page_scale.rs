// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Ask one retained document session to present at `scale`, keeping the
/// effective level it answers with. Unlike the hosted lane this is a real
/// read-back, so the answer is worth holding; a lane without the control
/// refuses quietly, exactly as an absent hosted surface does.
pub(super) fn apply_retained_page_scale(
    app: &mut crate::app::App,
    node: uuid::Uuid,
    scale: f32,
    session: &mut dyn inker::DocumentSession<netrender::Scene>,
) {
    match session.set_page_zoom(scale) {
        Ok(state) => app.content.note_page_zoom(node, app_page_zoom_facts(state)),
        // A capability-gated surface never offers the rows to a lane that
        // reports no page zoom, so this is a chord or a script arriving at a
        // scripted or smolweb session: nothing to say, nothing to break.
        Err(inker::SessionError::Unsupported(reason)) => {
            tracing::debug!(%node, scale, %reason, "retained session does not offer page zoom");
        },
        Err(error) => {
            let error = error.to_string();
            tracing::warn!(%node, scale, %error, "retained page zoom apply failed");
            app.note(crate::observe::AppEvent::AffordanceUnavailable {
                what: "page-zoom",
                target: error,
            });
        },
    }
}
