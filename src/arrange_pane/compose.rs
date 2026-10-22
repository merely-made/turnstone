// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The Arrange pane's composing controls: role, two-law mix, grouping,
//! two-stage schedule and speed, the native counterpart of the same sections
//! in the web host's tools region. Each edits a draft held by the pane and
//! applies it through the `mere::canvas::physics_controls` input the web
//! host's form builds, so the canvas decides what is admitted. The drafts
//! follow the canvas when what it authored changes, not on every frame.

use mere::canvas::physics_controls::{
    StageDraft, TwoLawGrouping, TwoLawMix, TwoStageSchedule,
};
use mere::canvas::{
    CANVAS_PHYSICS_KIND_SOURCES, CANVAS_PHYSICS_LAWS, PhysicsKindSource, PhysicsLaw,
    PhysicsOverlay, Role, StageStop,
};
use mere_surface_api::settings::{SettingControl, SettingValue};

use super::{
    ArrangeIntent, ArrangeState, ArrangeView, Authored, apply_button, catalog_options,
    choice_control, choice_row, el, section, setting_row, spec,
};

const STOP_REST: &str = "rest";
const STOP_FRAMES: &str = "frames";
const STOP_LAW: &str = "law";
const NO_CAPTURE: &str = "none";
const DEFAULT_FRAMES: u32 = 120;

/// One stage of the schedule form: `frames` is kept while the stop is not
/// Frames, so switching back restores what was typed.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct StageForm {
    law: PhysicsLaw,
    stop: &'static str,
    frames: u32,
    capture: Option<Role>,
}

impl StageForm {
    fn new(law: PhysicsLaw) -> Self {
        Self {
            law,
            stop: STOP_REST,
            frames: DEFAULT_FRAMES,
            capture: None,
        }
    }

    fn of(draft: StageDraft) -> Self {
        let (stop, frames) = match draft.stop {
            StageStop::Rest => (STOP_REST, DEFAULT_FRAMES),
            StageStop::Frames(count) => (STOP_FRAMES, count),
            StageStop::LawDone => (STOP_LAW, DEFAULT_FRAMES),
        };
        Self {
            law: draft.law,
            stop,
            frames,
            capture: draft.capture,
        }
    }

    fn draft(&self) -> StageDraft {
        StageDraft {
            law: self.law,
            stop: match self.stop {
                STOP_FRAMES => StageStop::Frames(self.frames.max(1)),
                STOP_LAW => StageStop::LawDone,
                _ => StageStop::Rest,
            },
            capture: self.capture,
        }
    }
}

/// What the pane's composing forms hold between Apply presses.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Drafts {
    mix: TwoLawMix,
    grouping: TwoLawGrouping,
    first: StageForm,
    second: StageForm,
    overlays: Vec<PhysicsOverlay>,
}

impl Default for Drafts {
    fn default() -> Self {
        Self {
            mix: TwoLawMix {
                first: PhysicsLaw::ALL[0],
                second: PhysicsLaw::ALL[1],
                first_share: 1.0,
                second_share: 1.0,
            },
            grouping: TwoLawGrouping {
                partition: PhysicsKindSource::ALL[0],
                outer: PhysicsLaw::Charge,
                inner: PhysicsLaw::Springs,
                outer_share: 1.0,
            },
            first: StageForm::new(PhysicsLaw::ALL[0]),
            second: StageForm::new(PhysicsLaw::ALL[1]),
            overlays: Vec::new(),
        }
    }
}

impl Drafts {
    /// Take what the canvas authored, for each form that it authored in the
    /// shape the form can show. A form the canvas has not authored keeps its
    /// draft.
    pub(super) fn follow(&mut self, authored: &Authored) {
        if let Ok(Some(mix)) = &authored.mix {
            self.mix = *mix;
        }
        if let Ok(Some(grouping)) = &authored.grouping {
            self.grouping = *grouping;
        }
        if let Ok(Some(schedule)) = &authored.schedule {
            self.first = StageForm::of(schedule.first);
            self.second = StageForm::of(schedule.second);
            self.overlays = schedule.overlays.clone();
        }
    }

    fn schedule(&self) -> TwoStageSchedule {
        TwoStageSchedule {
            first: self.first.draft(),
            second: self.second.draft(),
            overlays: PhysicsOverlay::ALL
                .into_iter()
                .filter(|overlay| self.overlays.contains(overlay))
                .collect(),
        }
    }
}

/// The note under a form: what the canvas authored, when the form cannot
/// show it.
fn authored_note<T>(what: &str, authored: &Result<Option<T>, String>) -> String {
    match authored {
        Ok(Some(_)) => String::new(),
        Ok(None) => format!("No authored {what}"),
        Err(reason) => format!(
            "Unsupported authored {what} shape · {reason} · editor values are not readback"
        ),
    }
}

