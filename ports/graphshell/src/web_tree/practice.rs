// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Retained realization of the existing practice state and its control model.
use super::*;
use cambium::{PointerButton, PointerEvent, button_with, on_pointer};
pub(super) type Practice = crate::web_practice::PracticeHost;

pub(super) fn open(root: &Element) -> Result<Option<Practice>, String> {
    if !root.has_attribute("data-practice-workspace") {
        return Ok(None);
    }
    let mut practice = Practice::new(crate::web_dataset::supplied(root))?;
    if root.get_attribute("data-reduced-motion").as_deref() == Some("true") {
        practice.command("motion")?;
    }
    Ok(Some(practice))
}
pub(super) fn advance(page: &mut TreePage, now_ms: f64) -> bool {
    page.practice
        .as_mut()
        .is_some_and(|practice| practice.advance(page.size.0, page.size.1, now_ms))
}
pub(super) fn needs_frame(page: &TreePage) -> bool {
    page.practice.as_ref().is_some_and(Practice::needs_frame)
}
pub(super) fn fields(page: &TreePage, mut snapshot: ProbeSnapshot) -> ProbeSnapshot {
    if let Some(practice) = &page.practice {
        if let Ok(fields) = practice.fields() {
            for (key, value) in fields {
                snapshot = snapshot.with_field(key.strip_prefix("data-").unwrap_or(key), value);
            }
        }
    }
    snapshot
}
pub(super) fn view(page: &TreePage) -> Option<Child> {
    let practice = page.practice.as_ref()?;
    let controls = practice.controls(page.size.0, page.size.1);
    let mut children: Vec<Child> = Vec::new();
    for ([x, y, tx, ty], curved) in practice.relation_lines() {
        let color = if curved { "#99b391" } else { "#dbb86b" };
        let steps = if curved { 16 } else { 1 };
        let point = |t: f32| {
            if !curved {
                return (x + (tx - x) * t, y + (ty - y) * t);
            }
            let u = 1.0 - t;
            (
                u * u * u * x + 3.0 * u * u * t * x + 3.0 * u * t * t * tx + t * t * t * tx,
                u * u * u * y
                    + 3.0 * u * u * t * (y + ty) * 0.5
                    + 3.0 * u * t * t * (y + ty) * 0.5
                    + t * t * t * ty,
            )
        };
        for step in 0..steps {
            let (a, b) = point(step as f32 / steps as f32);
            let (c, d) = point((step + 1) as f32 / steps as f32);
            children.push(Box::new(el("div", "").attr("aria-hidden", "true").attr("style", format!(
                "position:absolute;left:{a}px;top:{b}px;width:{}px;height:2px;background:{color};transform:rotate({}rad);transform-origin:0 0;pointer-events:none;",
                (c-a).hypot(d-b), (d-b).atan2(c-a)))));
        }
    }
    for control in controls {
        let [x, y, width, height] = control.rect;
        let style = format!(
            "position:absolute;left:{x}px;top:{y}px;width:{width}px;height:{height}px;overflow:hidden;"
        );
        let label = format!("{} {}", control.face.label, control.face.detail)
            .trim()
            .to_owned();
        let face = (
            el("span", control.face.label.clone()).attr("class", "practice-label"),
            el("span", control.face.detail.clone()).attr("class", "practice-detail"),
        );
        let class = format!(
            "practice-control practice-{} {}",
            control.face.kind,
            if control.face.selected {
                "practice-selected"
            } else {
                ""
            }
        );
        if let Some(command) = control.command {
            let action = command.clone();
            let target = button_with(face, move |page: &mut TreePage, _| {
                if let Some(practice) = &mut page.practice {
                    if let Err(error) = practice.command(&action) {
                        practice.set_error(error);
                    }
                }
            })
            .attr("aria-label", label)
            .attr("aria-pressed", control.face.selected.to_string())
            .attr("data-practice-command", command)
            .attr("data-practice-id", control.id)
            .attr("style", style)
            .attr("class", class);
            if let Some(body) = control.body {
                children.push(Box::new(on_pointer(
                    target.attr("data-practice-body", body.clone()),
                    move |page: &mut TreePage, event: PointerEvent| {
                        if event.button != PointerButton::Primary {
                            return;
                        }
                        if let Some(practice) = &mut page.practice {
                            practice.board_pointer(
                                &body,
                                event.phase,
                                event.local.0 + x,
                                event.local.1 + y,
                            );
                        }
                    },
                )));
            } else {
                children.push(Box::new(target));
            }
        } else {
            children.push(Box::new(
                el("div", face)
                    .attr("aria-label", label)
                    .attr(
                        "role",
                        if control.face.kind == "heading" {
                            "heading"
                        } else {
                            "group"
                        },
                    )
                    .attr("data-practice-id", control.id)
                    .attr("style", style)
                    .attr("class", class),
            ));
        }
    }
    Some(Box::new(
        el("main", children)
            .attr("aria-label", "Thursday practice workspace")
            .attr("class", "tree-practice")
            .attr(
                "style",
                format!(
                    "position:relative;width:{}px;height:{}px;",
                    page.size.0, page.size.1
                ),
            ),
    ))
}
