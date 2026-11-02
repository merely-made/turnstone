// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The app tier: wake on what the application did, rather than on what the
/// graph recorded. Runs first, so anything a woken body writes to the graph is
/// picked up by the graph drain in the same beat.
pub(super) fn drain_app_tier(
    app: &mut App,
    origin: crate::host_journal::RuntimeOrigin,
) -> Vec<Effect> {
    if app.app_watches.is_empty() {
        app.mark_events_seen();
        return Vec::new();
    }
    let entries = app_entries(app, None);
    app.mark_events_seen();
    if entries.is_empty() {
        return Vec::new();
    }
    let budget = CascadeBudget::new(app.cascade_budget);
    let mut effects: Vec<Effect> = Vec::new();
    let mut watches = std::mem::take(&mut app.app_watches);
    let scoped_reads = scoped_reads(&watches);
    let mut round_entries = entries.clone();
    let cascade = run_cascade(&mut watches, budget, entries, |wakes| {
        let mut produced = Vec::new();
        for wake in wakes {
            if !binding_matches(app, origin) {
                routing_refused(app);
                effects.push(Effect::Redraw);
                produced.clear();
                break;
            }
            let required = scoped_reads
                .get(&wake.subject)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            app.denizens.authority.set_now(crate::denizen::now_ms());
            let Some(member) = member_of(app, wake.subject, required) else {
                app.record_event(AppEvent::DenizenRefused(
                    "behavior wake refused by current subject routing or read authority".into(),
                ));
                effects.push(Effect::Redraw);
                continue;
            };
            let context = context_for(&round_entries, wake);
            let hex = wake.subject.to_hex();
            let before = app.events_len();
            effects.extend(app.run_denizen_for_cascade(member, &context, required));
            if !binding_matches(app, origin) {
                routing_refused(app);
                effects.push(Effect::Redraw);
                produced.clear();
                break;
            }
            // Whatever the body just caused is attributed to it, so it cannot
            // be woken by its own noise.
            produced.extend(app_entries(app, Some((before, &hex))));
            app.mark_events_seen();
        }
        round_entries = produced.clone();
        produced
    });
    let current = app.behavior_execution_origin();
    if crate::host_journal::restore_table(&mut app.app_watches, watches, origin, current) {
        report(app, &cascade);
    }
    effects
}