/// The notes as plain rows, for the observe snapshot.
pub(super) fn authored_rows(authored: &Authored) -> [String; 3] {
    [
        format!("Mix: {}", mix_text(&authored.mix)),
        format!("Grouping: {}", grouping_text(&authored.grouping)),
        format!("Schedule: {}", schedule_text(&authored.schedule)),
    ]
}

fn mix_text(authored: &Result<Option<TwoLawMix>, String>) -> String {
    match authored {
        Ok(Some(mix)) => format!(
            "{} {} + {} {}",
            mix.first.label(),
            mix.first_share,
            mix.second.label(),
            mix.second_share
        ),
        other => authored_note("two-law mix", other),
    }
}

fn grouping_text(authored: &Result<Option<TwoLawGrouping>, String>) -> String {
    match authored {
        Ok(Some(grouping)) => format!(
            "{} between {} groups, {} within, outer share {}",
            grouping.outer.label(),
            grouping.partition.label(),
            grouping.inner.label(),
            grouping.outer_share
        ),
        other => authored_note("grouping", other),
    }
}

fn schedule_text(authored: &Result<Option<TwoStageSchedule>, String>) -> String {
    match authored {
        Ok(Some(schedule)) => format!(
            "{} then {}",
            schedule.first.law.label(),
            schedule.second.law.label()
        ),
        other => authored_note("two-stage schedule", other),
    }
}

fn status(text: String, label: &str) -> ArrangeView {
    Box::new(
        el::<_, ArrangeState, ArrangeIntent>("div", text)
            .attr("role", "status")
            .attr("aria-label", label.to_string()),
    )
}

/// A choice whose pick lands in the draft instead of applying.
fn draft_choice(
    id: &str,
    label: &str,
    options: Vec<(String, String)>,
    current: &str,
    set: impl Fn(&mut Drafts, &str) + 'static,
) -> ArrangeView {
    let spec = spec(
        id,
        label,
        choice_control(options),
        SettingValue::Text(current.to_string()),
    );
    Box::new(setting_row(
        &spec,
        label,
        move |state: &mut ArrangeState, value: SettingValue| {
            if let SettingValue::Text(id) = value {
                set(&mut state.drafts, &id);
            }
            None::<ArrangeIntent>
        },
    ))
}

/// A number whose entry lands in the draft instead of applying.
fn draft_number(
    id: &str,
    label: &str,
    current: f64,
    min: f64,
    set: impl Fn(&mut Drafts, f64) + 'static,
) -> ArrangeView {
    let spec = spec(
        id,
        label,
        SettingControl::Number {
            min: Some(min),
            max: None,
            step: None,
        },
        SettingValue::Number(current),
    );
    Box::new(setting_row(
        &spec,
        label,
        move |state: &mut ArrangeState, value: SettingValue| {
            if let SettingValue::Number(number) = value
                && number.is_finite()
            {
                set(&mut state.drafts, number);
            }
            None::<ArrangeIntent>
        },
    ))
}

fn draft_toggle(overlay: PhysicsOverlay, on: bool) -> ArrangeView {
    let spec = spec(
        &format!("schedule.overlay.{}", overlay.id()),
        overlay.label(),
        SettingControl::Toggle,
        SettingValue::Boolean(on),
    );
    Box::new(setting_row(
        &spec,
        overlay.label(),
        move |state: &mut ArrangeState, value: SettingValue| {
            if let SettingValue::Boolean(on) = value {
                state.drafts.overlays.retain(|known| *known != overlay);
                if on {
                    state.drafts.overlays.push(overlay);
                }
            }
            None::<ArrangeIntent>
        },
    ))
}

fn law_options() -> Vec<(String, String)> {
    catalog_options(CANVAS_PHYSICS_LAWS)
}

fn set_law(slot: &mut PhysicsLaw, id: &str) {
    if let Some(law) = PhysicsLaw::parse(id) {
        *slot = law;
    }
}

fn stop_options() -> Vec<(String, String)> {
    [
        (STOP_REST, "Rest"),
        (STOP_FRAMES, "After frames"),
        (STOP_LAW, "Law done"),
    ]
    .into_iter()
    .map(|(id, label)| (id.to_string(), label.to_string()))
    .collect()
}

fn capture_options() -> Vec<(String, String)> {
    std::iter::once((NO_CAPTURE.to_string(), "No capture".to_string()))
        .chain(
            Role::ALL
                .iter()
                .map(|role| (role.id().to_string(), role.label().to_string())),
        )
        .collect()
}

