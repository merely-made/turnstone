// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use scenomise::projection::{ProjectionOccurrence, ProjectionValue};

pub(in crate::web_tree::product::projection) fn subjects(
    page: &TreePage,
    p: &ProjectionControls,
) -> Option<Child> {
    let data = p.dataset.as_ref()?;
    let fields = data.fields.keys().map(|id| (SubjectKind::Field, id, id));
    let relations = p
        .relationships
        .iter()
        .map(|edge| (SubjectKind::Relation, &edge.id, &edge.label));
    let controls: Vec<_> = fields
        .chain(relations)
        .filter_map(|(kind, id, label)| {
            let subject = Subject {
                kind,
                id: id.clone(),
            };
            if !enabled(page, p, &subject) {
                return None;
            }
            let clicked = subject.clone();
            let anchor = Anchor::Control(&subject);
            let placement = panel(page, p, Some(anchor));
            let control = interactive(
                button(label.clone(), move |page: &mut TreePage, _| {
                    select(page, &clicked);
                })
                .attr("data-context-subject", id.clone()),
                subject,
            );
            Some(Box::new(
                el("div", (control, placement))
                    .attr("style", "display:inline-block;position:relative;"),
            ) as Child)
        })
        .collect();
    (!controls.is_empty())
        .then(|| Box::new(el("div", controls).attr("aria-label", "Context subjects")) as Child)
}

fn matches_active(scene: &ContextScene, active: &Active) -> bool {
    scene.trigger == active.trigger && scene.subject == active.subject.kind
}

pub(in crate::web_tree::product::projection) enum Anchor<'a> {
    Tile(&'a str, [f32; 4]),
    Control(&'a Subject),
}

impl Anchor<'_> {
    fn matches(&self, subject: &Subject) -> bool {
        match self {
            Self::Tile(id, _) => {
                subject.id == *id
                    && matches!(subject.kind, SubjectKind::Node | SubjectKind::Selection)
            },
            Self::Control(control) => *control == subject,
        }
    }
}

pub(in crate::web_tree::product::projection) fn panel(
    page: &TreePage,
    p: &ProjectionControls,
    anchor: Option<Anchor<'_>>,
) -> Vec<Child> {
    let Some(active) = &p.contexts.active else {
        return Vec::new();
    };
    if anchor
        .as_ref()
        .is_some_and(|anchor| !anchor.matches(&active.subject))
    {
        return Vec::new();
    }
    definitions(page, p, &active.subject)
        .iter()
        .filter(|ctx| matches_active(ctx, active))
        .filter_map(|ctx| {
            let placement = match (ctx.placement, &anchor) {
                (ContextPlacement::Pane, None) => String::new(),
                (ContextPlacement::Pane, Some(_)) | (_, None) => return None,
                (ContextPlacement::Beside, Some(Anchor::Tile(_, [x, y, w, _]))) => {
                    format!("position:absolute;left:{}px;top:{y}px;", x + w + 8.0)
                },
                (ContextPlacement::Beside, Some(Anchor::Control(_))) => {
                    "position:absolute;left:100%;top:0;".into()
                },
                (ContextPlacement::Overlay, Some(_)) => {
                    "position:absolute;right:4px;top:4px;".into()
                },
                (ContextPlacement::InPlace, Some(Anchor::Tile(_, [x, y, w, h]))) => format!(
                    "position:absolute;left:{x}px;top:{y}px;min-width:{w}px;min-height:{h}px;"
                ),
                (ContextPlacement::InPlace, Some(Anchor::Control(_))) => {
                    "position:absolute;left:0;top:0;min-width:100%;min-height:100%;".into()
                },
            };
            let (rows, mut contents) = match inputs::read(page, p, ctx, &active.subject) {
                Some(data) => match scenomise::contexts::compile(
                    graphshell::projection_compile::practice_compiler(),
                    ctx,
                    &data,
                ) {
                    Ok(compiled) => (
                        compiled.scene.items.len(),
                        child_scene(active, ctx, &data, &compiled),
                    ),
                    Err(error) => (
                        0,
                        Box::new(el("p", format!("Context could not compile: {error:?}"))) as Child,
                    ),
                },
                None => (
                    0,
                    Box::new(el("p", "This subject has no matching input")) as Child,
                ),
            };
            if rows == 0 {
                contents = Box::new(el("div", (contents, el("p", "No context items"))));
            }
            Some(Box::new(on_key(
                el(
                    "section",
                    (
                        el("h3", ctx.recipe.appearance.title.clone()),
                        button("Close context", |page: &mut TreePage, _| {
                            if let Some(product) = &mut page.product {
                                product.projection.contexts.active = None;
                            }
                        }),
                        contents,
                    ),
                )
                .attr("class", "tree-command-menu")
                .attr("role", "region")
                .attr(
                    "style",
                    format!("{placement}max-width:min(100%,420px);min-width:190px;"),
                )
                .attr("data-context-scene", ctx.id.clone())
                .attr(
                    "data-context-placement",
                    format!("{:?}", ctx.placement).to_lowercase(),
                )
                .attr(
                    "data-context-trigger",
                    format!("{:?}", ctx.trigger).to_lowercase(),
                )
                .attr("aria-label", ctx.recipe.appearance.title.clone()),
                |page: &mut TreePage, event: KeyEvent| {
                    keyboard(page, &event);
                },
            )) as Child)
        })
        .collect()
}

