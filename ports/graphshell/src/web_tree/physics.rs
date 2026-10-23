// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The "Graph tools" side region's first section: arrangement and physics.
//!
//! Cambium controls hold the form; a button reads them into
//! `mere::canvas::physics_form`, the model Turnstone's Arrange pane drives
//! too. The model owns the composing fields (role, role groups, two-law mix,
//! grouping, two-stage schedule, speed): their choices, defaults, read-back,
//! validation and refusals, and the edit each Apply makes. This file draws
//! them and carries the scenario hooks.
use super::*;
use cambium::{
    SelectState, TextInput, button, checkbox, checkbox_disabled, disclosure, lens, select,
    text_field_typed,
};
use mere::canvas::physics_controls::{
    self, ArrangementTransition, CUSTOM_PROFILE, LawStart, arrangement_choices,
};
use mere::canvas::physics_form::{
    Command, Control, Field, FieldId, FormAction, Group, PhysicsForm, Readback, STOP_FRAMES,
    STOP_LAW, Section, Slot,
};
use mere::canvas::{
    CANVAS_PHYSICS_DEPTH_SOURCES, CANVAS_PHYSICS_KIND_SOURCES, CANVAS_PHYSICS_LAWS,
    CANVAS_PHYSICS_MASS_SOURCES, CANVAS_PHYSICS_OVERLAYS, CANVAS_PHYSICS_PROFILES, PhysicsChoice,
    PhysicsDepthSource, PhysicsKindSource, PhysicsLaw, PhysicsMassSource, PhysicsOverlay,
};
use std::collections::HashMap;

/// The profile picker's first entry, read when the live pair names no profile.
const CUSTOM_LABEL: &str = "Custom (no profile)";

/// The panel's control state and the arrangement it last applied.
pub(super) struct PhysicsPanel {
    pub(super) arrangement: SelectState,
    pub(super) law: SelectState,
    pub(super) kind: SelectState,
    pub(super) mass: SelectState,
    pub(super) depth: SelectState,
    /// Group pull's channel (any kind source; F37).
    pub(super) groups: SelectState,
    /// Index 0 is custom; profile `i` is at `i + 1`.
    pub(super) profile: SelectState,
    /// One per `CANVAS_PHYSICS_OVERLAYS` entry, in catalog order.
    pub(super) overlays: Vec<bool>,
    pub(super) layout_id: String,
    pub(super) status: String,
    pub(super) transition: Option<ArrangementTransition>,
    /// The layout where the last law was applied.
    pub(super) law_start: Option<LawStart>,
    /// The composing form, as of the last time the controls were read into it.
    form: PhysicsForm,
    /// What the canvas authored when the controls last followed it.
    readback: Readback,
    /// The controls that edit the form, one per field: a choice's select, a
    /// number's text, a toggle's box. Text keeps its authored digits.
    selects: HashMap<FieldId, SelectState>,
    inputs: HashMap<FieldId, TextInput>,
    ticks: HashMap<FieldId, bool>,
    /// The speed preset the canvas runs, and the note on the speed reached
    /// while the budget binds. The picked preset is the speed select's.
    pub(super) applied_speed: usize,
    pub(super) speed_note: Option<String>,
}

fn index_of<T: PartialEq>(items: impl IntoIterator<Item = T>, item: T) -> usize {
    items.into_iter().position(|i| i == item).unwrap_or(0)
}

fn labels(catalog: &[(&'static str, &'static str)]) -> Vec<&'static str> {
    catalog.iter().map(|(_, label)| *label).collect()
}

/// Every field of the composing form, in the order it is drawn.
fn composing_fields() -> Vec<FieldId> {
    [FieldId::Role, FieldId::RoleGroups, FieldId::Speed]
        .into_iter()
        .chain(
            Section::ALL
                .into_iter()
                .flat_map(Section::groups)
                .flat_map(|group| group.fields),
        )
        .collect()
}

