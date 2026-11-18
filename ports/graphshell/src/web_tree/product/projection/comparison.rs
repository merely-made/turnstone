// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Realize the comparison scene with the same shared controls as its editor.
use super::*;
use graphshell::projection_compare::{FACET_AXIS_ADAPTER, FACET_CELL_ADAPTER};
use graphshell::projection_compile::practice_compiler;
use scenomise::arrangement::with_kind;

pub(super) fn view(p: &ProjectionControls, width: f32) -> Option<Child> {
    let comparison = p.comparison.as_ref()?;
    let scene = &comparison.scene;
    let bounds = scene.bounds;
    // Fixed-size pages preserve legible dynamics stop labels. The existing
    // whole Scope comparison keeps its previous layout.
    let paged = p.compare_dynamics
        || p.dynamics_comparison.is_some()
        || p.compare_column > 0
        || p.compare_row > 0;
    let left = if paged {
        28.0 + p.compare_column as f32 * 236.0
    } else {
        bounds.origin.x
    };
    let top = if paged {
        28.0 + p.compare_row as f32 * 166.0
    } else {
        bounds.origin.y
    };
    let shown_width = if paged { 456.0 } else { bounds.size.w };
    let shown_height = if paged { 150.0 } else { bounds.size.h };
    let scale = ((width - 24.0) / shown_width.max(1.0)).min(1.0);
    let mut children: Vec<Child> = Vec::new();
    let by_space: std::collections::HashMap<_, _> = comparison
        .cells
        .iter()
        .map(|cell| (format!("cell: {}", cell.swatch_id), cell.field_form))
        .collect();
    let form_at = |mut space: sceno::SpaceId| loop {
        let Some(entry) = scene.spaces.get(space.0 as usize) else {
            break None;
        };
        if let Some(form) = entry.name.as_ref().and_then(|name| by_space.get(name)) {
            break *form;
        }
        let Some(parent) = entry.parent else {
            break None;
        };
        space = parent;
    };
    let to_screen = |point: sceno::Vec2| {
        [
            12.0 + (point.x - left) * scale,
            12.0 + (point.y - top) * scale,
        ]
    };
    children.extend(scene_marks::fields(scene, &to_screen, |space| {
        form_at(space).filter(|form| *form == scenograph::FieldForm::Backdrop)
    }));
    let overlays = scene_marks::fields(scene, &to_screen, |space| {
        form_at(space).filter(|form| *form != scenograph::FieldForm::Backdrop)
    })
    .into_iter()
    .chain(scene_marks::relations(scene, &to_screen));
    for (index, item) in scene.items.iter().enumerate() {
        if !item.visible {
            continue;
        }
        let source = &scene.sources[item.source.0 as usize];
        let Some(footprint) = item.footprint.bounds() else {
            continue;
        };
        let world = scene
            .to_world(item.space)
            .unwrap_or(sceno::Transform2::IDENTITY)
            .then(&item.transform);
        let x = world.translate.x + footprint.origin.x * world.scale;
        let y = world.translate.y + footprint.origin.y * world.scale;
        if paged && (x < left || x >= left + shown_width || y < top || y >= top + shown_height) {
            continue;
        }
        let style = format!(
            "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;overflow:hidden;",
            12.0 + (x - left) * scale,
            12.0 + (y - top) * scale,
            footprint.size.w * world.scale * scale,
            footprint.size.h * world.scale * scale
        );
        if source.adapter == FACET_CELL_ADAPTER {
            let Some(cell) = comparison
                .cells
                .iter()
                .find(|cell| cell.swatch_id == source.id)
            else {
                continue;
            };
            let family = cell.family.clone();
            let label = cell.label.clone();
            let choice = cell.choice.clone();
            let working = cell.working;
            let scope = comparison
                .facet
                .rows
                .as_ref()
                .and_then(|axis| axis.labels.get(cell.row))
                .map_or("All", String::as_str);
            let dynamics = cell.dynamics.clone();
            let detail = p
                .dynamics_comparison
                .as_ref()
                .and_then(|preview| {
                    if working {
                        preview.working_detail(p.motion_paused)
                    } else {
                        preview
                            .stops
                            .iter()
                            .find(|(id, _)| id == &cell.swatch_id)
                            .map(|(_, stop)| stop.clone())
                    }
                })
                .unwrap_or_default();
            children.push(Box::new(
                button(
                    format!(
                        "{}{} · {scope} · {detail}",
                        label,
                        if working { " (working)" } else { "" }
                    ),
                    move |page: &mut TreePage, _| {
                        if working {
                            return;
                        }
                        let Some(product) = &mut page.product else {
                            return;
                        };
                        let p = &mut product.projection;
                        let action = if let Some(choice) = &choice {
                            EditorAction::ApplySwatch(choice.clone())
                        } else {
                            let arrangement = with_kind(
                                &p.editor.draft().arrangement,
                                &family,
                                practice_compiler().registry(),
                            );
                            EditorAction::ApplyComparison {
                                arrangement,
                                dynamics: dynamics.clone(),
                            }
                        };
                        p.editor.break_run();
                        p.editor.reduce(action, js_sys::Date::now() as u64);
                        p.editor.break_run();
                        p.compare_column = 0;
                        p.compare_row = 0;
                        p.refresh();
                        p.sync_fields();
                        if p.compiled.is_some() {
                            p.status = format!("Applied · {label}");
                        }
                    },
                )
                .attr("data-projection-compare-cell", cell.swatch_id.clone())
                .attr("aria-pressed", working.to_string())
                .attr("style", style),
            ));
        } else if source.adapter == FACET_AXIS_ADAPTER {
            let label = source
                .id
                .split_once(':')
                .and_then(|(axis, index)| {
                    let index: usize = index.parse().ok()?;
                    match axis {
                        "columns" => comparison.facet.columns.labels.get(index).cloned(),
                        "rows" => comparison.facet.rows.as_ref()?.labels.get(index).cloned(),
                        _ => None,
                    }
                })
                .unwrap_or_default();
            children.push(Box::new(el("span", label).attr("style", style)));
        } else {
            let card = item.representation == sceno::Representation::Card;
            let tint = form_at(item.space) == Some(scenograph::FieldForm::MemberTint)
                && scene
                    .regions
                    .iter()
                    .any(|region| region.members.contains(&sceno::InstanceId(index as u32)));
            let fill = if tint {
                "#31594f"
            } else if card {
                "#385565"
            } else {
                "#263d49"
            };
            let mut style = format!(
                "{style}box-sizing:border-box;border:1px solid #879cab;background:{fill};pointer-events:none;"
            );
            super::assigned_clip(&mut style, &item.footprint, footprint);
            let label = if card {
                p.dataset
                    .as_ref()
                    .and_then(|data| data.occurrences.iter().find(|row| row.source == *source))
                    .and_then(|row| match p.editor.draft().encoding.label.as_ref() {
                        Some(Channel::Field(field)) => row.values.get(field),
                        _ => None,
                    })
                    .and_then(|value| match value {
                        scenomise::projection::ProjectionValue::Text(label) => Some(label.clone()),
                        _ => None,
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            children.push(Box::new(
                el("div", label)
                    .attr("aria-hidden", "true")
                    .attr("style", style),
            ));
        }
    }
    for (column, family) in comparison.facet.columns.labels.iter().enumerate() {
        for row in 0..comparison
            .facet
            .rows
            .as_ref()
            .map_or(1, |axis| axis.labels.len())
        {
            if comparison
                .cells
                .iter()
                .any(|cell| cell.row == row && cell.column == column)
            {
                continue;
            }
            if paged
                && (column < p.compare_column
                    || column >= p.compare_column + 2
                    || row != p.compare_row)
            {
                continue;
            }
            let row_label = comparison
                .facet
                .rows
                .as_ref()
                .and_then(|axis| axis.labels.get(row));
            let key =
                row_label.map_or_else(|| family.clone(), |label| format!("{family} / {label}"));
            let reason = comparison
                .refused
                .iter()
                .find(|(id, _)| id == &key || id == family)
                .map_or("Unavailable for this input", |(_, reason)| reason.as_str());
            children.push(Box::new(el("button", format!("{key} · Refused: {reason}"))
                .attr("disabled", "").attr("aria-disabled", "true")
                .attr("data-projection-compare-cell", format!("refused:{column}:{row}"))
                .attr("style", format!("position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;overflow:hidden;",
                    12.0 + (28.0 + column as f32 * 236.0 - left) * scale,
                    12.0 + (28.0 + row as f32 * 166.0 - top) * scale,
                    220.0 * scale, 150.0 * scale))));
        }
    }
    children.extend(overlays);
    let refused: Vec<Child> = if p.compare_slots.is_some() {
        comparison
            .refused
            .iter()
            .map(|(name, reason)| Box::new(el("li", format!("{name} · {reason}"))) as Child)
            .collect()
    } else {
        Vec::new()
    };
    Some(Box::new(
        el(
            "div",
            (
                el("div", children)
                    .attr("aria-label", "Projection comparison grid")
                    .attr(
                        "style",
                        format!(
                            "position:relative;overflow:hidden;width:{width}px;height:{}px;",
                            shown_height * scale + 24.0
                        ),
                    ),
                el("ul", refused).attr("aria-label", "Unavailable swatches"),
            ),
        )
        .attr("aria-label", "Arrangement comparison"),
    ))
}
