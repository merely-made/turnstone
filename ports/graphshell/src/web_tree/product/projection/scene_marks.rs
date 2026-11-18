// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Draw assigned field and relation forms over the shared scene geometry.
use super::*;
use sceno::{Scene, SpaceId, Vec2};
use scenograph::FieldForm;

pub(super) fn axis_id(axis: AxisKind) -> &'static str {
    match axis {
        AxisKind::Scope => "scope",
        AxisKind::Arrangement => "arrangement",
        AxisKind::Dynamics => "dynamics",
        AxisKind::NodeMark => "node-mark",
        AxisKind::RelationForm => "relation-form",
        AxisKind::FieldForm => "field-form",
        AxisKind::Frame => "frame",
    }
}

pub(super) fn axis_controls(page: &TreePage, p: &ProjectionControls) -> Child {
    let viewport = sceno::Rect::new(
        Vec2::ZERO,
        sceno::Size2::new(
            (page.size.0 as f32 - 420.0).max(280.0) - 24.0,
            (page.size.1 as f32 * 0.55).max(160.0),
        ),
    );
    let controls: Vec<Child> = [
        (AxisKind::NodeMark, "Node marks"),
        (AxisKind::RelationForm, "Relation forms"),
        (AxisKind::FieldForm, "Field forms"),
        (AxisKind::Frame, "Frames"),
    ]
    .into_iter()
    .map(|(axis, label)| {
        Box::new(
            button(label, move |page: &mut TreePage, _| {
                if let Some(product) = &mut page.product {
                    let p = &mut product.projection;
                    p.comparing = p.compare_slots.is_none_or(|(active, _)| active != axis);
                    p.compare_slots = p.comparing.then_some((axis, viewport));
                    p.compare_dynamics = false;
                    p.compare_column = 0;
                    p.compare_row = 0;
                    p.refresh_comparison();
                }
            })
            .attr("data-projection-axis", axis_id(axis))
            .attr(
                "aria-pressed",
                p.compare_slots
                    .is_some_and(|(active, _)| active == axis)
                    .to_string(),
            ),
        ) as Child
    })
    .collect();
    Box::new(
        el("div", controls)
            .attr("role", "group")
            .attr("aria-label", "Swatch axes"),
    )
}

fn bounded_frame(scene: &Scene) -> Option<sceno::Rect> {
    let frame = scene.frame.as_ref()?;
    if !matches!(frame.spec, sceno::Frame::Bounded { .. }) {
        return None;
    }
    frame.world_bounds(scene)
}

pub(super) fn bounded_frame_contains_items(scene: &Scene) -> bool {
    let Some(frame) = bounded_frame(scene) else {
        return false;
    };
    scene.items.iter().filter(|item| item.visible).all(|item| {
        let Some(world) = scene.to_world(item.space) else {
            return false;
        };
        let world = world.then(&item.transform);
        let Some(rect) = item.footprint.bounds() else {
            return false;
        };
        [
            (0.0, 0.0),
            (rect.size.w, 0.0),
            (0.0, rect.size.h),
            (rect.size.w, rect.size.h),
        ]
        .into_iter()
        .map(|(x, y)| world.apply(Vec2::new(rect.origin.x + x, rect.origin.y + y)))
        .all(|p| {
            p.x >= frame.origin.x - 0.01
                && p.y >= frame.origin.y - 0.01
                && p.x <= frame.origin.x + frame.size.w + 0.01
                && p.y <= frame.origin.y + frame.size.h + 0.01
        })
    })
}

pub(super) fn frame_outline(scene: &Scene, project: &impl Fn(Vec2) -> [f32; 2]) -> Option<Child> {
    let bounds = bounded_frame(scene)?;
    let [x, y] = project(bounds.origin);
    let [right, bottom] = project(Vec2::new(
        bounds.origin.x + bounds.size.w,
        bounds.origin.y + bounds.size.h,
    ));
    Some(Box::new(el("div", "").attr("aria-hidden", "true").attr("data-projection-mark", "frame")
        .attr("style", format!("position:absolute;pointer-events:none;left:{x}px;top:{y}px;width:{}px;height:{}px;border:1px dashed #82a997;box-sizing:border-box;", right-x, bottom-y))))
}