impl PhysicsPanel {
    pub(super) fn new(canvas: &Canvas, layout_id: &str) -> Self {
        let form = PhysicsForm::new(canvas);
        let (mut selects, mut inputs, mut ticks) = (HashMap::new(), HashMap::new(), HashMap::new());
        for id in composing_fields() {
            let field = form.field(id);
            match field.control {
                Control::Choice { .. } => {
                    selects.insert(id, SelectState::new(0).with_label(field.label));
                },
                Control::Number { text, .. } => {
                    inputs.insert(id, TextInput::new(text));
                },
                Control::Toggle(on) => {
                    ticks.insert(id, on);
                },
            }
        }
        let mut panel = Self {
            arrangement: SelectState::new(index_of(
                arrangement_choices().map(|(id, _)| id),
                layout_id,
            ))
            .with_label("Arrangement"),
            law: SelectState::new(0).with_label("Physics law"),
            kind: SelectState::new(0).with_label("Kinds"),
            mass: SelectState::new(0).with_label("Mass"),
            depth: SelectState::new(0).with_label("Depth"),
            groups: SelectState::new(0).with_label("Groups"),
            profile: SelectState::new(0).with_label("Profile"),
            overlays: vec![false; CANVAS_PHYSICS_OVERLAYS.len()],
            layout_id: layout_id.to_string(),
            status: String::new(),
            transition: None,
            law_start: None,
            form,
            readback: Readback::of(canvas),
            selects,
            inputs,
            ticks,
            applied_speed: mere::canvas::physics_controls::speed_preset(canvas),
            speed_note: None,
        };
        panel.push();
        panel.sync(canvas);
        panel
    }

    /// Set every physics control to the canvas's live choice. A mix root is
    /// custom because its first flat law does not describe the full profile.
    pub(super) fn sync(&mut self, canvas: &Canvas) {
        let live = PhysicsChoice::live(canvas);
        self.law.selected = index_of(PhysicsLaw::ALL, live.law);
        self.kind.selected = index_of(PhysicsKindSource::ALL, live.kind);
        self.mass.selected = index_of(PhysicsMassSource::ALL, live.mass);
        self.depth.selected = index_of(PhysicsDepthSource::ALL, live.depth);
        self.groups.selected = index_of(PhysicsKindSource::ALL, live.groups);
        for (checked, overlay) in self.overlays.iter_mut().zip(PhysicsOverlay::ALL) {
            *checked = live.overlays.contains(&overlay);
        }
        self.profile.selected = live
            .profile_id()
            .and_then(|id| CANVAS_PHYSICS_PROFILES.iter().position(|p| p.id == id))
            .map_or(0, |index| index + 1);
        self.readback = Readback::of(canvas);
        if !matches!(self.readback.mix, Ok(None))
            || !matches!(self.readback.grouping, Ok(None))
            || !matches!(self.readback.schedule, Ok(None))
        {
            // A composed or scheduled root cannot be named by a profile
            // inferred from the live law or first stage.
            self.profile.selected = 0;
        }
        self.pull();
        self.form.follow_roles(&self.readback);
        self.form.follow_composition(&self.readback);
        self.push();
    }

    /// Set the role controls to the canvas's role and role groups, leaving
    /// the composing forms' unapplied values as they are.
    fn sync_roles(&mut self, canvas: &Canvas) {
        self.pull();
        self.form.follow_roles(&Readback::of(canvas));
        self.push();
    }

    /// Read the controls into the form.
    fn pull(&mut self) {
        for id in composing_fields() {
            match self.form.field(id).control {
                Control::Choice { options, .. } => {
                    let picked = self.selects[&id].selected.min(options.len() - 1);
                    self.form.choose(id, options[picked].0);
                },
                Control::Number { .. } => {
                    self.form.enter(id, self.inputs[&id].text());
                },
                Control::Toggle(_) => {
                    self.form.tick(id, self.ticks[&id]);
                },
            }
        }
    }

    /// Show the form in the controls.
    fn push(&mut self) {
        for id in composing_fields() {
            match self.form.field(id).control {
                Control::Choice { options, current } => {
                    if let Some(select) = self.selects.get_mut(&id) {
                        select.selected = index_of(options.iter().map(|(id, _)| *id), current);
                    }
                },
                Control::Number { text, .. } => {
                    if let Some(input) = self.inputs.get_mut(&id)
                        && input.text() != text
                    {
                        *input = TextInput::new(text);
                    }
                },
                Control::Toggle(on) => {
                    self.ticks.insert(id, on);
                },
            }
        }
    }

