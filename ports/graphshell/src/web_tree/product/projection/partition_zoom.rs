// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Partition zoom controls and the measured remainder in the spatial preview.
use super::*;
use sceno::{Footprint, PartitionForm, Rect, Region, Vec2};

pub(super) fn interaction(p: &ProjectionControls) -> Child {
    let draft = p.editor.draft();
    let zoom = draft.interaction.zoom;
    let family = scenomise::catalog::Family::resolve(&draft.arrangement.kind);
    if !zoom && family != Some(scenomise::catalog::Family::Partition) {
        return Box::new(el(
            "p",
            "Selection, pan, and zoom follow the viewer's interaction policy.",
        ));
    }
    Box::new(
        button("Partition zoom", |page: &mut TreePage, _| {
            if let Some(product) = &mut page.product {
                let p = &mut product.projection;
                let mut interaction = p.editor.draft().interaction.clone();
                interaction.zoom = !interaction.zoom;
                p.editor.break_run();
                p.editor.reduce(
                    EditorAction::SetInteraction(interaction),
                    js_sys::Date::now() as u64,
                );
                p.editor.break_run();
                p.refresh();
            }
        })
        .attr("aria-pressed", zoom.to_string())
        .attr("data-partition-zoom-policy", ""),
    )
}

pub(super) struct PartitionView<'a> {
    compiled: &'a CompiledProjection,
    form: PartitionForm,
    bounds: Rect,
}

impl<'a> PartitionView<'a> {
    fn new(compiled: &'a CompiledProjection) -> Option<Self> {
        let sceno::Arrangement::Partition(config) = &compiled.score.arrangement else {
            return None;
        };
        let scene = &compiled.scene;
        let bounds = scene
            .frame
            .as_ref()
            .and_then(|frame| frame.world_bounds(scene))
            .unwrap_or(scene.bounds);
        Some(Self {
            compiled,
            form: config.form,
            bounds,
        })
    }

    fn share(&self, region: &Region) -> Option<f64> {
        let Some(Footprint::Polygon { points }) = &region.contour else {
            return None;
        };
        if points.len() < 3 || self.bounds.size.w <= 0.0 || self.bounds.size.h <= 0.0 {
            return None;
        }
        let cross =
            |a: Vec2, b: Vec2| f64::from(a.x) * f64::from(b.y) - f64::from(a.y) * f64::from(b.x);
        Some(match self.form {
            PartitionForm::Tiles => {
                let area = points
                    .iter()
                    .zip(points.iter().cycle().skip(1))
                    .map(|(&a, &b)| cross(a, b))
                    .sum::<f64>()
                    .abs()
                    / 2.0;
                area / (f64::from(self.bounds.size.w) * f64::from(self.bounds.size.h))
            },
            PartitionForm::Rings => {
                // The outer arc occupies the first half of a Partition sector.
                let center = Vec2::new(
                    self.bounds.origin.x + self.bounds.size.w / 2.0,
                    self.bounds.origin.y + self.bounds.size.h / 2.0,
                );
                points[..points.len() / 2]
                    .windows(2)
                    .map(|pair| {
                        let a = Vec2::new(pair[0].x - center.x, pair[0].y - center.y);
                        let b = Vec2::new(pair[1].x - center.x, pair[1].y - center.y);
                        let dot = f64::from(a.x) * f64::from(b.x) + f64::from(a.y) * f64::from(b.y);
                        cross(a, b).atan2(dot)
                    })
                    .sum::<f64>()
                    .abs()
                    / std::f64::consts::TAU
            },
        })
    }

