// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Say what the cascade did, loudly when it hit the budget.
pub(super) fn report(app: &mut App, cascade: &Cascade) {
    app.denizens.authority.set_now(crate::denizen::now_ms());
    if let CascadeOutcome::BudgetExhausted { still_waking } = &cascade.outcome {
        let names: Vec<String> = still_waking
            .iter()
            .filter_map(|subject| member_of(app, *subject, &[]))
            .filter_map(|member| {
                app.denizens
                    .residents
                    .get(&member)
                    .map(|resident| resident.label.clone())
            })
            .collect();
        let named = if names.is_empty() {
            "unnamed behaviors".to_string()
        } else {
            names.join(", ")
        };
        tracing::warn!(
            rounds = cascade.rounds.len(),
            %named,
            "behavior cascade hit its budget"
        );
        app.record_event(AppEvent::CascadeExhausted(named));
    } else if !cascade.rounds.is_empty() {
        tracing::debug!(rounds = cascade.rounds.len(), "behavior cascade settled");
    }
}
