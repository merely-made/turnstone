// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The machine-readable place receipt a scenario writes out: the same status
/// lines a person reads, plus the facts two peers are compared on.
pub fn place_record(app: &App) -> serde_json::Value {
    let contract = crate::place::contract::lifecycle_report(&app.place, crate::denizen::now_ms());
    serde_json::json!({
        "status": app.place.status_lines(),
        "place": place_facts(app),
        "contract": contract,
        "knot_documents": app.knot_documents(),
    })
}