    /// The form as the controls hold it now, applied or not.
    pub(super) fn pulled(&self) -> PhysicsForm {
        let mut panel_form = self.form.clone();
        for id in composing_fields() {
            match self.form.field(id).control {
                Control::Choice { options, .. } => {
                    let picked = self.selects[&id].selected.min(options.len() - 1);
                    panel_form.choose(id, options[picked].0);
                },
                Control::Number { .. } => {
                    panel_form.enter(id, self.inputs[&id].text());
                },
                Control::Toggle(_) => {
                    panel_form.tick(id, self.ticks[&id]);
                },
            }
        }
        panel_form
    }

    /// The panel's form controls as observation fields: what the scenarios
    /// read back of the editors, unapplied values included.
    pub(super) fn observed(&self) -> Vec<(&'static str, String)> {
        let form = self.pulled();
        let stop = |slot| match form.value(FieldId::StageStop(slot)).as_str() {
            STOP_FRAMES => "Frames",
            STOP_LAW => "law-done",
            _ => "Rest",
        };
        let mut fields = vec![
            ("panel-role", form.value(FieldId::Role)),
            ("panel-role-group-source", form.value(FieldId::RoleGroups)),
            ("panel-mix-first-share", form.value(FieldId::MixShare(Slot::First))),
            ("panel-mix-second-share", form.value(FieldId::MixShare(Slot::Second))),
            ("panel-grouping-partition", form.value(FieldId::GroupPartition)),
            ("panel-grouping-outer", form.value(FieldId::GroupOuter)),
            ("panel-grouping-inner", form.value(FieldId::GroupInner)),
            ("panel-grouping-outer-share", form.value(FieldId::GroupOuterShare)),
            ("panel-grouping-note", self.readback.note(Section::Grouping)),
        ];
        const STAGE_KEYS: [[&str; 4]; 2] = [
            [
                "panel-schedule-first-law",
                "panel-schedule-first-stop",
                "panel-schedule-first-frames",
                "panel-schedule-first-capture",
            ],
            [
                "panel-schedule-second-law",
                "panel-schedule-second-stop",
                "panel-schedule-second-frames",
                "panel-schedule-second-capture",
            ],
        ];
        for (slot, [law, stop_key, frames, capture]) in
            Slot::BOTH.into_iter().zip(STAGE_KEYS)
        {
            fields.push((law, form.value(FieldId::StageLaw(slot))));
            fields.push((stop_key, stop(slot).to_string()));
            fields.push((frames, form.value(FieldId::StageFrames(slot))));
            fields.push((capture, form.value(FieldId::StageCapture(slot))));
        }
        fields.push(("panel-schedule-note", self.readback.note(Section::Schedule)));
        fields.push((
            "panel-schedule-overlays",
            form.schedule_overlays()
                .iter()
                .map(|overlay| overlay.id())
                .collect::<Vec<_>>()
                .join(","),
        ));
        fields
    }

    /// The choice the controls hold. Overlays the picked law refuses are
    /// left out, whatever was ticked before it was picked.
    pub(super) fn choice(&self) -> PhysicsChoice {
        let law = self.picked_law();
        PhysicsChoice {
            law,
            overlays: mere::canvas::physics_controls::ticked_overlays(|overlay| {
                law.refuses(overlay).is_none()
                    && PhysicsOverlay::ALL
                        .iter()
                        .position(|o| *o == overlay)
                        .is_some_and(|index| self.overlays[index])
            }),
            kind: PhysicsKindSource::ALL[self.kind.selected.min(PhysicsKindSource::ALL.len() - 1)],
            groups: PhysicsKindSource::ALL
                [self.groups.selected.min(PhysicsKindSource::ALL.len() - 1)],
            mass: PhysicsMassSource::ALL[self.mass.selected.min(PhysicsMassSource::ALL.len() - 1)],
            depth: PhysicsDepthSource::ALL
                [self.depth.selected.min(PhysicsDepthSource::ALL.len() - 1)],
        }
    }

