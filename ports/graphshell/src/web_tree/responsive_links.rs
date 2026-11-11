// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Visible reference fixture over real source readings and temporary geometry.
use super::*;
use cambium::button_with;
use graphshell::responsive_links::{FieldKind, ResponsiveLinksFixture};

pub(super) fn open(root: &Element) -> Result<Option<ResponsiveLinksFixture>, String> {
    if !root.has_attribute("data-responsive-links") {
        return Ok(None);
    }
    ResponsiveLinksFixture::new()
        .map(Some)
        .map_err(|error| format!("Responsive link fixture refused: {error:?}"))
}

fn field_name(field: FieldKind) -> &'static str {
    match field {
        FieldKind::Attractor => "attractor",
        FieldKind::Flow => "flow",
        FieldKind::Vortex => "vortex",
    }
}

fn line(children: &mut Vec<Child>, a: (f32, f32), b: (f32, f32), color: &str, width: f32) {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    children.push(Box::new(el("div", "").attr("aria-hidden", "true").attr(
        "style",
        format!(
            "position:absolute;left:{}px;top:{}px;width:{}px;height:{width}px;background:{color};transform:rotate({}rad);transform-origin:0 0;pointer-events:none;",
            a.0, a.1, dx.hypot(dy), dy.atan2(dx)
        ),
    )));
}

pub(super) fn view(page: &TreePage) -> Option<Child> {
    let model = page.responsive_links.as_ref()?;
    let snapshot = match model.evaluate() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return Some(Box::new(
                el("p", format!("Responsive link fixture refused: {error:?}"))
                    .attr("role", "alert"),
            ));
        },
    };
    let available_width = page.size.0.saturating_sub(32).max(1).min(960) as f32;
    let available_height = page.size.1.saturating_sub(360).max(180) as f32;
    let scale = (available_width / 760.0).min(available_height / 500.0);
    let width = 760.0 * scale;
    let point = |point: (f32, f32)| (point.0 * scale, point.1 * scale);
    let mut chart: Vec<Child> = Vec::new();
    let mut controls: Vec<Child> = Vec::new();
    for (field, label) in [
        (FieldKind::Attractor, "Attractor"),
        (FieldKind::Flow, "Flow"),
        (FieldKind::Vortex, "Vortex"),
    ] {
        controls.push(Box::new(
            button_with(label, move |page: &mut TreePage, _| {
                if let Some(model) = &mut page.responsive_links {
                    model.set_field(field);
                }
            })
            .attr("aria-pressed", (snapshot.field == field).to_string())
            .attr("data-responsive-field", field_name(field)),
        ));
    }
    for (field_toggle, label, enabled) in [
        (true, "Field response", snapshot.field_response_enabled),
        (
            false,
            "Compatible-link attraction",
            snapshot.compatible_attraction_enabled,
        ),
    ] {
        controls.push(Box::new(
            button_with(label, move |page: &mut TreePage, _| {
                if let Some(model) = &mut page.responsive_links {
                    if field_toggle {
                        model.set_field_response_enabled(!model.field_response_enabled());
                    } else {
                        model.set_compatible_attraction_enabled(
                            !model.compatible_attraction_enabled(),
                        );
                    }
                }
            })
            .attr("aria-pressed", enabled.to_string())
            .attr(
                "data-responsive-toggle",
                if field_toggle { "field" } else { "attraction" },
            ),
        ));
    }
    // Muted chords preserve the no-response comparison in every setting.
    for appearance in &snapshot.paths {
        if let (Some(start), Some(end)) = (
            appearance.path.points.first(),
            appearance.path.points.last(),
        ) {
            line(&mut chart, point(*start), point(*end), "#344550", 1.0);
        }
    }
    // Direction-only glyphs disclose the configured field, independent of path response.
    for sample in &snapshot.field_samples {
        let length = sample.vector.0.hypot(sample.vector.1);
        if length <= f32::EPSILON {
            continue;
        }
        let center = point(sample.position);
        let direction = (sample.vector.0 / length, sample.vector.1 / length);
        let end = (
            center.0 + direction.0 * 12.0 * scale,
            center.1 + direction.1 * 12.0 * scale,
        );
        let color = if snapshot.field_response_enabled {
            "#527f91"
        } else {
            "#293d48"
        };
        line(&mut chart, center, end, color, 1.0);
        for sign in [-1.0_f32, 1.0] {
            let tail = (
                end.0 - direction.0 * 4.0 * scale + direction.1 * sign * 3.0 * scale,
                end.1 - direction.1 * 4.0 * scale - direction.0 * sign * 3.0 * scale,
            );
            line(&mut chart, tail, end, color, 1.0);
        }
    }
    for (appearance, color) in snapshot.paths.iter().zip(["#f0cd75", "#83d8d2", "#ef9b91"]) {
        for segment in appearance.path.points.windows(2) {
            line(&mut chart, point(segment[0]), point(segment[1]), color, 3.0);
        }
    }
    for endpoint in snapshot.endpoints {
        let (x, y) = point(endpoint.position);
        chart.push(Box::new(el("div", "").attr("aria-hidden", "true").attr(
            "style",
            format!("position:absolute;left:{}px;top:{}px;width:8px;height:8px;background:#e3eaf0;border-radius:4px;", x - 4.0, y - 4.0),
        )));
        chart.push(Box::new(el("span", endpoint.label).attr(
            "style",
            format!(
                "position:absolute;left:{}px;top:{}px;font-size:12px;color:#dce3e8;",
                x - 25.0,
                y + 10.0
            ),
        )));
    }
    let observations: Vec<Child> = snapshot
        .paths
        .iter()
        .map(|appearance| {
            Box::new(
                el(
                    "li",
                    format!(
                        "{} · maximum bend {:.2} units · endpoints {} · {}",
                        appearance.label,
                        appearance.max_chord_deviation,
                        if appearance.endpoints_preserved {
                            "fixed"
                        } else {
                            "changed"
                        },
                        appearance.assertion_ids.join(", ")
                    ),
                )
                .attr("style", "margin:5px 0;overflow-wrap:anywhere;"),
            ) as Child
        })
        .collect();
    let settings = snapshot.settings;
    Some(Box::new(
        el(
            "main",
            (
                el("h1", "Responsive links · fixed endpoints"),
                el("p", "Compare a field acting along each path with attraction between the two declared compatible links. The coral path is an incompatible control. Gray chords show the no-response baseline.").attr("style", "margin:6px 0;"),
                el("div", controls).attr("class", "tree-controls tools-switch"),
                el("p", format!("Configured: {} · field strength {} · field {} · attraction {} · {} samples · {} iterations · field gain {} · compatibility gain {} · max step {}", field_name(snapshot.field), snapshot.field_strength, snapshot.field_response_enabled, snapshot.compatible_attraction_enabled, settings.control_points, settings.iterations, settings.field_gain, settings.compatibility_gain, settings.max_step)).attr("style", "margin:6px 0;"),
                el("p", format!("{} · arrows show direction only", snapshot.field_description)).attr("style", "margin:6px 0;"),
                el("div", chart)
                    .attr("role", "img")
                    .attr("aria-label", "Three responsive source links with six fixed endpoints")
                    .attr("style", format!("position:relative;flex:0 0 auto;width:{width}px;height:{}px;background:#101b24;border:1px solid #344550;", 500.0 * scale)),
                el("h2", "Observed path geometry").attr("style", "font-size:16px;margin:10px 0 4px;"),
                el("ul", observations).attr("style", "margin:4px 0;padding-left:24px;"),
                el("p", snapshot.spring_share_note).attr("style", "margin:6px 0;"),
                el("p", "Source: Graphshell reference fixture v1. Geometry is temporary; all six endpoint identities and the three source assertion handles are retained.").attr("style", "margin:6px 0;"),
            ),
        )
        .attr("aria-label", "Responsive link dynamics fixture")
        .attr("style", format!("width:{}px;height:{}px;padding:12px;box-sizing:border-box;overflow-y:auto;", page.size.0, page.size.1)),
    ))
}

pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let Some(model) = &page.responsive_links else {
        return snapshot;
    };
    match model.evaluate() {
        Err(error) => snapshot.with_field("responsive-error", format!("{error:?}")),
        Ok(reading) => snapshot
            .with_field("responsive-field", field_name(reading.field))
            .with_field(
                "responsive-field-enabled",
                reading.field_response_enabled.to_string(),
            )
            .with_field(
                "responsive-attraction-enabled",
                reading.compatible_attraction_enabled.to_string(),
            )
            .with_field("responsive-paths", reading.paths.len().to_string())
            .with_field(
                "responsive-endpoints-fixed",
                reading
                    .paths
                    .iter()
                    .all(|path| path.endpoints_preserved)
                    .to_string(),
            )
            .with_field(
                "responsive-bends",
                reading
                    .paths
                    .iter()
                    .map(|path| format!("{:.3}", path.max_chord_deviation))
                    .collect::<Vec<_>>()
                    .join(","),
            )
            .with_field(
                "responsive-assertions",
                reading
                    .paths
                    .iter()
                    .flat_map(|path| path.assertion_ids.iter().copied())
                    .collect::<Vec<_>>()
                    .join("|"),
            )
            .with_field(
                "responsive-any-moved",
                reading
                    .paths
                    .iter()
                    .any(|path| path.max_chord_deviation > 0.01)
                    .to_string(),
            )
            .with_field(
                "responsive-control-moved",
                reading
                    .paths
                    .last()
                    .is_some_and(|path| path.max_chord_deviation > 0.01)
                    .to_string(),
            )
            .with_field("responsive-spring-share", "unavailable"),
    }
}
