// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The pane's rows as plain text, for the observe snapshot and `assert row`:
/// what a person reads off the pane, without rendering it.
pub fn arrange_rows(app: &App) -> Vec<String> {
    let choice = Choice::of(app);
    let label = |catalog: &[(&str, &str)], id: &str| -> String {
        catalog
            .iter()
            .find(|(known, _)| *known == id)
            .map(|(_, label)| (*label).to_string())
            .unwrap_or_else(|| id.to_string())
    };
    let layout = if choice.layout == FREE_LAYOUT_ID {
        "Free".to_string()
    } else {
        label(CANVAS_LAYOUT_STRATEGIES, &choice.layout)
    };
    let physics = &choice.readback.physics;
    let overlays = if physics.overlays.is_empty() {
        "none".to_string()
    } else {
        physics
            .overlays
            .iter()
            .map(|overlay| overlay.label().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut rows = vec![
        format!("Arrangement: {layout}"),
        format!(
            "Seed: {}",
            if choice.saved_seed { "saved" } else { "recipe" }
        ),
        format!("Regions: {}", choice.regions),
        format!(
            "Physics: {}",
            field_label(FieldId::PhysicsLaw, physics.law.id())
        ),
        format!("Overlays: {overlays}"),
        format!(
            "Kinds: {}",
            field_label(FieldId::PhysicsKind, physics.kind.id())
        ),
        format!(
            "Mass: {}",
            field_label(FieldId::PhysicsMass, physics.mass.id())
        ),
        format!(
            "Depth: {}",
            field_label(FieldId::PhysicsDepth, physics.depth.id())
        ),
        format!(
            "Profile: {}",
            field_label(FieldId::PhysicsProfile, choice.readback.physics_profile)
        ),
        format!("Role: {}", choice.readback.role.label()),
        format!("Role groups: {}", choice.readback.role_groups.label()),
        format!("Speed: {}", SPEED_PRESETS[choice.readback.speed].1),
    ];
    rows.extend(compose::authored_rows(&choice.readback));
    if let Some(status) = choice.status {
        rows.push(format!("Physics status: {status}"));
    }
    if let Some(refusal) = choice.refusal {
        rows.push(format!("Physics refused: {refusal}"));
    }
    rows
}