    /// The profile id the picker shows, or custom.
    pub(super) fn profile_id(&self) -> &'static str {
        self.profile
            .selected
            .checked_sub(1)
            .and_then(|index| CANVAS_PHYSICS_PROFILES.get(index))
            .map_or(CUSTOM_PROFILE, |profile| profile.id)
    }

    /// The overlay ids ticked in the panel, comma-joined.
    pub(super) fn ticked(&self) -> String {
        self.choice()
            .overlays
            .iter()
            .map(|overlay| overlay.id())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Whether the speed picker names a preset the canvas does not run yet.
    pub(super) fn speed_pending(&self) -> bool {
        self.selects[&FieldId::Speed].selected != self.applied_speed
    }

    /// The law the picker names (applied or not).
    pub(super) fn picked_law(&self) -> PhysicsLaw {
        PhysicsLaw::ALL[self.law.selected.min(PhysicsLaw::ALL.len() - 1)]
    }

    fn select_mut(&mut self, id: FieldId) -> &mut SelectState {
        self.selects.get_mut(&id).expect("a choice field has a select")
    }

    fn input_mut(&mut self, id: FieldId) -> &mut TextInput {
        self.inputs.get_mut(&id).expect("a number field has an input")
    }

    fn tick_mut(&mut self, id: FieldId) -> &mut bool {
        self.ticks.get_mut(&id).expect("a toggle field has a box")
    }
}

impl TreePage {
    fn apply_arrangement(&mut self) {
        let Some((id, _)) = arrangement_choices().nth(self.physics.arrangement.selected) else {
            return;
        };
        let mut canvas = self.shared.canvas.borrow_mut();
        self.physics.transition = None;
        self.physics.status =
            match physics_controls::apply_arrangement(&mut canvas, id, self.shared.size.get()) {
                Ok(applied) => {
                    self.physics.layout_id = applied.layout_id;
                    self.physics.transition = applied.transition;
                    applied.status
                },
                Err(error) => format!("Failed · {error}"),
            };
        self.shared.dirty.set(true);
    }

    /// A form button: read the controls into the model, and run what it asks.
    fn press(&mut self, command: Command) {
        self.physics.pull();
        match self.physics.form.action(command, &self.physics.choice()) {
            Ok(action) => self.run_form(action),
            Err(refusal) => self.physics.status = refusal,
        }
    }

    /// Run an edit the form asked for. A law-changing edit remembers where
    /// the law started and refreshes the controls from the canvas; a role
    /// edit refreshes the role controls alone.
    fn run_form(&mut self, action: FormAction) {
        let mut canvas = self.shared.canvas.borrow_mut();
        let law_start = action.changes_law().then(|| LawStart::of(&canvas));
        let outcome = action.run(&mut canvas);
        self.physics.status = outcome.status;
        if !outcome.applied {
            return;
        }
        if law_start.is_some() {
            self.physics.law_start = law_start;
            self.physics.sync(&canvas);
        } else {
            self.physics.sync_roles(&canvas);
        }
        self.shared.dirty.set(true);
    }

    fn apply_physics(&mut self) {
        let choice = self.physics.choice();
        let mut canvas = self.shared.canvas.borrow_mut();
        self.physics.law_start = Some(LawStart::of(&canvas));
        self.physics.status = physics_controls::apply_physics(&mut canvas, &choice);
        self.physics.sync(&canvas);
        self.shared.dirty.set(true);
    }

    fn apply_profile(&mut self) {
        let id = self.physics.profile_id();
        let mut canvas = self.shared.canvas.borrow_mut();
        self.physics.law_start = Some(LawStart::of(&canvas));
        self.physics.status = match physics_controls::apply_profile(&mut canvas, id) {
            Ok(status) => status,
            Err(error) => format!("Failed · {error}"),
        };
        self.physics.sync(&canvas);
        self.shared.dirty.set(true);
    }

