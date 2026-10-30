// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn arrange_view(state: &ArrangeState) -> ArrangeView {
    let choice = &state.choice;
    let mut layouts = vec![(
        FREE_LAYOUT_ID.to_string(),
        "Free (physics alone)".to_string(),
    )];
    layouts.extend(catalog_options(CANVAS_LAYOUT_STRATEGIES));
    let profile_field = state.form.field(FieldId::PhysicsProfile);
    let Control::Choice {
        options,
        current: profile,
    } = profile_field.control
    else {
        unreachable!("PhysicsProfile is a choice")
    };
    let profiles = options
        .into_iter()
        .map(|(id, label)| {
            (
                id.to_string(),
                if id == "custom" {
                    "Custom".to_string()
                } else {
                    label.to_string()
                },
            )
        })
        .collect();
    let overlay_rows = PhysicsOverlay::ALL
        .iter()
        .map(|overlay| {
            let overlay = *overlay;
            compose::live_toggle_with_key(
                state.form.field(FieldId::PhysicsOverlay(overlay)),
                &format!("overlay.{}", overlay.id()),
                move |on| ArrangeIntent::SetOverlay {
                    id: overlay.id().to_string(),
                    on,
                },
            )
        })
        .collect::<Vec<_>>();
    Box::new(
        el::<_, ArrangeState, ArrangeIntent>(
            "div",
            (
                el::<_, ArrangeState, ArrangeIntent>(
                    "div",
                    (
                        section("Arrangement"),
                        choice_row(
                            "arrangement",
                            "Arrangement",
                            layouts,
                            &choice.layout,
                            ArrangeIntent::SetLayout,
                        ),
                        el::<_, ArrangeState, ArrangeIntent>(
                            "div",
                            format!(
                                "{} seed · {} regions",
                                if choice.saved_seed { "Saved" } else { "Recipe" },
                                choice.regions
                            ),
                        )
                        .attr("role", "status"),
                        VERBS.into_iter().map(scene_button).collect::<Vec<_>>(),
                    ),
                ),
                section("Physics"),
                el::<_, ArrangeState, ArrangeIntent>(
                    "div",
                    choice.refusal.clone().unwrap_or_default(),
                )
                .attr("role", "status")
                .attr("aria-live", "polite"),
                compose::live_choice_with_key(
                    state.form.field(FieldId::PhysicsLaw),
                    "physics.law",
                    ArrangeIntent::SetLaw,
                ),
                choice_row(
                    "physics.profile",
                    "Profile",
                    profiles,
                    profile,
                    ArrangeIntent::ApplyProfile,
                ),
                section("Overlays"),
                overlay_rows,
                section("Sources"),
                compose::live_choice_with_key(
                    state.form.field(FieldId::PhysicsKind),
                    "physics.kind-source",
                    ArrangeIntent::SetKindSource,
                ),
                compose::live_choice_with_key(
                    state.form.field(FieldId::PhysicsMass),
                    "physics.mass-source",
                    ArrangeIntent::SetMassSource,
                ),
                compose::live_choice_with_key(
                    state.form.field(FieldId::PhysicsDepth),
                    "physics.depth-source",
                    ArrangeIntent::SetDepthSource,
                ),
                compose::views(state),
                el::<_, ArrangeState, ArrangeIntent>(
                    "div",
                    choice.status.clone().unwrap_or_default(),
                )
                .attr("role", "status")
                .attr("aria-live", "polite")
                .attr("aria-label", "Physics status"),
            ),
        )
        .attr("class", "pane")
        .attr(
            "style",
            format!(
                "width: {}px; height: {}px;",
                state.viewport_w, state.viewport_h
            ),
        ),
    )
}
