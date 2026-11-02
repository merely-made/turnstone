// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Run the behavior cascade for whatever has been committed since last time.
///
/// The after-dispatch drain: called once per action, after the action's own
/// effects are decided, so a woken body sees the world the action left rather
/// than the one it found.
pub fn drain(app: &mut App) -> Vec<Effect> {
    if app.session_load_refused() || app.draining {
        return Vec::new();
    }
    let error = match app.journal.lock() {
        Ok(journal) => journal.execution_error().map(|error| error.to_string()),
        Err(_) => Some("behavior journal capture lock poisoned".into()),
    };
    if let Some(error) = error {
        app.refuse_behavior(error);
        return vec![Effect::Redraw];
    }
    if app.denizens.is_empty() {
        return Vec::new();
    }
    let Some(origin) = app.behavior_execution_origin() else {
        routing_refused(app);
        return vec![Effect::Redraw];
    };
    app.denizens.authority.set_now(crate::denizen::now_ms());
    app.draining = true;
    let mut effects = drain_time_tier(app, origin);
    if binding_matches(app, origin) {
        effects.extend(drain_app_tier(app, origin));
    }
    if binding_matches(app, origin) && !app.watches.is_empty() {
        effects.extend(drain_graph_tier(app, origin));
    }
    app.draining = false;
    if binding_matches(app, origin) && !effects.is_empty() {
        crate::denizen::save_watches(
            &app.session_dir(),
            &app.watches,
            &app.app_watches,
            &app.time_watches,
            &app.deadbands,
        );
    }
    effects
}