    /// The picked speed, applied when chosen (ruled 2026-10-04, "Speed select").
    pub(super) fn apply_speed(&mut self) {
        self.physics.pull();
        let mut canvas = self.shared.canvas.borrow_mut();
        self.physics.status = self.physics.form.speed_action().run(&mut canvas).status;
        self.physics.applied_speed = self.physics.selects[&FieldId::Speed].selected;
        self.shared.dirty.set(true);
    }

    /// Step a running arrangement transition at host time `host_ms`. Under
    /// reduced motion it lands on this frame: a second step, far enough on
    /// to finish any schedule.
    pub(super) fn advance_arrangement(&mut self, host_ms: f64) {
        /// Past the end of any arrangement transition, in ms.
        const LAND_MS: f64 = 3_600_000.0;
        let reduced = self.shared.reduced_motion();
        let mut canvas = self.shared.canvas.borrow_mut();
        let steps: &[f64] = if reduced {
            &[host_ms, host_ms + LAND_MS]
        } else {
            &[host_ms]
        };
        for &at in steps {
            if let Some(status) = physics_controls::advance_arrangement(
                &mut canvas,
                &mut self.physics.transition,
                &self.physics.layout_id,
                at,
            ) {
                self.physics.status = status;
            }
        }
        self.shared.dirty.set(true);
    }

    /// Apply the arrangement last applied again, onto a new graph: a host
    /// history's checkpoint keeps the reader's arrangement.
    pub(super) fn reapply_arrangement(&mut self) {
        let layout_id = self.physics.layout_id.clone();
        let mut canvas = self.shared.canvas.borrow_mut();
        self.physics.transition = None;
        match physics_controls::apply_arrangement(&mut canvas, &layout_id, self.shared.size.get()) {
            Ok(applied) => {
                self.physics.transition = applied.transition;
                self.physics.status = applied.status;
            },
            Err(error) => self.physics.status = format!("Failed · {error}"),
        }
    }
}

/// A labelled select bound to one of the panel's states.
fn picker(
    caption: &'static str,
    options: Vec<&'static str>,
    state: fn(&mut TreePage) -> &mut SelectState,
) -> Child {
    Box::new(el(
        "label",
        (
            el("span", caption).attr("class", "tools-caption"),
            lens(move |s: &mut SelectState| select(s, &options), state),
        ),
    ))
}

/// The attributes a scenario finds a composing control by.
fn hooks(id: FieldId) -> Vec<(&'static str, &'static str)> {
    use FieldId::*;
    let stage = |slot: Slot, part| {
        vec![
            ("data-physics-schedule-stage", slot.id()),
            ("data-physics-schedule-field", part),
        ]
    };
    match id {
        MixLaw(slot) => vec![("data-physics-law-role", slot.id())],
        GroupOuter => vec![("data-physics-law-role", "group-outer")],
        GroupInner => vec![("data-physics-law-role", "group-inner")],
        MixShare(slot) => vec![("data-physics-share-role", slot.id())],
        GroupOuterShare => vec![("data-physics-share-role", "group-outer")],
        StageLaw(slot) => stage(slot, "law"),
        StageStop(slot) => stage(slot, "stop"),
        StageFrames(slot) => stage(slot, "frames"),
        StageCapture(slot) => stage(slot, "capture"),
        ScheduleOverlay(overlay) => vec![
            ("data-physics-schedule-overlay", overlay.id()),
            ("data-overlay-id", overlay.id()),
        ],
        Role | RoleGroups | Speed | GroupPartition => Vec::new(),
    }
}

/// `label` with the attributes a scenario finds the field by.
fn hooked<Seq, State, Action>(
    mut label: cambium::El<Seq, State, Action>,
    id: FieldId,
) -> cambium::El<Seq, State, Action> {
    for (name, value) in hooks(id) {
        label = label.attr(name, value);
    }
    label
}

