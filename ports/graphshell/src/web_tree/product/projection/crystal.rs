// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Controls for live Crystal placement in the projection preview.
use super::*;
use cambium::{
    Key, KeyEvent, NamedKey, PointerButton, PointerEvent, PointerPhase, on_key, on_pointer,
};
use sceno::{Footprint, Scene, Vec2};
use scenomise::crystal::{CrystalEdit, edit};

pub(super) struct Drag {
    occurrence: String,
    pointer: i32,
    start: (f32, f32),
    origin: Vec2,
    at: Vec2,
    scale: f32,
}

pub(super) fn pinned(scene: &Scene, tile: InstanceId) -> bool {
    scene
        .honored_holds
        .iter()
        .any(|hold| hold.instance == tile && hold.placement.hold == sceno::Hold::Pinned)
}

pub(super) fn tile_style(scene: &Scene, tile: InstanceId) -> &'static str {
    if pinned(scene, tile) {
        "background:#31594f;box-shadow:inset 0 0 0 2px #84cbb0;touch-action:none;"
    } else {
        "background:#253f50;box-shadow:inset 0 0 0 1px #879cab;touch-action:none;"
    }
}

pub(super) fn controls(compiled: &CompiledProjection) -> Child {
    let mut controls: Vec<Child> = Vec::new();
    if let Some(tile) = compiled.selected
        && let Some(occurrence) = compiled.occurrence_by_instance.get(&tile)
    {
        let occurrence = occurrence.clone();
        let held = pinned(&compiled.scene, tile);
        let label = if held {
            "Unpin selected tile"
        } else {
            "Pin selected tile"
        };
        controls.push(Box::new(button(label, move |page: &mut TreePage, _| {
            if let Some(product) = &mut page.product {
                let p = &mut product.projection;
                let Some(tile) = p
                    .compiled
                    .as_ref()
                    .and_then(|c| c.instance_by_occurrence.get(&occurrence))
                    .copied()
                else {
                    return;
                };
                apply(
                    p,
                    if held {
                        CrystalEdit::Unpin(tile)
                    } else {
                        CrystalEdit::Pin(tile)
                    },
                );
            }
        })));
        controls.push(Box::new(button(
            "Clear tile selection",
            |page: &mut TreePage, _| {
                if let Some(product) = &mut page.product {
                    product.projection.selected = None;
                    product.projection.refresh();
                }
            },
        )));
    }
    Box::new(
        el("div", controls)
            .attr("role", "group")
            .attr("aria-label", "Crystal controls"),
    )
}

pub(super) fn tile<V>(
    control: V,
    occurrence: String,
    scale: f32,
    subject: Option<contexts::Subject>,
) -> Child
where
    V: cambium::ElementView<TreePage, ()> + 'static,
{
    let drag_tile = occurrence.clone();
    let pointed = subject.clone();
    let keyed = subject.clone();
    let control = on_key(
        on_pointer(control, move |page: &mut TreePage, event: PointerEvent| {
            if pointed
                .as_ref()
                .is_some_and(|subject| contexts::request_pointer(page, subject, &event))
            {
                if let Some(product) = &mut page.product {
                    product.projection.crystal_drag = None;
                }
                return;
            }
            if let Some(product) = &mut page.product {
                pointer(&mut product.projection, &drag_tile, event, scale);
            }
        }),
        move |page: &mut TreePage, event: KeyEvent| {
            if keyed
                .as_ref()
                .is_some_and(|subject| contexts::request_key(page, subject, &event))
            {
                return;
            }
            if let Some(product) = &mut page.product {
                nudge(&mut product.projection, &occurrence, event);
            }
        },
    );
    match subject {
        Some(subject) => Box::new(contexts::focus_and_hover(control, subject)),
        None => Box::new(control),
    }
}

fn apply(p: &mut ProjectionControls, action: CrystalEdit) {
    let Some(compiled) = &mut p.compiled else {
        return;
    };
    p.status = match edit(compiled, action) {
        Ok(()) => match action {
            CrystalEdit::Pin(_) => "Tile pinned",
            CrystalEdit::Unpin(_) => "Tile unpinned",
            CrystalEdit::Drop { .. } => "Tile moved",
        }
        .into(),
        Err(problem) => problem.into(),
    };
}

