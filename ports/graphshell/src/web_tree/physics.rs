// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The "Graph tools" side region's first section: arrangement and physics.
//!
//! Cambium controls hold the form; Apply reads them into the typed inputs of
//! `graphshell::canvas_physics`, which the old page's form also calls.
use super::*;
use cambium::{SelectState, button, checkbox, disclosure, lens, select};
use graphshell::canvas_physics::{
    self, ArrangementTransition, CUSTOM_PROFILE, LawStart, arrangement_choices,
};
use mere::canvas::{
    CANVAS_PHYSICS_DEPTH_SOURCES, CANVAS_PHYSICS_KIND_SOURCES, CANVAS_PHYSICS_LAWS,
    CANVAS_PHYSICS_MASS_SOURCES, CANVAS_PHYSICS_OVERLAYS, CANVAS_PHYSICS_PROFILES, PhysicsChoice,
    PhysicsDepthSource, PhysicsKindSource, PhysicsLaw, PhysicsMassSource, PhysicsOverlay, Role,
};

/// The profile picker's first entry, read when the live pair names no profile.
const CUSTOM_LABEL: &str = "Custom (no profile)";

/// The panel's control state and the arrangement it last applied.
pub(super) struct PhysicsPanel {
    pub(super) arrangement: SelectState,
    /// The recipe's role for the arrangement's positions (F48).
    pub(super) role: SelectState,
    pub(super) law: SelectState,
    pub(super) kind: SelectState,
    pub(super) mass: SelectState,
    pub(super) depth: SelectState,
    /// Index 0 is custom; profile `i` is at `i + 1`.
    pub(super) profile: SelectState,
    /// One per `CANVAS_PHYSICS_OVERLAYS` entry, in catalog order.
    pub(super) overlays: Vec<bool>,
    pub(super) layout_id: String,
    pub(super) status: String,
    pub(super) transition: Option<ArrangementTransition>,
    /// The layout where the last law was applied.
    pub(super) law_start: Option<LawStart>,
    /// The speed preset picked, the one the canvas runs, and the note on the
    /// speed reached while the budget binds.
    pub(super) speed: SelectState,
    pub(super) applied_speed: usize,
    pub(super) speed_note: Option<String>,
}

fn index_of<T: PartialEq>(items: impl IntoIterator<Item = T>, item: T) -> usize {
    items.into_iter().position(|i| i == item).unwrap_or(0)
}

fn labels(catalog: &[(&'static str, &'static str)]) -> Vec<&'static str> {
    catalog.iter().map(|(_, label)| *label).collect()
}

impl PhysicsPanel {
    pub(super) fn new(canvas: &Canvas, layout_id: &str) -> Self {
        let mut panel = Self {
            arrangement: SelectState::new(index_of(
                arrangement_choices().map(|(id, _)| id),
                layout_id,
            ))
            .with_label("Arrangement"),
            role: SelectState::new(0).with_label("Role"),
            law: SelectState::new(0).with_label("Physics law"),
            kind: SelectState::new(0).with_label("Kinds"),
            mass: SelectState::new(0).with_label("Mass"),
            depth: SelectState::new(0).with_label("Depth"),
            profile: SelectState::new(0).with_label("Profile"),
            overlays: vec![false; CANVAS_PHYSICS_OVERLAYS.len()],
            layout_id: layout_id.to_string(),
            status: String::new(),
            transition: None,
            law_start: None,
            speed: SelectState::new(crate::web_speed::preset_of(canvas)).with_label("Speed"),
            applied_speed: crate::web_speed::preset_of(canvas),
            speed_note: None,
        };
        panel.sync(canvas);
        panel
    }

    /// Set every physics control to the canvas's live choice, and the profile
    /// to the one naming the pair, or custom.
    pub(super) fn sync(&mut self, canvas: &Canvas) {
        let live = canvas.physics_choice();
        self.role.selected = index_of(Role::ALL, canvas.arrangement_roles().default);
        self.law.selected = index_of(PhysicsLaw::ALL, live.law);
        self.kind.selected = index_of(PhysicsKindSource::ALL, live.kind);
        self.mass.selected = index_of(PhysicsMassSource::ALL, live.mass);
        self.depth.selected = index_of(PhysicsDepthSource::ALL, live.depth);
        for (checked, overlay) in self.overlays.iter_mut().zip(PhysicsOverlay::ALL) {
            *checked = live.overlays.contains(&overlay);
        }
        self.profile.selected = canvas
            .physics_profile_id()
            .and_then(|id| CANVAS_PHYSICS_PROFILES.iter().position(|p| p.id == id))
            .map_or(0, |index| index + 1);
    }

    /// The recipe role the role control holds.
    pub(super) fn role(&self) -> Role {
        Role::ALL[self.role.selected.min(Role::ALL.len() - 1)]
    }