/// One of the form's fields as a labelled control bound to the panel's
/// state for it. Parsing and validity checks happen at Apply, where a
/// refusal leaves the live canvas untouched.
fn field_view(field: Field) -> Child {
    let id = field.id;
    let caption = field.label;
    match field.control {
        Control::Choice { options, .. } => {
            let options: Vec<&'static str> = options.iter().map(|(_, label)| *label).collect();
            Box::new(hooked(
                el(
                    "label",
                    (
                        el("span", caption).attr("class", "tools-caption"),
                        lens(
                            move |s: &mut SelectState| select(s, &options),
                            move |page: &mut TreePage| page.physics.select_mut(id),
                        ),
                    ),
                ),
                id,
            ))
        },
        Control::Number { .. } => Box::new(hooked(
            el(
                "label",
                (
                    el("span", caption.clone()).attr("class", "tools-caption"),
                    lens(
                        move |input: &mut TextInput| {
                            text_field_typed(input).attr("aria-label", caption.clone())
                        },
                        move |page: &mut TreePage| page.physics.input_mut(id),
                    ),
                ),
            ),
            id,
        )),
        Control::Toggle(_) => {
            let shown = match id {
                FieldId::ScheduleOverlay(overlay) => overlay.label(),
                _ => "",
            };
            Box::new(hooked(
                el(
                    "label",
                    (
                        lens(
                            move |checked: &mut bool| {
                                checkbox(*checked).attr("aria-label", caption.clone())
                            },
                            move |page: &mut TreePage| page.physics.tick_mut(id),
                        ),
                        el("span", shown),
                    ),
                ),
                id,
            ))
        },
    }
}

fn tools_note(text: &'static str) -> Child {
    Box::new(el("p", text).attr("class", "tools-note"))
}

/// Common schedule overlays are intentionally never filtered or disabled by
/// either stage law. The complete selected set is offered to the binder.
fn overlay_fieldset(page: &TreePage, group: &Group) -> Child {
    let mut body: Vec<Child> = vec![Box::new(el("legend", group.title.unwrap_or_default()))];
    body.extend(group.note.map(tools_note));
    body.push(Box::new(el(
        "div",
        group
            .fields
            .iter()
            .map(|id| field_view(page.physics.form.field(*id)))
            .collect::<Vec<_>>(),
    )));
    Box::new(el("fieldset", body).attr("data-physics-schedule-overlays", "common"))
}

/// A form button. Role's sits bare; the others carry an id for scenarios.
fn command_button(command: Command) -> Child {
    let button = button(command.label(), move |page: &mut TreePage, _| {
        page.press(command)
    });
    if command == Command::ApplyRole {
        Box::new(button)
    } else {
        Box::new(el("div", button).attr("data-physics-action", command.id()))
    }
}

/// One composing editor: its fields in groups, its notes and buttons, and
/// what the canvas authored for it.
fn editor(page: &TreePage, section: Section) -> Child {
    let mut body: Vec<Child> = vec![Box::new(el("h3", section.title()))];
    body.extend(section.intro().iter().copied().map(tools_note));
    for group in section.groups() {
        if matches!(group.fields.first(), Some(FieldId::ScheduleOverlay(_))) {
            body.push(overlay_fieldset(page, &group));
            continue;
        }
        body.extend(group.title.map(|title| Box::new(el("h4", title)) as Child));
        body.extend(
            group
                .fields
                .iter()
                .map(|id| field_view(page.physics.form.field(*id))),
        );
        body.extend(group.note.map(tools_note));
    }
    body.extend(section.commands().iter().copied().map(command_button));
    body.push(Box::new(status_note_labelled(
        "tools-note",
        section.status_label(),
        page.physics.readback.note(section),
    )));
    Box::new(el("div", body).attr("data-physics-composition-editor", section.editor()))
}

fn apply(label: &'static str, action: fn(&mut TreePage)) -> Child {
    Box::new(button(label, move |page: &mut TreePage, _| action(page)))
}

