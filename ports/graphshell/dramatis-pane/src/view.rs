// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Retained, accessible controls for the pane's cards. Every control is an
//! action the surface advertised, and every press becomes an intent.
use cambium::{AnyView, GenetCtx, GenetElement, button, el, text};
use cambium_genet_winit_host::{HostHooks, ScrollAlign};
use cambium_rootstock::meristem_bounds::RootView;
use dramatis_surface::Custody;

use crate::pane::{Pane, PaneCard};

pub type Child<S> = Box<dyn AnyView<S, (), GenetCtx, GenetElement>>;
/// How the pane is reached inside a host's state.
pub type Access<S, C> = fn(&mut S) -> &mut Pane<C>;

fn line<S: 'static>(s: impl Into<String>) -> Child<S> {
    Box::new(el("p", text(s.into())))
}

fn disabled<S: 'static>(label: &str) -> Child<S> {
    Box::new(
        el("button", text(label.to_string()))
            .attr("disabled", "true")
            .attr("aria-disabled", "true")
            .attr("tabindex", "-1"),
    )
}

fn control<S: 'static, C: Custody + 'static>(
    card: &PaneCard,
    intent: &str,
    label: &str,
    explanation: &str,
    busy: bool,
    access: Access<S, C>,
) -> Child<S> {
    if busy {
        return disabled(label);
    }
    let (instance, intent) = (card.instance, intent.to_string());
    Box::new(
        button(label.to_string(), move |state: &mut S, _| {
            access(state).activate(instance, &intent);
        })
        .attr("title", explanation.to_string())
        .attr("data-dramatis-action", intent_attr(card, label)),
    )
}

fn intent_attr(card: &PaneCard, label: &str) -> String {
    card.actions
        .iter()
        .find(|a| a.label == label)
        .map(|a| a.intent.0.clone())
        .unwrap_or_default()
}

fn form<S: 'static, C: Custody + 'static>(
    pane: &Pane<C>,
    access: Access<S, C>,
) -> Option<Child<S>> {
    let draft = pane.form.draft.as_ref()?;
    let semantics = draft.semantics();
    let mut children: Vec<Child<S>> = Vec::new();
    for field in semantics.fields {
        let name = field.name;
        let choices: Vec<Child<S>> = field
            .choices
            .into_iter()
            .map(|choice| {
                let (name, value) = (name.clone(), choice.value);
                Box::new(
                    button(choice.label, move |state: &mut S, _| {
                        access(state).choose(&name, &value);
                    })
                    .attr("aria-pressed", choice.selected.to_string()),
                ) as Child<S>
            })
            .collect();
        children.push(Box::new(el(
            "fieldset",
            (el("legend", field.label), choices),
        )));
    }
    children.push(Box::new(
        button(semantics.submit_label, move |state: &mut S, _| {
            access(state).submit();
        })
        .attr("data-dramatis-submit", "true"),
    ));
    children.push(Box::new(button("Cancel", move |state: &mut S, _| {
        access(state).close_form();
    })));
    children.push(Box::new(
        el("p", pane.form.status.clone())
            .attr("role", "status")
            .attr("class", "dramatis-form-status"),
    ));
    Some(Box::new(
        el("div", children)
            .attr("role", "group")
            .attr("aria-label", semantics.label),
    ))
}

fn card_view<S: 'static, C: Custody + 'static>(
    pane: &Pane<C>,
    card: &PaneCard,
    access: Access<S, C>,
) -> Child<S> {
    let busy = pane.busy();
    let status = card.card.badges.iter().any(|b| b == "status");
    let mut contents: Vec<Child<S>> = vec![Box::new(el("h3", text(card.card.title.clone())))];
    for value in &card.card.values {
        // The status card's notice is the live region: it exists for every
        // reading, empty or not, so announcements do not move the controls.
        if status && value.label == "Notice" {
            contents.push(Box::new(
                el("p", text(value.value.clone()))
                    .attr("role", "status")
                    .attr("class", "dramatis-status"),
            ));
        } else {
            contents.push(line(format!("{}: {}", value.label, value.value)));
        }
    }
    let badges: Vec<&str> = card
        .card
        .badges
        .iter()
        .map(String::as_str)
        .filter(|b| *b != "status" && *b != "busy")
        .collect();
    if !badges.is_empty() {
        contents.push(line(badges.join(" · ")));
    }
    for action in &card.actions {
        contents.push(control(
            card,
            &action.intent.0,
            &action.label,
            &action.explanation,
            busy,
            access,
        ));
    }
    if pane.form_card == Some(card.instance) {
        if let Some(open) = form(pane, access) {
            contents.push(open);
        }
    }
    Box::new(el("section", contents).attr("class", "card"))
}

/// The pane as a section of the host's page. `access` names where the pane
/// lives in the host's state, so a press can reach it.
pub fn view<S: 'static, C: Custody + 'static>(pane: &Pane<C>, access: Access<S, C>) -> Child<S> {
    let mut body: Vec<Child<S>> = vec![
        Box::new(el("h2", text("Dramatis")).attr("class", "dramatis-heading")),
        line("Your personae and imported credentials"),
    ];
    for card in pane.cards() {
        body.push(card_view(pane, card, access));
    }
    if !pane.message.is_empty() {
        body.push(Box::new(
            el("p", text(pane.message.clone()))
                .attr("role", "alert")
                .attr("class", "dramatis-refusal"),
        ));
    }
    Box::new(
        el("section", el("div", body).attr("class", "contents"))
            .attr("class", "dramatis-pane")
            .attr("role", "region")
            .attr("aria-label", "Dramatis"),
    )
}

/// Reveal a newly focused control without undoing deliberate wheel scrolling.
pub fn follow_focus_hooks<State: 'static, Logic, V>() -> HostHooks<State, Logic, V>
where
    Logic: FnMut(&State) -> V + 'static,
    V: RootView<State>,
{
    let mut hooks: HostHooks<State, Logic, V> = HostHooks::inert();
    let mut followed = None;
    hooks.after_dispatch = Box::new(move |ctx| {
        let focus = ctx.runner.focus();
        if focus != followed {
            followed = focus;
            if let Some(node) = focus {
                ctx.scroll_into_view(node, ScrollAlign::Nearest);
            }
        }
    });
    hooks
}

/// The pane's stylesheet. Hosts append it to their own.
pub const SHEET: &str = r#"
.dramatis-pane { padding: 16px; }
.dramatis-pane .contents { max-width: 820px; margin: 0 auto; }
.dramatis-pane h2 { font-size: 20px; margin: 8px 0 12px; }
.dramatis-pane h3 { font-size: 17px; margin: 0 0 8px; }
.dramatis-pane p { line-height: 1.5; margin: 8px 0; overflow-wrap: anywhere; }
.dramatis-pane button { margin: 6px 10px 6px 0; padding: 10px 16px; border: 1px solid #76766b; border-radius: 6px; }
.dramatis-pane button:focus-visible { outline: 3px solid #315e75; outline-offset: 2px; }
.dramatis-pane button[aria-disabled="true"] { opacity: 0.6; }
.dramatis-pane button[aria-pressed="true"] { border-color: #8cb4de; }
.dramatis-pane .card { border: 1px solid #c6c6b6; border-radius: 8px; padding: 16px; margin: 12px 0; }
.dramatis-pane .dramatis-refusal { color: #f0b35a; }
"#;