    pub(super) fn controls(&self) -> Option<Child> {
        let scene = &self.compiled.scene;
        let focus = self.compiled.partition_focus();
        let mut controls: Vec<Child> = Vec::new();
        for region in &scene.regions {
            let (Some(name), Some(share)) = (&region.label, self.share(region)) else {
                continue;
            };
            if region.members.is_empty() {
                controls.push(Box::new(
                    el("span", format!("Remainder {}", percent_label(share)))
                        .attr("data-partition-remainder", ""),
                ));
            } else if region.members.len() < scene.items.len() {
                let group = name.clone();
                let active = Some(name.as_str()) == focus;
                controls.push(Box::new(
                    button(
                        format!("Zoom into {name} ({})", percent_label(share)),
                        move |page: &mut TreePage, _| {
                            set_focus(page, Some(&group));
                        },
                    )
                    .attr("data-partition-zoom-group", name.clone())
                    .attr("aria-pressed", active.to_string()),
                ));
            }
        }
        if scene
            .regions
            .iter()
            .any(|r| !r.members.is_empty() && r.members.len() < scene.items.len())
        {
            controls.push(Box::new(
                button("Zoom out", |page: &mut TreePage, _| set_focus(page, None))
                    .attr("data-partition-zoom-out", "")
                    .attr("aria-disabled", focus.is_none().to_string()),
            ));
        }
        (!controls.is_empty()).then(|| {
            Box::new(
                el("div", controls)
                    .attr("role", "group")
                    .attr("aria-label", "Partition view"),
            ) as Child
        })
    }

    pub(super) fn regions(&self, project: &impl Fn(Vec2) -> [f32; 2]) -> Vec<Child> {
        self.compiled.scene.regions.iter().filter_map(|region| {
            let Some(Footprint::Polygon { points }) = &region.contour else {
                return None;
            };
            let world = self.compiled.scene.to_world(region.space)?;
            let outline = Footprint::Polygon {
                points: points.iter().map(|p| {
                    let [x, y] = project(world.apply(*p));
                    Vec2::new(x, y)
                }).collect(),
            };
            let rect = outline.bounds()?;
            let focus = self.compiled.partition_focus();
            let paint = if region.members.is_empty() {
                "background:#283c48;background-image:repeating-linear-gradient(135deg,#2c404a 0 3px,#182c37 3px 9px);"
            } else if focus.is_some() && region.label.as_deref() == focus {
                "background:#36564888;"
            } else {
                "background:#2a4a5a66;"
            };
            let mut style = format!(
                "position:absolute;pointer-events:none;left:{}px;top:{}px;width:{}px;height:{}px;{paint}",
                rect.origin.x, rect.origin.y, rect.size.w, rect.size.h
            );
            assigned_clip(&mut style, &outline, rect);
            Some(Box::new(el("div", "").attr("aria-hidden", "true").attr("style",style)) as Child)
        }).collect()
    }
}

fn percent_label(share: f64) -> String {
    let percent = share * 100.0;
    if percent > 0.0 && percent < 0.05 {
        return "<0.1%".into();
    }
    let rounded = format!("{percent:.1}");
    format!("{}%", rounded.trim_end_matches(".0"))
}

fn set_focus(page: &mut TreePage, group: Option<&str>) {
    if let Some(product) = &mut page.product {
        let p = &mut product.projection;
        if p.compiled
            .as_ref()
            .and_then(CompiledProjection::partition_focus)
            != group
        {
            p.refresh_with_partition_focus(group);
        }
    }
}

pub(super) fn view<'a>(
    p: &ProjectionControls,
    compiled: &'a CompiledProjection,
) -> Option<PartitionView<'a>> {
    p.editor
        .draft()
        .interaction
        .zoom
        .then(|| PartitionView::new(compiled))
        .flatten()
}

pub(super) fn add_snapshot(
    snapshot: ProbeSnapshot,
    compiled: Option<&CompiledProjection>,
) -> ProbeSnapshot {
    let Some(view) = compiled.and_then(PartitionView::new) else {
        return snapshot;
    };
    let shares = view
        .compiled
        .scene
        .regions
        .iter()
        .filter_map(|region| {
            let name = region.label.as_ref()?;
            let share = view.share(region)?;
            Some(format!("{name}:{:.0}", share * 100.0))
        })
        .collect::<Vec<_>>()
        .join(",");
    snapshot
        .with_field(
            "projection-partition-form",
            match view.form {
                PartitionForm::Tiles => "tiles",
                PartitionForm::Rings => "rings",
            },
        )
        .with_field(
            "projection-partition-focus",
            view.compiled.partition_focus().unwrap_or("whole"),
        )
        .with_field("projection-partition-shares", shares)
        .with_field(
            "projection-partition-viewport",
            format!("{}x{}", view.bounds.size.w, view.bounds.size.h),
        )
}