pub(super) fn fields(
    scene: &Scene,
    project: &impl Fn(Vec2) -> [f32; 2],
    form_at: impl Fn(SpaceId) -> Option<FieldForm>,
) -> Vec<Child> {
    let mut children = Vec::new();
    for region in &scene.regions {
        let (Some(form), Some(sceno::Footprint::Polygon { points }), Some(world)) = (
            form_at(region.space),
            &region.contour,
            scene.to_world(region.space),
        ) else {
            continue;
        };
        if points.len() < 3 {
            continue;
        }
        let points: Vec<_> = points.iter().map(|p| project(world.apply(*p))).collect();
        match form {
            FieldForm::Contour => {
                for (&a, &b) in points.iter().zip(points.iter().cycle().skip(1)) {
                    children.push(line(a, b, "field", "#82a997"));
                }
            },
            FieldForm::Backdrop => {
                let (min_x, min_y, max_x, max_y) = points.iter().fold(
                    (
                        f32::INFINITY,
                        f32::INFINITY,
                        f32::NEG_INFINITY,
                        f32::NEG_INFINITY,
                    ),
                    |(min_x, min_y, max_x, max_y), &[x, y]| {
                        (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
                    },
                );
                let (w, h) = (max_x - min_x, max_y - min_y);
                if w <= 0.0 || h <= 0.0 {
                    continue;
                }
                let clip = points
                    .iter()
                    .map(|&[x, y]| {
                        format!("{}% {}%", (x - min_x) * 100.0 / w, (y - min_y) * 100.0 / h)
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                children.push(Box::new(el("div", "").attr("aria-hidden", "true")
                    .attr("data-projection-mark", "field")
                    .attr("style", format!("position:absolute;pointer-events:none;left:{min_x}px;top:{min_y}px;width:{w}px;height:{h}px;clip-path:polygon({clip});background:#3b5c52;"))) as Child);
            },
            FieldForm::Guide => {
                if let Some(label) = region.label.as_ref() {
                    let [x, y] = points
                        .iter()
                        .copied()
                        .min_by(|a, b| a[1].total_cmp(&b[1]))
                        .unwrap();
                    children.push(Box::new(el("span", label.clone())
                        .attr("data-projection-mark", "field")
                        .attr("style", format!("position:absolute;pointer-events:none;left:{x}px;top:{y}px;color:#b9d6c9;"))) as Child);
                }
            },
            FieldForm::MemberTint => {},
        }
    }
    children
}

pub(super) fn relations(scene: &Scene, project: &impl Fn(Vec2) -> [f32; 2]) -> Vec<Child> {
    let mut children = Vec::new();
    for (index, relation) in scene.relations.iter().enumerate() {
        let Some(world) = scene.to_world(relation.space) else {
            continue;
        };
        match relation.form {
            sceno::RelationForm::Stroke => {
                for segment in relation.points.windows(2) {
                    children.push(line(
                        project(world.apply(segment[0])),
                        project(world.apply(segment[1])),
                        "stroke",
                        "#dbb86b",
                    ));
                }
            },
            sceno::RelationForm::Adjacency => {
                let honored = scene
                    .relation_constraints
                    .get(index)
                    .is_some_and(|report| report.honored);
                let (mark, color) = if honored {
                    ("border", "#84cbb0")
                } else {
                    ("unmet", "#dbb86b")
                };
                for segment in relation.points.windows(2) {
                    children.push(line(
                        project(world.apply(segment[0])),
                        project(world.apply(segment[1])),
                        mark,
                        color,
                    ));
                }
            },
            sceno::RelationForm::MatrixCell => {
                let Some(cell) = relation.cell_bounds() else {
                    continue;
                };
                let [x, y] = project(world.apply(cell.origin));
                let [right, bottom] = project(world.apply(Vec2::new(
                    cell.origin.x + cell.size.w,
                    cell.origin.y + cell.size.h,
                )));
                let label = relation.kind.clone().unwrap_or_else(|| "Relation".into());
                children.push(Box::new(el("div", label).attr("data-projection-mark", "matrix")
                    .attr("style", format!("position:absolute;pointer-events:none;left:{x}px;top:{y}px;width:{}px;height:{}px;overflow:hidden;background:#755c32;", right-x, bottom-y))) as Child);
            },
            _ => {},
        }
    }
    children
}

fn line(
    [x, y]: [f32; 2],
    [right, bottom]: [f32; 2],
    mark: &'static str,
    color: &'static str,
) -> Child {
    let (dx, dy) = (right - x, bottom - y);
    Box::new(el("div", "").attr("aria-hidden", "true").attr("data-projection-mark", mark)
        .attr("style", format!("position:absolute;pointer-events:none;left:{x}px;top:{y}px;width:{}px;height:1px;background:{color};transform-origin:0 0;transform:rotate({}rad);", dx.hypot(dy),dy.atan2(dx))))
}