    /// The choice the controls hold. Overlays the picked law refuses are
    /// left out, whatever was ticked before it was picked.
    pub(super) fn choice(&self) -> PhysicsChoice {
        let law = self.picked_law();
        PhysicsChoice {
            law,
            overlays: canvas_physics::ticked_overlays(|overlay| {
                law.refuses(overlay).is_none()
                    && PhysicsOverlay::ALL
                        .iter()
                        .position(|o| *o == overlay)
                        .is_some_and(|index| self.overlays[index])
            }),
            kind: PhysicsKindSource::ALL[self.kind.selected.min(PhysicsKindSource::ALL.len() - 1)],
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
        self.speed.selected != self.applied_speed
    }

    /// The law the picker names (applied or not).
    pub(super) fn picked_law(&self) -> PhysicsLaw {
        PhysicsLaw::ALL[self.law.selected.min(PhysicsLaw::ALL.len() - 1)]
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
            match canvas_physics::apply_arrangement(&mut canvas, id, self.shared.size.get()) {
                Ok(applied) => {
                    self.physics.layout_id = applied.layout_id;
                    self.physics.transition = applied.transition;
                    applied.status
                },
                Err(error) => format!("Failed · {error}"),
            };
        self.shared.dirty.set(true);
    }

    /// The recipe's role: every item takes it unless a group or the item
    /// says otherwise (F22, F48).
    fn apply_role(&mut self) {
        let role = self.physics.role();
        self.shared.canvas.borrow_mut().set_arrangement_role(role);
        self.physics.status = format!("Role set to {}", role.id());
        self.shared.dirty.set(true);
    }

    fn apply_physics(&mut self) {
        let choice = self.physics.choice();
        let mut canvas = self.shared.canvas.borrow_mut();
        self.physics.law_start = Some(LawStart::of(&canvas));
        self.physics.status = canvas_physics::apply_physics(&mut canvas, &choice);
        self.physics.sync(&canvas);
        self.shared.dirty.set(true);
    }

    /// The picked speed, applied when chosen (ruled 2026-10-04, "Speed select").
    pub(super) fn apply_speed(&mut self) {
        let index = self.physics.speed.selected;
        let mut canvas = self.shared.canvas.borrow_mut();
        self.physics.status = crate::web_speed::choose(&mut canvas, index);
        self.physics.applied_speed = index;
        self.shared.dirty.set(true);
    }

    fn apply_profile(&mut self) {
        let id = self.physics.profile_id();
        let mut canvas = self.shared.canvas.borrow_mut();
        self.physics.law_start = Some(LawStart::of(&canvas));
        self.physics.status = match canvas_physics::apply_profile(&mut canvas, id) {
            Ok(status) => status,
            Err(error) => format!("Failed · {error}"),
        };
        self.physics.sync(&canvas);
        self.shared.dirty.set(true);
    }

    /// Step a running arrangement transition at host time `host_ms`.
    pub(super) fn advance_arrangement(&mut self, host_ms: f64) {
        let mut canvas = self.shared.canvas.borrow_mut();
        if let Some(status) = canvas_physics::advance_arrangement(
            &mut canvas,
            &mut self.physics.transition,
            &self.physics.layout_id,
            host_ms,
        ) {
            self.physics.status = status;
        }
        self.shared.dirty.set(true);
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
                            move |checked: &mut bool| checkbox(*checked).attr("aria-label", *label),
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
                                el("span", "[ ]")
                                    .attr("role", "checkbox")
                                    .attr("aria-label", *label)
                                    .attr("aria-checked", "false")
                                    .attr("aria-disabled", "true")
                                    .attr("aria-describedby", "tools-overlay-note")
                                    .attr("class", "checkbox disabled"),
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
            el("p", reason)
                .attr("id", "tools-overlay-note")
                .attr("class", "tools-note"),
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
            el("p", note.clone())
                .attr("class", "tools-note")
                .attr("role", "status"),
        ),
        None => Box::new(el("span", "").attr("hidden", "")),
    }
}

/// The "Graph tools" region's arrangement and physics section.
pub(super) fn section(page: &TreePage) -> Child {
    let profiles: Vec<&'static str> = std::iter::once(CUSTOM_LABEL)
        .chain(CANVAS_PHYSICS_PROFILES.iter().map(|profile| profile.label))
        .collect();
    let section: Vec<Child> = vec![
        picker(
            "Arrangement",
            arrangement_choices().map(|(_, label)| label).collect(),
            |page| &mut page.physics.arrangement,
        ),
        apply("Apply arrangement", TreePage::apply_arrangement),
        picker(
            "Role",
            Role::ALL.iter().map(|role| role.label()).collect(),
            |page| &mut page.physics.role,
        ),
        apply("Apply role", TreePage::apply_role),
        picker("Physics law", labels(CANVAS_PHYSICS_LAWS), |page| {
            &mut page.physics.law
        }),
        overlay_group(page),
        picker("Kinds", labels(CANVAS_PHYSICS_KIND_SOURCES), |page| {
            &mut page.physics.kind
        }),
        picker("Mass", labels(CANVAS_PHYSICS_MASS_SOURCES), |page| {
            &mut page.physics.mass
        }),
        picker("Depth", labels(CANVAS_PHYSICS_DEPTH_SOURCES), |page| {
            &mut page.physics.depth
        }),
        apply("Apply physics", TreePage::apply_physics),
        picker(
            "Speed",
            crate::web_speed::PRESETS
                .iter()
                .map(|(_, label)| *label)
                .collect(),
            |page| &mut page.physics.speed,
        ),
        speed_note(page),
        picker("Profile", profiles, |page| &mut page.physics.profile),
        apply("Apply profile", TreePage::apply_profile),
        Box::new(
            el("p", page.physics.status.clone())
                .attr("class", "tools-status")
                .attr("role", "status"),
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
