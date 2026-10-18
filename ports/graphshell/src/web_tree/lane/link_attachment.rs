// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Scenario-only attachment profiles for the ordinary `link-rest` graph.
//!
//! These profiles exercise real shared Canvas controls; they are scripted
//! qualification inputs, not ordinary user controls or persisted settings.

use super::*;
use mere::canvas::{NodeShape, responsive_links::LinkPathResponse};
use scenograph::suggestion::{SuggestionMark, SuggestionSettings};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Profile {
    Flat,
    Tilted,
    Rounded,
    Circle,
    Suggestion,
    Lifted,
}

impl Profile {
    pub(super) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "flat" => Ok(Self::Flat),
            "tilted" => Ok(Self::Tilted),
            "rounded" => Ok(Self::Rounded),
            "circle" => Ok(Self::Circle),
            "suggestion" => Ok(Self::Suggestion),
            "lifted" => Ok(Self::Lifted),
            _ => Err("link-attachment-profile wants flat, tilted, rounded, circle, suggestion, or lifted".into()),
        }
    }
}

/// Set a real shared-view profile only when the normal link-rest source fixture
/// is active. Each call starts from ordinary camera/shape/grammar/height state,
/// but preserves the caller's view-local responsive path settings.
pub(super) fn apply(shared: &Shared, profile: Profile) -> Result<(), String> {
    let canvas = shared.canvas.borrow();
    let source = super::link_rest_fields(&canvas, ProbeSnapshot::default());
    if source.field("link-rest-ready") != Some("true") {
        return Err("link attachment profiles require the ordinary link-rest source graph".into());
    }
    drop(canvas);

    let mut canvas = shared.canvas.borrow_mut();
    canvas.set_yaw(0.0);
    canvas.set_tilt(1.0);
    canvas.set_height_by_degree(false);
    canvas.set_node_shapes(HashMap::new());
    canvas.set_graph_grammar(false);
    canvas.offer_suggestions(std::iter::empty());
    canvas.set_suggestion_settings(SuggestionSettings::for_host("graphshell"));

    match profile {
        Profile::Flat => {},
        Profile::Tilted => {
            canvas.set_yaw(0.42);
            canvas.set_tilt(0.58);
        },
        Profile::Rounded | Profile::Circle => {
            let shape = if profile == Profile::Rounded {
                NodeShape::Rounded
            } else {
                NodeShape::Circle
            };
            let shapes = canvas
                .graph()
                .nodes()
                .map(|(_, node)| (node.id, shape))
                .collect();
            canvas.set_node_shapes(shapes);
        },
        Profile::Suggestion => {
            let first = canvas
                .graph()
                .nodes()
                .next()
                .map(|(_, node)| node.id)
                .ok_or("link-rest has no node to offer as a suggestion")?;
            canvas.set_graph_grammar(true);
            canvas.offer_suggestions([(first, "ordinary-link-attachment-profile".to_owned())]);
            canvas.set_suggestion_settings(SuggestionSettings {
                on: true,
                at_once: 2,
                tracked: 20,
                mark: SuggestionMark::Ring,
                sources: Vec::new(),
            });
        },
        Profile::Lifted => {
            canvas.set_yaw(0.42);
            canvas.set_tilt(0.58);
            canvas.set_height_by_degree(true);
        },
    }
    shared.dirty.set(true);
    Ok(())
}

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// Observe attachment through the same billboard model that paint and picking
/// use. Values are explicitly marked as scripted profile observations.
pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let canvas = page.shared.canvas.borrow();
    let report = canvas.link_path_response_report();
    let mut endpoint_rows = Vec::new();
    let mut invalid = 0usize;
    let mut max_boundary_error = 0.0f32;
    let mut max_foot_error = 0.0f32;
    let mut interior_violations = 0usize;
    let mut max_stem = 0.0f32;
    let mut responsive_count = 0usize;
    let mut visible_body_count = 0usize;
    let mut body_readback_missing = 0usize;
    let mut max_body_rect_error = 0.0f32;
    let mut body_readback_error = None;

    // Compare the shared billboard projection against Livery's hit-tested body
    // bounds for every currently visible node. This is independent of the path
    // endpoint checks below and catches a model/paint mismatch even when no
    // responsive path happens to touch that node.
    for (key, _) in canvas.graph().nodes() {
        if !canvas.node_in_scope(key) {
            continue;
        }
        let Some(model) = canvas.node_billboard(key) else {
            body_readback_missing += 1;
            body_readback_error
                .get_or_insert_with(|| format!("visible node {key:?} has no projected billboard"));
            continue;
        };
        visible_body_count += 1;
        let Some((min_x, min_y, max_x, max_y)) = canvas.node_body_rect(key) else {
            body_readback_missing += 1;
            body_readback_error.get_or_insert_with(|| {
                format!("visible node {key:?} has no Livery body-rectangle readback")
            });
            continue;
        };
        let rect = model.screen_rect();
        let edges = [
            (rect.min.x, min_x),
            (rect.min.y, min_y),
            (rect.max.x, max_x),
            (rect.max.y, max_y),
        ];
        for (expected, actual) in edges {
            let error = (expected - actual).abs();
            if error.is_finite() {
                max_body_rect_error = max_body_rect_error.max(error);
            } else {
                body_readback_missing += 1;
                body_readback_error.get_or_insert_with(|| {
                    format!("visible node {key:?} has non-finite body-rectangle error")
                });
            }
        }
    }
    if visible_body_count == 0 {
        body_readback_error
            .get_or_insert_with(|| "no visible nodes have projected billboards".into());
        body_readback_missing = 1;
    }

    for path in &report.paths {
        if !path.responsive {
            continue;
        }
        responsive_count += 1;
        let (Some(&first), Some(&second), Some(&last), Some(&previous)) = (
            path.points.first(),
            path.points.get(1),
            path.points.last(),
            path.points.get(path.points.len().saturating_sub(2)),
        ) else {
            invalid += 1;
            continue;
        };
        let endpoint_data = [
            (path.displayed_from, first, second),
            (path.displayed_to, last, previous),
        ];
        for (id, endpoint, next) in endpoint_data {
            let Some(key) = canvas.graph().get_node_key_by_id(id) else {
                invalid += 1;
                continue;
            };
            let Some(billboard) = canvas.node_billboard(key) else {
                invalid += 1;
                continue;
            };
            let screen = |point: (f32, f32)| canvas.screen_point_of(point);
            let endpoint_screen = screen(endpoint);
            let next_screen = screen(next);
            let point = endpoint_screen.into();
            let next_point = next_screen.into();
            let ground = (billboard.ground_anchor.x, billboard.ground_anchor.y);
            let center = (billboard.center.x, billboard.center.y);
            let has_stem = distance(ground, center) >= 0.5;
            let endpoint_error = if has_stem {
                distance(endpoint_screen, ground)
            } else {
                billboard
                    .signed_distance_screen(point)
                    .map_or(f32::INFINITY, f32::abs)
            };
            if !endpoint_error.is_finite() {
                invalid += 1;
            } else if has_stem {
                max_foot_error = max_foot_error.max(endpoint_error);
            } else {
                max_boundary_error = max_boundary_error.max(endpoint_error);
            }
            max_stem = max_stem.max(distance(ground, center));
            // Exercise the canonical silhouette segment clipper for each actual
            // path endpoint segment; the encoded interval is readback only.
            let interval = billboard.segment_interior_interval(point, next_point);
            let ordered_points = if id == path.displayed_from {
                path.points.clone()
            } else {
                path.points.iter().rev().copied().collect()
            };
            interior_violations += strict_reentry(&ordered_points, &canvas, billboard, has_stem);
            endpoint_rows.push(format!(
                "{:?}:{}:{}:{}:{:.3}:{:.3}:{:.3}:{:.3}:{:.3}",
                path.stratum,
                id,
                interval.map_or_else(|| "none".to_owned(), |(a, b)| format!("{a:.3},{b:.3}")),
                has_stem,
                endpoint_error,
                distance(ground, center),
                billboard.local_side,
                billboard.zoom,
                billboard.corner_radius,
            ));
        }
    }
    endpoint_rows.sort();
    let camera = canvas.viewport();
    snapshot
        .with_field("link-attachment-profile", "scripted-shared-view")
        .with_field(
            "link-attachment-state",
            format!("{:?}", report.state).to_lowercase(),
        )
        .with_field(
            "link-attachment-profile-name",
            current_profile_label(&canvas),
        )
        .with_field(
            "link-attachment-camera",
            format!(
                "{:.3},{:.3},{:.3},{:.3},{:.3}",
                camera.yaw, camera.tilt, camera.zoom, camera.offset.0, camera.offset.1
            ),
        )
        .with_field("link-attachment-endpoints", endpoint_rows.join("|"))
        .with_field("link-attachment-invalid", invalid.to_string())
        .with_field(
            "link-attachment-visible-body-count",
            visible_body_count.to_string(),
        )
        .with_field(
            "link-attachment-body-readback-missing",
            body_readback_missing.to_string(),
        )
        .with_field(
            "link-attachment-body-readback-error",
            body_readback_error.as_deref().unwrap_or("none"),
        )
        .with_field(
            "link-attachment-max-body-rect-error",
            if body_readback_missing == 0 {
                format!("{max_body_rect_error:.3}")
            } else {
                // Keep the numeric gate fail-closed while the separate error
                // field explains why actual readback was unavailable.
                "999.000".to_owned()
            },
        )
        .with_field(
            "link-attachment-responsive-paths",
            responsive_count.to_string(),
        )
        .with_field(
            "link-attachment-max-boundary-error",
            format!("{max_boundary_error:.3}"),
        )
        .with_field(
            "link-attachment-max-foot-error",
            format!("{max_foot_error:.3}"),
        )
        .with_field(
            "link-attachment-interior-violations",
            interior_violations.to_string(),
        )
        .with_field("link-attachment-max-stem", format!("{max_stem:.3}"))
        .with_field(
            "link-attachment-height",
            canvas.height_by_degree().to_string(),
        )
        .with_field(
            "link-attachment-grammar",
            canvas.graph_grammar().to_string(),
        )
        .with_field(
            "link-attachment-suggestions",
            canvas.suggestion_settings().on.to_string(),
        )
}

