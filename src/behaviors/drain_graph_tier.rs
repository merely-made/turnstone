// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn drain_graph_tier(
    app: &mut App,
    origin: crate::host_journal::RuntimeOrigin,
) -> Vec<Effect> {
    let Some((boundary, entries)) = projected_tail(app, origin, app.behavior_cursor) else {
        return Vec::new();
    };
    // A foreign-only tail is consumed without constructing a local trigger.
    app.behavior_cursor = boundary;
    if entries.is_empty() {
        return Vec::new();
    }

    let budget = CascadeBudget::new(app.cascade_budget);
    let mut effects = Vec::new();
    let mut watches = std::mem::take(&mut app.watches);
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
            let before = journal_high_water(app);
            effects.extend(app.run_denizen_for_cascade(member, &context, required));
            if !binding_matches(app, origin) {
                routing_refused(app);
                effects.push(Effect::Redraw);
                produced.clear();
                break;
            }

            if let Some((boundary, entries)) = projected_tail(app, origin, before) {
                app.behavior_cursor = boundary;
                produced.extend(entries);
            }
        }
        round_entries = produced.clone();
        produced
    });
    // A body may have loaded another session or replaced this same graph.
    // Its newly loaded table is authoritative; never put the old one over it.
    let current = app.behavior_execution_origin();
    if crate::host_journal::restore_table(&mut app.watches, watches, origin, current) {
        report(app, &cascade);
    }
    effects
}