fn stage_rows(
    prefix: &'static str,
    title: &'static str,
    form: &StageForm,
    stage: fn(&mut Drafts) -> &mut StageForm,
) -> Vec<ArrangeView> {
    let stop_label = format!("{title} stop");
    let capture_current = form.capture.map_or(NO_CAPTURE, Role::id);
    let mut rows = vec![
        section(title),
        draft_choice(
            &format!("schedule.{prefix}.law"),
            &format!("{title} law"),
            law_options(),
            form.law.id(),
            move |drafts, id| set_law(&mut stage(drafts).law, id),
        ),
        draft_choice(
            &format!("schedule.{prefix}.stop"),
            &stop_label,
            stop_options(),
            form.stop,
            move |drafts, id| {
                stage(drafts).stop = match id {
                    STOP_FRAMES => STOP_FRAMES,
                    STOP_LAW => STOP_LAW,
                    _ => STOP_REST,
                }
            },
        ),
    ];
    if form.stop == STOP_FRAMES {
        rows.push(draft_number(
            &format!("schedule.{prefix}.frames"),
            &format!("{title} frames"),
            f64::from(form.frames),
            1.0,
            move |drafts, number| stage(drafts).frames = number.round().clamp(1.0, f64::from(u32::MAX)) as u32,
        ));
    }
    rows.push(draft_choice(
        &format!("schedule.{prefix}.capture"),
        &format!("{title} capture"),
        capture_options(),
        capture_current,
        move |drafts, id| stage(drafts).capture = Role::parse(id),
    ));
    rows
}

/// The composing sections, after Sources.
pub(super) fn views(state: &ArrangeState) -> Vec<ArrangeView> {
    let choice = &state.choice;
    let drafts = &state.drafts;
    let kind_options = || catalog_options(CANVAS_PHYSICS_KIND_SOURCES);
    let speed_options = mere::canvas::physics_controls::SPEED_PRESETS
        .iter()
        .map(|(id, label)| ((*id).to_string(), (*label).to_string()))
        .collect();
    let mut views: Vec<ArrangeView> = vec![
        section("Roles"),
        choice_row(
            "physics.role",
            "Role",
            Role::ALL
                .iter()
                .map(|role| (role.id().to_string(), role.label().to_string()))
                .collect(),
            &choice.role,
            ArrangeIntent::SetRole,
        ),
        choice_row(
            "physics.role-groups",
            "Role groups",
            kind_options(),
            &choice.role_groups,
            ArrangeIntent::SetRoleGroups,
        ),
        section("Two-law mix"),
        status(mix_text(&choice.authored.mix), "Authored mix status"),
        draft_choice(
            "mix.first",
            "First law",
            law_options(),
            drafts.mix.first.id(),
            |drafts, id| set_law(&mut drafts.mix.first, id),
        ),
        draft_number("mix.first-share", "First share", drafts.mix.first_share, 0.0, |drafts, n| {
            drafts.mix.first_share = n
        }),
        draft_choice(
            "mix.second",
            "Second law",
            law_options(),
            drafts.mix.second.id(),
            |drafts, id| set_law(&mut drafts.mix.second, id),
        ),
        draft_number("mix.second-share", "Second share", drafts.mix.second_share, 0.0, |drafts, n| {
            drafts.mix.second_share = n
        }),
        apply_button("Apply mix", |state| ArrangeIntent::ApplyMix(state.drafts.mix)),
        apply_button("Use single law", |_| ArrangeIntent::UseSingleLaw),
        section("Grouped composition"),
        status(
            grouping_text(&choice.authored.grouping),
            "Authored grouping status",
        ),
        draft_choice(
            "grouping.partition",
            "Grouping partition",
            kind_options(),
            drafts.grouping.partition.id(),
            |drafts, id| {
                if let Some(source) = PhysicsKindSource::parse(id) {
                    drafts.grouping.partition = source;
                }
            },
        ),
        draft_choice(
            "grouping.outer",
            "Outer law",
            law_options(),
            drafts.grouping.outer.id(),
            |drafts, id| set_law(&mut drafts.grouping.outer, id),
        ),
        draft_number(
            "grouping.outer-share",
            "Outer share",
            drafts.grouping.outer_share,
            0.0,
            |drafts, n| drafts.grouping.outer_share = n,
        ),
        draft_choice(
            "grouping.inner",
            "Inner law",
            law_options(),
            drafts.grouping.inner.id(),
            |drafts, id| set_law(&mut drafts.grouping.inner, id),
        ),
        apply_button("Apply grouping", |state| {
            ArrangeIntent::ApplyGrouping(state.drafts.grouping)
        }),
        section("Two-stage schedule"),
        status(
            schedule_text(&choice.authored.schedule),
            "Authored schedule status",
        ),
    ];
    views.extend(stage_rows("first", "Stage 1", &drafts.first, |d| &mut d.first));
    views.extend(stage_rows("second", "Stage 2", &drafts.second, |d| &mut d.second));
    views.extend(PhysicsOverlay::ALL.iter().map(|overlay| {
        draft_toggle(*overlay, drafts.overlays.contains(overlay))
    }));
    views.push(apply_button("Apply and run schedule", |state| {
        ArrangeIntent::ApplySchedule(state.drafts.schedule())
    }));
    views.push(section("Speed"));
    views.push(choice_row(
        "physics.speed",
        "Speed",
        speed_options,
        &choice.speed,
        ArrangeIntent::SetSpeed,
    ));
    views
}