/// The overlay checkboxes, each greyed and inert while the picked law
/// refuses it, with the law's reason beneath.
fn overlay_group(page: &TreePage) -> Child {
    let law = page.physics.picked_law();
    let mut reason = None;
    let mut boxes: Vec<Child> = CANVAS_PHYSICS_OVERLAYS
        .iter()
        .zip(PhysicsOverlay::ALL)
        .enumerate()
        .map(
            |(index, ((_, label), overlay))| match law.refuses(overlay) {
                None => Box::new(el(
                    "label",
                    (
                        lens(
                            move |checked: &mut bool| {
                                checkbox(*checked)
                                    .attr("aria-label", *label)
                                    .attr("data-overlay", overlay.id())
                            },
                            move |page: &mut TreePage| &mut page.physics.overlays[index],
                        ),
                        el("span", *label),
                    ),
                )) as Child,
                Some(why) => {
                    reason = Some(why);
                    Box::new(
                        el(
                            "label",
                            (
                                checkbox_disabled(false)
                                    .attr("data-overlay", overlay.id())
                                    .attr("aria-label", *label)
                                    .attr("aria-describedby", "tools-overlay-note"),
                                el("span", *label),
                            ),
                        )
                        .attr("class", "disabled"),
                    ) as Child
                },
            },
        )
        .collect();
    if let Some(reason) = reason {
        boxes.push(Box::new(
            status_note_labelled("tools-note", "Overlay availability", reason).attr("id", "tools-overlay-note"),
        ));
    }
    let group = el("div", boxes)
        .attr("class", "tools-overlays")
        .attr("role", "group")
        .attr("aria-label", "Overlays");
    let all_refused = PhysicsOverlay::ALL
        .iter()
        .all(|o| law.refuses(*o).is_some());
    if all_refused {
        Box::new(group.attr("aria-disabled", "true"))
    } else {
        Box::new(group)
    }
}

/// The speed reached, beneath the speed picker, while the budget binds.
fn speed_note(page: &TreePage) -> Child {
    match &page.physics.speed_note {
        Some(note) => Box::new(
            status_note_labelled("tools-note", "Speed status", note.clone()),
        ),
        None => Box::new(el("span", "").attr("hidden", "")),
    }
}

/// The "Graph tools" region's arrangement and physics section.
pub(super) fn section(page: &TreePage) -> Child {
    let profiles: Vec<&'static str> = std::iter::once(CUSTOM_LABEL)
        .chain(CANVAS_PHYSICS_PROFILES.iter().map(|profile| profile.label))
        .collect();
    let field = |id: FieldId| field_view(page.physics.form.field(id));
    let section: Vec<Child> = vec![
        picker(
            "Arrangement",
            arrangement_choices().map(|(_, label)| label).collect(),
            |page| &mut page.physics.arrangement,
        ),
        apply("Apply arrangement", TreePage::apply_arrangement),
        field(FieldId::Role),
        command_button(Command::ApplyRole),
        picker("Physics law", labels(CANVAS_PHYSICS_LAWS), |page| {
            &mut page.physics.law
        }),
        overlay_group(page),
        picker("Kinds", labels(CANVAS_PHYSICS_KIND_SOURCES), |page| {
            &mut page.physics.kind
        }),
        picker("Groups", labels(CANVAS_PHYSICS_KIND_SOURCES), |page| {
            &mut page.physics.groups
        }),
        field(FieldId::RoleGroups),
        tools_note(
            "Role groups chooses which groups role overrides use. Groups supplies Group pull.",
        ),
        command_button(Command::ApplyRoleGroups),
        picker("Mass", labels(CANVAS_PHYSICS_MASS_SOURCES), |page| {
            &mut page.physics.mass
        }),
        picker("Depth", labels(CANVAS_PHYSICS_DEPTH_SOURCES), |page| {
            &mut page.physics.depth
        }),
        apply("Apply physics", TreePage::apply_physics),
        editor(page, Section::Mix),
        editor(page, Section::Grouping),
        editor(page, Section::Schedule),
        field(FieldId::Speed),
        speed_note(page),
        picker("Profile", profiles, |page| &mut page.physics.profile),
        apply("Apply profile", TreePage::apply_profile),
        Box::new(
            status_note_labelled("tools-status", "Physics status", page.physics.status.clone()),
        ),
    ];
    Box::new(
        el(
            "section",
            disclosure(&page.sections.physics, section, |page: &mut TreePage| {
                page.sections.physics.toggle()
            }),
        )
        .attr("class", "tools-section")
        .attr("aria-label", "Arrangement and physics"),
    )
}
