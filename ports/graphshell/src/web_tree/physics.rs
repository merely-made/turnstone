// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The "Graph tools" side region's first section: arrangement and physics.
//!
//! Cambium controls hold the form; Apply reads them into the typed inputs of
//! `graphshell::canvas_physics`, which the old page's form also calls.
use super::*;
use cambium::{SelectState, button, checkbox, disclosure, lens, select};
use graphshell::canvas_physics::{
    self, ArrangementTransition, CUSTOM_PROFILE, arrangement_choices,
};
use mere::canvas::{
    CANVAS_PHYSICS_DEPTH_SOURCES, CANVAS_PHYSICS_KIND_SOURCES, CANVAS_PHYSICS_LAWS,
    CANVAS_PHYSICS_MASS_SOURCES, CANVAS_PHYSICS_OVERLAYS, CANVAS_PHYSICS_PROFILES, PhysicsChoice,
    PhysicsDepthSource, PhysicsKindSource, PhysicsLaw, PhysicsMassSource, PhysicsOverlay,
};

/// The profile picker's first entry, read when the live pair names no profile.
const CUSTOM_LABEL: &str = "Custom (no profile)";

/// The panel's control state and the arrangement it last applied.
pub(super) struct PhysicsPanel {
    pub(super) arrangement: SelectState,
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
            law: SelectState::new(0).with_label("Physics law"),
            kind: SelectState::new(0).with_label("Kinds"),
            mass: SelectState::new(0).with_label("Mass"),
            depth: SelectState::new(0).with_label("Depth"),
            profile: SelectState::new(0).with_label("Profile"),
            overlays: vec![false; CANVAS_PHYSICS_OVERLAYS.len()],
            layout_id: layout_id.to_string(),
            status: String::new(),
            transition: None,
        };
        panel.sync(canvas);
        panel
    }

    /// Set every physics control to the canvas's live choice, and the profile
    /// to the one naming the pair, or custom.
    pub(super) fn sync(&mut self, canvas: &Canvas) {
        let live = canvas.physics_choice();
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

    /// The choice the controls hold.
    pub(super) fn choice(&self) -> PhysicsChoice {
        PhysicsChoice {
            law: PhysicsLaw::ALL[self.law.selected.min(PhysicsLaw::ALL.len() - 1)],
            overlays: canvas_physics::ticked_overlays(|overlay| {
                PhysicsOverlay::ALL
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

    fn apply_physics(&mut self) {
        let choice = self.physics.choice();
        let mut canvas = self.shared.canvas.borrow_mut();
        self.physics.status = canvas_physics::apply_physics(&mut canvas, &choice);
        self.physics.sync(&canvas);
        self.shared.dirty.set(true);
    }

    fn apply_profile(&mut self) {
        let id = self.physics.profile_id();
        let mut canvas = self.shared.canvas.borrow_mut();
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

/// The "Graph tools" region's arrangement and physics section.
pub(super) fn section(page: &TreePage) -> Child {
    let overlays: Vec<Child> = CANVAS_PHYSICS_OVERLAYS
        .iter()
        .enumerate()
        .map(|(index, (_, label))| {
            Box::new(el(
                "label",
                (
                    lens(
                        move |checked: &mut bool| checkbox(*checked).attr("aria-label", *label),
                        move |page: &mut TreePage| &mut page.physics.overlays[index],
                    ),
                    el("span", *label),
                ),
            )) as Child
        })
        .collect();
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
        picker("Physics law", labels(CANVAS_PHYSICS_LAWS), |page| {
            &mut page.physics.law
        }),
        Box::new(
            el("div", overlays)
                .attr("class", "tools-overlays")
                .attr("role", "group")
                .attr("aria-label", "Overlays"),
        ),
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
