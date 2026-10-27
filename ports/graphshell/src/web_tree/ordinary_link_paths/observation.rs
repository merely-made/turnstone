// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Read-only acceptance observations of the ordinary rendered paths.
use super::*;

pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let canvas = page.shared.canvas.borrow();
    let report = canvas.link_path_response_report();
    let paths = &report.paths;
    let bend = max_bend(paths);
    let grouped_bend = paths
        .iter()
        .filter(|path| report.settings.compatible_links.contains(&path.key))
        .map(|path| path.max_bend())
        .fold(0.0_f32, f32::max);
    let other_bend = paths
        .iter()
        .filter(|path| !report.settings.compatible_links.contains(&path.key))
        .map(|path| path.max_bend())
        .fold(0.0_f32, f32::max);
    snapshot
        .with_field(
            "link-path-state",
            format!("{:?}", report.state).to_lowercase(),
        )
        .with_field(
            "link-path-error",
            report
                .failure
                .as_ref()
                .map_or_else(String::new, |e| format!("{e:?}")),
        )
        .with_field(
            "link-path-field-response",
            report.settings.field_response.to_string(),
        )
        .with_field(
            "link-path-compatible-attraction",
            report.settings.compatible_attraction.to_string(),
        )
        .with_field(
            "link-path-selected-field-count",
            report.settings.selected_fields.len().to_string(),
        )
        .with_field(
            "link-path-group-count",
            report.settings.compatible_links.len().to_string(),
        )
        .with_field(
            "link-path-group-keys",
            report
                .settings
                .compatible_links
                .iter()
                .map(LinkKey::stable_id)
                .collect::<Vec<_>>()
                .join("|"),
        )
        .with_field("link-path-count", paths.len().to_string())
        .with_field(
            "link-path-responsive-count",
            report.responsive_paths.to_string(),
        )
        .with_field(
            "link-path-presentation-fallbacks",
            report.presentation_fallbacks.to_string(),
        )
        .with_field(
            "link-path-excluded-endpoints",
            report.excluded_endpoints.to_string(),
        )
        .with_field(
            "link-path-configured-field",
            configured_field(&report.settings),
        )
        .with_field("link-path-max-bend", format!("{bend:.3}"))
        .with_field(
            "link-path-compatible-max-bend",
            format!("{grouped_bend:.3}"),
        )
        .with_field("link-path-other-max-bend", format!("{other_bend:.3}"))
        .with_field(
            "link-path-geometry",
            paths
                .iter()
                .map(|path| {
                    format!(
                        "{}:{}",
                        path.key.stable_id(),
                        path.points
                            .iter()
                            .map(|(x, y)| format!("{x:.2},{y:.2}"))
                            .collect::<Vec<_>>()
                            .join(";"),
                    )
                })
                .collect::<Vec<_>>()
                .join("|"),
        )
        .with_field(
            "link-path-other-geometry",
            paths
                .iter()
                .filter(|path| !report.settings.compatible_links.contains(&path.key))
                .map(|path| {
                    format!(
                        "{}:{}",
                        path.key.stable_id(),
                        path.points
                            .iter()
                            .map(|(x, y)| format!("{x:.2},{y:.2}"))
                            .collect::<Vec<_>>()
                            .join(";"),
                    )
                })
                .collect::<Vec<_>>()
                .join("|"),
        )
        .with_field(
            "link-path-surface-geometry",
            paths
                .iter()
                .filter(|path| path.stratum == scenograph::link::LinkStratum::Surface)
                .map(|path| {
                    format!(
                        "{}:{}",
                        path.key.stable_id(),
                        path.points
                            .iter()
                            .map(|(x, y)| format!("{x:.2},{y:.2}"))
                            .collect::<Vec<_>>()
                            .join(";"),
                    )
                })
                .collect::<Vec<_>>()
                .join("|"),
        )
        .with_field(
            "link-path-keys",
            paths
                .iter()
                .map(|path| path.key.stable_id())
                .collect::<Vec<_>>()
                .join("|"),
        )
        .with_field(
            "link-path-endpoint-ids",
            paths
                .iter()
                .map(|path| format!("{}>{}", path.displayed_from, path.displayed_to))
                .collect::<Vec<_>>()
                .join("|"),
        )
        .with_field(
            "link-path-endpoints",
            paths
                .iter()
                .map(|path| {
                    let positions = [path.displayed_from, path.displayed_to].map(|id| {
                        canvas
                            .graph()
                            .get_node_key_by_id(id)
                            .and_then(|key| canvas.screen_position_of(key))
                            .map(|(x, y)| format!("{x:.2},{y:.2}"))
                            .unwrap_or_else(|| "missing".into())
                    });
                    format!(
                        "{}>{}:{}>{}",
                        path.displayed_from, path.displayed_to, positions[0], positions[1]
                    )
                })
                .collect::<Vec<_>>()
                .join("|"),
        )
        .with_field(
            "link-path-source-assertions",
            paths
                .iter()
                .flat_map(|path| {
                    path.buckets
                        .iter()
                        .flat_map(|b| b.assertion_ids.iter().cloned())
                })
                .collect::<Vec<_>>()
                .join("|"),
        )
        .with_field(
            "link-path-responsive-keys",
            paths
                .iter()
                .filter(|path| path.responsive)
                .map(|path| path.key.stable_id())
                .collect::<Vec<_>>()
                .join("|"),
        )
        .with_field(
            "link-path-ineligible-self-links",
            report.ineligible_self_links.to_string(),
        )
}
