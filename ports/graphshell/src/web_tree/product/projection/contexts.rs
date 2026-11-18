// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use cambium::{
    FocusEvent, FocusPhase, HoverEvent, HoverPhase, Key, KeyEvent, NamedKey, PointerButton,
    PointerEvent, PointerPhase, on_focus, on_hover, on_key, on_pointer, textarea_typed,
};
use scenograph::contexts::{
    ContextPlacement, ContextScene, ContextTrigger, SubjectKind, context_issues,
    contexts_from_toml, contexts_to_toml, hover_only_subjects,
};

mod inputs;
mod rules;
mod view;
use std::borrow::Cow;

pub(super) use view::{Anchor, panel, subjects};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Subject {
    kind: SubjectKind,
    id: String,
}

impl Subject {
    pub(super) fn node(id: String) -> Self {
        Self {
            kind: SubjectKind::Node,
            id,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Active {
    subject: Subject,
    trigger: ContextTrigger,
}

struct Press {
    subject: Subject,
    pointer: i32,
    since: f64,
    local: (f32, f32),
}

#[derive(Default)]
pub(super) struct State {
    active: Option<Active>,
    press: Option<Press>,
    text: TextInput,
}

impl State {
    pub(super) fn sync(&mut self, scenes: &[ContextScene]) {
        self.text = TextInput::new(contexts_to_toml(scenes).unwrap_or_default());
    }
}

fn definitions<'a>(
    page: &TreePage,
    p: &'a ProjectionControls,
    subject: &Subject,
) -> Cow<'a, [ContextScene]> {
    if let Some(override_scenes) = rules::override_scenes(page, p, subject) {
        return Cow::Owned(override_scenes);
    }
    Cow::Borrowed(compiled_definitions(p, subject))
}

fn compiled_definitions<'a>(p: &'a ProjectionControls, subject: &Subject) -> &'a [ContextScene] {
    let defaults = &p.editor.draft().marks.contexts;
    let Some(compiled) = &p.compiled else {
        return defaults;
    };
    match subject.kind {
        SubjectKind::Node | SubjectKind::Selection => compiled
            .occurrence_by_instance
            .iter()
            .find(|(_, id)| *id == &subject.id)
            .and_then(|(instance, _)| compiled.node_marks.get(instance)),
        SubjectKind::Relation => compiled.relation_marks.get(&subject.id),
        SubjectKind::Field => None,
    }
    .map_or(defaults, |resolved| &resolved.marks.contexts)
}

fn has(
    page: &TreePage,
    p: &ProjectionControls,
    subject: &Subject,
    trigger: ContextTrigger,
) -> bool {
    definitions(page, p, subject)
        .iter()
        .any(|scene| scene.trigger == trigger && scene.subject == subject.kind)
}

fn fire(page: &mut TreePage, subject: &Subject, trigger: ContextTrigger) -> bool {
    let Some(product) = &page.product else {
        return false;
    };
    let p = &product.projection;
    if !has(page, p, subject, trigger) {
        return false;
    }
    if trigger == ContextTrigger::Hover
        && p.contexts
            .active
            .as_ref()
            .is_some_and(|active| active.trigger != ContextTrigger::Hover)
    {
        return false;
    }
    page.product
        .as_mut()
        .expect("projection")
        .projection
        .contexts
        .active = Some(Active {
        subject: subject.clone(),
        trigger,
    });
    true
}

pub(super) fn select(page: &mut TreePage, subject: &Subject) {
    if !fire(page, subject, ContextTrigger::Selection) && subject.kind == SubjectKind::Node {
        let selection = Subject {
            kind: SubjectKind::Selection,
            id: subject.id.clone(),
        };
        fire(page, &selection, ContextTrigger::Selection);
    }
}

pub(super) fn focus_and_hover<V>(
    control: V,
    subject: Subject,
) -> impl cambium::ElementView<TreePage, ()>
where
    V: cambium::ElementView<TreePage, ()> + 'static,
{
    let hovered = subject.clone();
    on_focus(
        on_hover(
            control,
            move |page: &mut TreePage, event: HoverEvent| match event.phase {
                HoverPhase::Enter => {
                    fire(page, &hovered, ContextTrigger::Hover);
                },
                HoverPhase::Leave => {
                    if let Some(product) = &mut page.product {
                        let state = &mut product.projection.contexts;
                        if state.active.as_ref().is_some_and(|active| {
                            active.trigger == ContextTrigger::Hover && active.subject == hovered
                        }) {
                            state.active = None;
                        }
                    }
                },
                _ => {},
            },
        ),
        move |page: &mut TreePage, event: FocusEvent| {
            if event.phase == FocusPhase::Gained {
                fire(page, &subject, ContextTrigger::Focus);
            }
        },
    )
}