fn child_scene(
    active: &Active,
    ctx: &ContextScene,
    data: &ProjectionDataset,
    compiled: &CompiledProjection,
) -> Child {
    let scene = &compiled.scene;
    let scale = (360.0 / scene.bounds.size.w.max(1.0)).min(1.0);
    let items: Vec<Child> = scene.items.iter().enumerate().filter(|(_, item)| item.visible)
        .filter_map(|(index, item)| {
            let instance = InstanceId(index as u32);
            let id = compiled.occurrence_by_instance.get(&instance)?;
            let label = compiled.labels.get(&instance).unwrap_or(id);
            let bounds = item.footprint.bounds()?;
            let world = scene.to_world(item.space)?.then(&item.transform);
            let origin = world.apply(bounds.origin);
            let x = (origin.x - scene.bounds.origin.x) * scale;
            let y = (origin.y - scene.bounds.origin.y) * scale;
            let w = (bounds.size.w * world.scale * scale).max(1.0);
            let h = (bounds.size.h * world.scale * scale).max(1.0);
            let style = format!(
                "position:absolute;left:{x}px;top:{y}px;width:{w}px;height:{h}px;overflow:hidden;overflow-wrap:anywhere;text-align:start;"
            );
            if ctx.input == "commands"
                && let Some(row) = data.occurrences.iter().find(|row| &row.occurrence_id == id)
            {
                return Some(command(active, ctx, row, label, style));
            }
            Some(Box::new(el("div", label.clone()).attr("style", style).attr("role", "listitem")
                .attr("data-context-item", id.clone())) as Child)
        }).collect();
    let height = (scene.bounds.size.h * scale).max(24.0);
    Box::new(el("div", items).attr(
        "style",
        format!(
            "position:relative;height:{height}px;width:{}px;max-width:100%;",
            scene.bounds.size.w * scale,
        ),
    ))
}

fn command(
    active: &Active,
    ctx: &ContextScene,
    row: &ProjectionOccurrence,
    label: &str,
    style: String,
) -> Child {
    let get = |field| {
        row.values
            .get(field)
            .and_then(ProjectionValue::text)
            .unwrap_or("")
    };
    let shortcut = get("shortcut");
    let disabled = get("disabled_reason");
    let enabled = disabled.is_empty();
    let active = active.clone();
    let context = ctx.id.clone();
    let id = row.occurrence_id.clone();
    let caption = if shortcut.is_empty() {
        label.to_owned()
    } else {
        format!("{label} · {shortcut}")
    };
    Box::new(
        button(caption, {
            let id = id.clone();
            move |page: &mut TreePage, _| {
                if enabled {
                    inputs::invoke(page, &active, &context, &id);
                }
            }
        })
        .attr("aria-label", label.to_owned())
        .attr("aria-keyshortcuts", shortcut.to_owned())
        .attr("aria-disabled", (!enabled).to_string())
        .attr("title", disabled.to_owned())
        .attr("data-context-item", id)
        .attr("style", style),
    )
}

fn shortcut_matches(chord: &str, event: &KeyEvent) -> bool {
    let mut parts = chord.split('+').map(str::trim);
    let Some(last) = parts.next_back() else {
        return false;
    };
    let key_matches = match &event.key {
        Key::Character(c) => last.eq_ignore_ascii_case(c),
        Key::Named(key) => last.eq_ignore_ascii_case(&format!("{key:?}")),
        _ => false,
    };
    if !key_matches || last.is_empty() {
        return false;
    }
    let mut modifiers = cambium::Modifiers::default();
    for part in parts {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.ctrl = true,
            "alt" => modifiers.alt = true,
            "shift" => modifiers.shift = true,
            "meta" => modifiers.meta = true,
            _ => return false,
        }
    }
    modifiers == event.mods
}

pub(super) fn keyboard(page: &mut TreePage, event: &KeyEvent) -> bool {
    let Some(p) = page.product.as_ref().map(|product| &product.projection) else {
        return false;
    };
    let Some(active) = p.contexts.active.clone() else {
        return false;
    };
    if matches!(event.key, Key::Named(NamedKey::Escape)) {
        page.product
            .as_mut()
            .expect("active projection")
            .projection
            .contexts
            .active = None;
    } else {
        let Some((context, id)) = definitions(page, p, &active.subject)
            .iter()
            .filter(|ctx| matches_active(ctx, &active) && ctx.input == "commands")
            .find_map(|ctx| {
                let data = inputs::read(page, p, ctx, &active.subject)?;
                let compiled = scenomise::contexts::compile(
                    graphshell::projection_compile::practice_compiler(),
                    ctx,
                    &data,
                )
                .ok()?;
                let row = data.occurrences.iter().find(|r| {
                    compiled
                        .occurrence_by_instance
                        .values()
                        .any(|id| id == &r.occurrence_id)
                        && r.values.get("enabled") == Some(&ProjectionValue::Boolean(true))
                        && r.values
                            .get("shortcut")
                            .and_then(ProjectionValue::text)
                            .is_some_and(|shortcut| shortcut_matches(shortcut, event))
                })?;
                Some((ctx.id.clone(), row.occurrence_id.clone()))
            })
        else {
            return false;
        };
        inputs::invoke(page, &active, &context, &id);
    }
    event.prop.prevent_default();
    event.prop.stop_propagation();
    true
}
