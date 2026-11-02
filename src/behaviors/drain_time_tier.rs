// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The clock tier: wake whatever is due.
///
/// No cascade here. A tick is caused by the clock rather than by a commit, so
/// there is nothing for a woken body to feed back into: what it *writes* goes
/// to the journal, which the graph drain picks up in the same beat. One pass,
/// stable order, nothing to bound.
pub(super) fn drain_time_tier(
    app: &mut App,
    origin: crate::host_journal::RuntimeOrigin,
) -> Vec<Effect> {
    if app.time_watches.is_empty() {
        return Vec::new();
    }
    // A host with no clock fires nothing, rather than treating "no time" as
    // time zero and running every schedule at once.
    let Some(now_ms) = app.now_ms else {
        return Vec::new();
    };
    let due = app.time_watches.due(now_ms);
    let mut effects = Vec::new();
    for subject in due {
        if !binding_matches(app, origin) {
            routing_refused(app);
            effects.push(Effect::Redraw);
            break;
        }
        app.denizens.authority.set_now(crate::denizen::now_ms());
        let Some(member) = member_of(app, subject, &[]) else {
            app.record_event(AppEvent::DenizenRefused(
                "clock behavior wake refused by current subject routing or read authority".into(),
            ));
            effects.push(Effect::Redraw);
            continue;
        };
        // Nothing woke it but the clock, so its context is empty: a scheduled
        // body has no matched entries to read, and saying so is truer than
        // handing it the last thing that happened to change.
        effects.extend(app.run_denizen_for_clock(member, now_ms));
        if !binding_matches(app, origin) {
            routing_refused(app);
            effects.push(Effect::Redraw);
            break;
        }
    }
    effects
}