fn nudge(p: &mut ProjectionControls, occurrence: &str, event: KeyEvent) {
    let delta = match &event.key {
        Key::Named(NamedKey::ArrowLeft) => Vec2::new(-1.0, 0.0),
        Key::Named(NamedKey::ArrowRight) => Vec2::new(1.0, 0.0),
        Key::Named(NamedKey::ArrowUp) => Vec2::new(0.0, -1.0),
        Key::Named(NamedKey::ArrowDown) => Vec2::new(0.0, 1.0),
        _ => return,
    };
    let Some(c) = &p.compiled else { return };
    let sceno::Arrangement::Crystal(config) = &c.score.arrangement else {
        return;
    };
    let Some(&tile) = c.instance_by_occurrence.get(occurrence) else {
        return;
    };
    let from = c.scene.items[tile.0 as usize].transform.translate;
    let step = config.smallest_side / 2.0;
    event.prevent_default();
    event.stop_propagation();
    apply(
        p,
        CrystalEdit::Drop {
            tile,
            at: Vec2::new(from.x + delta.x * step, from.y + delta.y * step),
        },
    );
}

fn pointer(p: &mut ProjectionControls, occurrence: &str, event: PointerEvent, scale: f32) {
    if event.button != PointerButton::Primary || scale <= 0.0 {
        return;
    }
    if event.phase == PointerPhase::Down {
        let Some(c) = &p.compiled else { return };
        let Some(&tile) = c.instance_by_occurrence.get(occurrence) else {
            return;
        };
        if !pinned(&c.scene, tile) {
            let origin = c.scene.items[tile.0 as usize].transform.translate;
            p.crystal_drag = Some(Drag {
                occurrence: occurrence.into(),
                pointer: event.pointer_id,
                start: event.local,
                origin,
                at: origin,
                scale,
            });
        }
        return;
    }
    let Some(drag) = &mut p.crystal_drag else {
        return;
    };
    if drag.pointer != event.pointer_id || drag.occurrence != occurrence {
        return;
    }
    let (x, y) = (event.local.0 - drag.start.0, event.local.1 - drag.start.1);
    drag.at = Vec2::new(
        drag.origin.x + x / drag.scale,
        drag.origin.y + y / drag.scale,
    );
    match event.phase {
        PointerPhase::Move => {},
        PointerPhase::Up => {
            let at = drag.at;
            p.crystal_drag = None;
            if x.hypot(y) >= 3.0 {
                event.prop.prevent_default();
                if let Some(&tile) = p
                    .compiled
                    .as_ref()
                    .and_then(|c| c.instance_by_occurrence.get(occurrence))
                {
                    apply(p, CrystalEdit::Drop { tile, at });
                }
            }
        },
        PointerPhase::Cancel => p.crystal_drag = None,
        PointerPhase::Down => {},
    }
}

pub(super) fn drag_preview(
    p: &ProjectionControls,
    compiled: &CompiledProjection,
    project: &impl Fn(Vec2) -> [f32; 2],
    scale: f32,
) -> Option<Child> {
    let drag = p
        .crystal_drag
        .as_ref()
        .filter(|drag| drag.at != drag.origin)?;
    let instance = compiled.instance_by_occurrence.get(&drag.occurrence)?;
    let Footprint::Rect { size } = compiled.scene.items[instance.0 as usize].footprint else {
        return None;
    };
    let [x, y] = project(Vec2::new(
        drag.at.x - size.w / 2.0,
        drag.at.y - size.h / 2.0,
    ));
    let style = format!(
        "position:absolute;pointer-events:none;left:{x}px;top:{y}px;width:{}px;height:{}px;box-sizing:border-box;border:1px dashed #84cbb0;",
        size.w * scale,
        size.h * scale
    );
    Some(Box::new(
        el("div", "")
            .attr("aria-hidden", "true")
            .attr("style", style),
    ))
}

pub(super) fn add_snapshot(
    mut snapshot: ProbeSnapshot,
    compiled: Option<&CompiledProjection>,
) -> ProbeSnapshot {
    let Some(c) =
        compiled.filter(|c| matches!(c.score.arrangement, sceno::Arrangement::Crystal(_)))
    else {
        return snapshot;
    };
    let scene = &c.scene;
    let borders = scene
        .relation_constraints
        .iter()
        .filter(|r| r.honored)
        .count();
    let strokes = scene
        .relations
        .iter()
        .zip(&scene.relation_constraints)
        .filter(|(r, report)| !report.honored && !r.points.is_empty())
        .count();
    snapshot = snapshot
        .with_field("crystal-pins", scene.honored_holds.len().to_string())
        .with_field("crystal-borders", borders.to_string())
        .with_field(
            "crystal-unmet",
            (scene.relation_constraints.len() - borders).to_string(),
        )
        .with_field("crystal-strokes", strokes.to_string())
        .with_field(
            "crystal-drawn",
            scene
                .relations
                .iter()
                .filter(|r| !r.points.is_empty())
                .count()
                .to_string(),
        );
    for (occurrence, &instance) in &c.instance_by_occurrence {
        let at = scene.items[instance.0 as usize].transform.translate;
        snapshot = snapshot.with_field(
            format!("crystal-tile-{occurrence}"),
            format!("{:.2},{:.2}", at.x, at.y),
        );
    }
    snapshot
}