pub(super) fn interactive<V>(control: V, subject: Subject) -> Child
where
    V: cambium::ElementView<TreePage, ()> + 'static,
{
    let pointed = subject.clone();
    let keyed = subject.clone();
    Box::new(focus_and_hover(
        on_key(
            on_pointer(control, move |page: &mut TreePage, event: PointerEvent| {
                request_pointer(page, &pointed, &event);
            }),
            move |page: &mut TreePage, event: KeyEvent| {
                request_key(page, &keyed, &event);
            },
        ),
        subject,
    ))
}

pub(super) fn request_pointer(
    page: &mut TreePage,
    subject: &Subject,
    event: &PointerEvent,
) -> bool {
    let Some(product) = &page.product else {
        return false;
    };
    if !has(page, &product.projection, subject, ContextTrigger::Request) {
        return false;
    }
    let now = js_sys::Date::now();
    let state = &mut page
        .product
        .as_mut()
        .expect("projection")
        .projection
        .contexts;
    let requested = match (event.phase, event.button) {
        (PointerPhase::Down, PointerButton::Secondary) => true,
        (PointerPhase::Down, PointerButton::Primary) => {
            state.press = Some(Press {
                subject: subject.clone(),
                pointer: event.pointer_id,
                since: now,
                local: event.local,
            });
            false
        },
        (PointerPhase::Move | PointerPhase::Cancel, PointerButton::Primary) => {
            if state.press.as_ref().is_some_and(|press| {
                press.pointer == event.pointer_id
                    && (event.phase == PointerPhase::Cancel
                        || (press.local.0 - event.local.0).hypot(press.local.1 - event.local.1)
                            > 8.0)
            }) {
                state.press = None;
            }
            false
        },
        (PointerPhase::Up, PointerButton::Primary) => state.press.take().is_some_and(|press| {
            press.subject == *subject
                && press.pointer == event.pointer_id
                && now - press.since >= 500.0
                && (press.local.0 - event.local.0).hypot(press.local.1 - event.local.1) <= 8.0
        }),
        _ => false,
    };
    if requested && fire(page, subject, ContextTrigger::Request) {
        event.prop.prevent_default();
        event.prop.stop_propagation();
        return true;
    }
    false
}

pub(super) fn request_key(page: &mut TreePage, subject: &Subject, event: &KeyEvent) -> bool {
    if (matches!(event.key, Key::Named(NamedKey::ContextMenu))
        || matches!(event.key, Key::Named(NamedKey::F10)) && event.mods.shift)
        && fire(page, subject, ContextTrigger::Request)
    {
        event.prop.prevent_default();
        event.prop.stop_propagation();
        return true;
    }
    view::keyboard(page, event)
}

pub(super) fn author(p: &ProjectionControls) -> Child {
    let warnings = hover_only_subjects(&p.editor.draft().marks.contexts);
    Box::new(el(
        "section",
        (
            el(
                "label",
                (
                    "Context recipes (TOML)",
                    lens(
                        |input: &mut TextInput| {
                            textarea_typed(input).attr("aria-label", "Context recipes")
                        },
                        |page: &mut TreePage| {
                            &mut page
                                .product
                                .as_mut()
                                .expect("projection")
                                .projection
                                .contexts
                                .text
                        },
                    ),
                ),
            ),
            button("Apply contexts", |page: &mut TreePage, _| {
                let Some(product) = &mut page.product else {
                    return;
                };
                let p = &mut product.projection;
                match contexts_from_toml(p.contexts.text.text()) {
                    Ok(scenes) if context_issues(&scenes).is_empty() => {
                        p.editor.break_run();
                        p.editor.reduce(
                            EditorAction::SetContexts(scenes),
                            js_sys::Date::now() as u64,
                        );
                        p.editor.break_run();
                        p.contexts.active = None;
                        p.status = "Applied context recipes".into();
                        p.refresh();
                    },
                    Ok(_) => {
                        p.status = "Use unique context ids, input bindings, and command ids".into()
                    },
                    Err(error) => p.status = format!("Context recipes: {error}"),
                }
            }),
            (!warnings.is_empty()).then(|| {
                el(
                    "p",
                    format!("Hover-only contexts have no touch trigger for {warnings:?}."),
                )
                .attr("role", "status")
                .attr("aria-label", "Context warnings")
            }),
        ),
    ))
}

pub(super) fn enabled(page: &TreePage, p: &ProjectionControls, subject: &Subject) -> bool {
    definitions(page, p, subject).iter().any(|scene| {
        scene.subject == subject.kind
            || subject.kind == SubjectKind::Node && scene.subject == SubjectKind::Selection
    })
}
