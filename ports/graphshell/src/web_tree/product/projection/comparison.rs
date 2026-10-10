// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Realize the comparison scene with the same shared controls as its editor.
use super::*;
use graphshell::projection_compare::{FACET_AXIS_ADAPTER, FACET_CELL_ADAPTER};
use graphshell::projection_compile::practice_compiler;
use graphshell::projection_editor::with_kind;

pub(super) fn view(p: &ProjectionControls, width: f32) -> Option<Child> {
    let comparison = p.comparison.as_ref()?;
    let scene = &comparison.scene;
    let bounds = scene.bounds;
    let scale = ((width - 24.0) / bounds.size.w.max(1.0)).min(1.0);
    let mut children: Vec<Child> = Vec::new();
    for item in &scene.items {
        let source = &scene.sources[item.source.0 as usize];
        let Some(footprint) = item.footprint.bounds() else {
            continue;
        };
        let world = scene
            .to_world(item.space)
            .unwrap_or(sceno::Transform2::IDENTITY)
            .then(&item.transform);
        let style = format!(
            "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;overflow:hidden;",
            12.0 + (world.translate.x + footprint.origin.x * world.scale - bounds.origin.x) * scale,
            12.0 + (world.translate.y + footprint.origin.y * world.scale - bounds.origin.y) * scale,
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
            let working = cell.working;
            let scope = if cell.row == 0 { "All" } else { "Selected" };
            children.push(Box::new(
                button(
                    format!(
                        "{}{} · {scope}",
                        family,
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
                        let arrangement = with_kind(
                            &p.editor.draft().arrangement,
                            &family,
                            practice_compiler().registry(),
                        );
                        p.editor.reduce(
                            EditorAction::SetArrangement(arrangement),
                            js_sys::Date::now() as u64,
                        );
                        p.editor.break_run();
                        p.refresh();
                        p.sync_fields();
                        p.status = format!("Applied · {family}");
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
            children.push(Box::new(el("div", "").attr("aria-hidden", "true").attr(
                "style",
                format!("{style}border:1px solid #879cab;background:#263d49;pointer-events:none;"),
            )));
        }
    }
    Some(Box::new(
        el("div", children)
            .attr("aria-label", "Arrangement comparison")
            .attr(
                "style",
                format!(
                    "position:relative;width:{width}px;height:{}px;",
                    bounds.size.h * scale + 24.0
                ),
            ),
    ))
}