fn strict_reentry(
    world_points: &[(f32, f32)],
    canvas: &mere::canvas::Canvas,
    billboard: mere::canvas::node_billboard::NodeBillboard,
    allow_initial_stem_exit: bool,
) -> usize {
    let points = world_points
        .iter()
        .map(|&point| canvas.screen_point_of(point))
        .collect::<Vec<_>>();
    let mut exited = !points
        .first()
        .is_some_and(|point| billboard.strictly_contains_screen((*point).into()));
    let mut violations = 0usize;
    for pair in points.windows(2) {
        if billboard.segment_crosses_interior(pair[0].into(), pair[1].into()) {
            if allow_initial_stem_exit && !exited {
                exited = !billboard.strictly_contains_screen(pair[1].into());
            } else {
                violations += 1;
            }
        }
        if !billboard.strictly_contains_screen(pair[1].into()) {
            exited = true;
        }
    }
    violations
}

fn current_profile_label(canvas: &mere::canvas::Canvas) -> &'static str {
    if canvas.suggestion_settings().on {
        return "suggestion";
    }
    if canvas.height_by_degree() {
        return "lifted";
    }
    let camera = canvas.viewport();
    if (camera.yaw - 0.42).abs() < 0.001 && (camera.tilt - 0.58).abs() < 0.001 {
        return "tilted";
    }
    if let Some((key, _)) = canvas.graph().nodes().next() {
        match canvas.node_shape(key) {
            NodeShape::Rounded => return "rounded",
            NodeShape::Circle => return "circle",
            NodeShape::Square => {},
        }
    }
    "flat"
}
