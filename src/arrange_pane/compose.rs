// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The Arrange pane's composing controls: role, role groups, two-law mix,
//! grouping, two-stage schedule and speed, drawn from
//! `mere::canvas::physics_form`, the model the web host's tools region draws
//! too. The model owns the fields, their choices, defaults, read-back and
//! validation; this file turns each field into a `cambium::setting_row` and
//! each button press into an [`ArrangeIntent`]. Role, role groups and speed
//! apply when picked. The mix, grouping and schedule fields land in the
//! model's form until their button applies them, and follow the canvas when
//! what it authored changes, not on every frame.

use mere::canvas::physics_form::{Command, Control, Field, FieldId, FormAction, Readback, Section};
use mere_surface_api::settings::{SettingControl, SettingValue};

use super::{
    ArrangeIntent, ArrangeState, ArrangeView, apply_button, choice_control, el, section,
    setting_row, spec,
};

/// The note under a form: what the canvas authored for it, or why the form
/// cannot show it.
fn status(readback: &Readback, section: Section) -> ArrangeView {
    Box::new(
        el::<_, ArrangeState, ArrangeIntent>("div", readback.summary(section))
            .attr("role", "status")
            .attr("aria-label", section.status_label()),
    )
}

/// The authored forms as plain rows, for the observe snapshot.
pub(super) fn authored_rows(readback: &Readback) -> [String; 3] {
    [
        format!("Mix: {}", readback.summary(Section::Mix)),
        format!("Grouping: {}", readback.summary(Section::Grouping)),
        format!("Schedule: {}", readback.summary(Section::Schedule)),
    ]
}

fn options_of(options: &[(&'static str, &'static str)]) -> Vec<(String, String)> {
    options
        .iter()
        .map(|(id, label)| ((*id).to_string(), (*label).to_string()))
        .collect()
}

/// A field whose pick applies at once, as `intent` lowers it.
pub(super) fn live_choice(field: Field, intent: fn(String) -> ArrangeIntent) -> ArrangeView {
    let key = field.id.key();
    live_choice_with_key(field, &key, intent)
}

pub(super) fn live_choice_with_key(
    field: Field,
    key: &str,
    intent: fn(String) -> ArrangeIntent,
) -> ArrangeView {
    let Control::Choice { options, current } = field.control else {
        unreachable!("{} is a choice", field.label)
    };
    let spec = spec(
        key,
        &field.label,
        choice_control(options_of(&options)),
        SettingValue::Text(current.to_string()),
    );
    Box::new(setting_row(
        &spec,
        &field.label,
        move |_state: &mut ArrangeState, value: SettingValue| match value {
            SettingValue::Text(id) => Some(intent(id)),
            _ => None,
        },
    ))
}

/// A toggle field whose accepted change applies immediately.
pub(super) fn live_toggle(
    field: Field,
    intent: impl Fn(bool) -> ArrangeIntent + 'static,
) -> ArrangeView {
    let key = field.id.key();
    live_toggle_with_key(field, &key, intent)
}

pub(super) fn live_toggle_with_key(
    field: Field,
    key: &str,
    intent: impl Fn(bool) -> ArrangeIntent + 'static,
) -> ArrangeView {
    let Control::Toggle(current) = field.control else {
        unreachable!("{} is a toggle", field.label)
    };
    let spec = spec(
        key,
        &field.label,
        SettingControl::Toggle,
        SettingValue::Boolean(current),
    );
    Box::new(setting_row(
        &spec,
        &field.label,
        move |_state: &mut ArrangeState, value: SettingValue| match value {
            SettingValue::Boolean(on) => Some(intent(on)),
            _ => None,
        },
    ))
}

/// A field whose edit lands in the form instead of applying. Shares and
/// frames are typed text, as the model keeps them; it validates on apply.
fn draft_field(field: Field) -> ArrangeView {
    let id = field.id;
    let typed = matches!(field.control, Control::Number { .. });
    let (control, value) = match field.control {
        Control::Choice { options, current } => (
            choice_control(options_of(&options)),
            SettingValue::Text(current.to_string()),
        ),
        Control::Number { text, .. } => (SettingControl::Text, SettingValue::Text(text)),
        Control::Toggle(on) => (SettingControl::Toggle, SettingValue::Boolean(on)),
    };
    let spec = spec(&id.key(), &field.label, control, value);
    Box::new(setting_row(
        &spec,
        &field.label,
        move |state: &mut ArrangeState, value: SettingValue| {
            match value {
                SettingValue::Text(text) if typed => {
                    state.form.enter(id, &text);
                },
                SettingValue::Text(option) => {
                    state.form.choose(id, &option);
                },
                SettingValue::Boolean(on) => {
                    state.form.tick(id, on);
                },
                _ => {},
            }
            None::<ArrangeIntent>
        },
    ))
}

/// A button whose press asks the model for the request, or says why not.
fn command_button(command: Command) -> ArrangeView {
    apply_button(command.label(), move |state| {
        let intent = state.form.action(command).map(|action| match action {
            FormAction::Mix(mix) => ArrangeIntent::ApplyMix(mix),
            FormAction::Grouping(grouping) => ArrangeIntent::ApplyGrouping(grouping),
            FormAction::Schedule(schedule) => ArrangeIntent::ApplySchedule(schedule),
            FormAction::SingleLaw(_) => ArrangeIntent::UseSingleLaw,
            FormAction::Role(_) | FormAction::RoleGroups(_) | FormAction::Speed(_) => {
                unreachable!("these form actions apply when picked")
            },
        });
        state.form_note = intent.as_ref().err().cloned().unwrap_or_default();
        intent.ok()
    })
}

/// The composing sections, after Sources.
pub(super) fn views(state: &ArrangeState) -> Vec<ArrangeView> {
    let form = &state.form;
    let mut views: Vec<ArrangeView> = vec![
        section("Roles"),
        live_choice(form.field(FieldId::Role), ArrangeIntent::SetRole),
        live_choice(
            form.field(FieldId::RoleGroups),
            ArrangeIntent::SetRoleGroups,
        ),
    ];
    for part in Section::ALL {
        views.push(section(part.title()));
        views.push(status(&state.choice.readback, part));
        for group in part.groups() {
            views.extend(group.title.map(section));
            views.extend(
                form.fields(&group)
                    .filter(|field| field.relevant)
                    .map(draft_field),
            );
        }
        views.extend(part.commands().iter().copied().map(command_button));
    }
    views.push(section("Speed"));
    views.push(live_choice(
        form.field(FieldId::Speed),
        ArrangeIntent::SetSpeed,
    ));
    if !state.form_note.is_empty() {
        views.push(Box::new(
            el::<_, ArrangeState, ArrangeIntent>("div", state.form_note.clone())
                .attr("role", "status")
                .attr("aria-live", "polite")
                .attr("aria-label", "Form status"),
        ));
    }
    views
}
