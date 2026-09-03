// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Arrange pane: the graph's arrangement and its physics, as controls.
//!
//! The native counterpart of the web host's Find-and-arrange section: one
//! choice row for the arrangement (the canvas's analytic strategies, or
//! Free — physics alone), one for the physics **law**, a toggle per
//! **overlay**, a choice for each attribute **source** (what Kinds reads a
//! kind from, what Orbit and the hub overlays weigh by, what Depth reads
//! depth from), and a choice for the named **profile**. Every row is a
//! shared `cambium::setting_row` over a spec built from the canvas
//! catalogs, so the plain names live in one place and a catalog change
//! reaches this pane without an edit here.
//!
//! An applied value leaves the pane as an [`ArrangeIntent`], which the shell
//! lowers to the same [`Action`] the command palette's `Physics:` /
//! `Overlay:` / `Profile:` rows fire — the pane and the palette are two
//! doors onto one spine, which is what lets a self-drive receipt act by
//! label and read the pane's rows back. The pane holds no truth of its own:
//! [`ArrangePane::sync`] mirrors the active canvas each frame.

use std::cell::RefCell;
use std::rc::Rc;

use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, PointerClick, el, setting_row,
};
use genet_host_api::settings::{
    SettingControl, SettingMovement, SettingMutability, SettingOption, SettingScope,
    SettingSecurity, SettingSpec, SettingValue,
};
use genet_scripted_dom::ScriptedDom;
use mere::canvas::{
    CANVAS_LAYOUT_STRATEGIES, CANVAS_PHYSICS_DEPTH_SOURCES, CANVAS_PHYSICS_KIND_SOURCES,
    CANVAS_PHYSICS_LAWS, CANVAS_PHYSICS_MASS_SOURCES, CANVAS_PHYSICS_OVERLAYS,
    CANVAS_PHYSICS_PROFILES, PhysicsDepthSource, PhysicsKindSource, PhysicsLaw, PhysicsMassSource,
    PhysicsOverlay,
};

use crate::action::Action;
use crate::app::App;

/// The arrangement choice's id for "no analytic arrangement": physics alone.
pub const FREE_LAYOUT_ID: &str = "free";
/// The profile choice's value when no profile names the live pair.
const CUSTOM_PROFILE_ID: &str = "custom";

/// What an Arrange interaction produces. Ids, never labels: the shell lowers
/// each to an [`Action`] through [`ArrangeIntent::into_action`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArrangeIntent {
    /// An arrangement id from the canvas catalog, or [`FREE_LAYOUT_ID`].
    SetLayout(String),
    SetLaw(String),
    SetOverlay {
        id: String,
        on: bool,
    },
    SetKindSource(String),
    SetMassSource(String),
    SetDepthSource(String),
    ApplyProfile(String),
}

/// The cambium event views return intents through `OptionalAction`, which
/// asks for this marker.
impl cambium::Action for ArrangeIntent {}

impl ArrangeIntent {
    /// The palette-equivalent action, or `None` for an id the catalogs do
    /// not know (a stale row, a typo in a script).
    pub fn into_action(self) -> Option<Action> {
        match self {
            ArrangeIntent::SetLayout(id) if id == FREE_LAYOUT_ID => {
                Some(Action::SetLayoutStrategy(None))
            }
            ArrangeIntent::SetLayout(id) => CANVAS_LAYOUT_STRATEGIES
                .iter()
                .find(|(known, _)| *known == id)
                .map(|(known, _)| Action::SetLayoutStrategy(Some(known))),
            ArrangeIntent::SetLaw(id) => {
                PhysicsLaw::parse(&id).map(|law| Action::SetPhysicsLaw(law.id()))
            }
            ArrangeIntent::SetOverlay { id, on } => PhysicsOverlay::parse(&id)
                .map(|overlay| Action::SetPhysicsOverlay(overlay.id(), on)),
            ArrangeIntent::SetKindSource(id) => PhysicsKindSource::parse(&id)
                .map(|source| Action::SetPhysicsKindSource(source.id())),
            ArrangeIntent::SetMassSource(id) => PhysicsMassSource::parse(&id)
                .map(|source| Action::SetPhysicsMassSource(source.id())),
            ArrangeIntent::SetDepthSource(id) => PhysicsDepthSource::parse(&id)
                .map(|source| Action::SetPhysicsDepthSource(source.id())),
            ArrangeIntent::ApplyProfile(id) => CANVAS_PHYSICS_PROFILES
                .iter()
                .find(|profile| profile.id == id)
                .map(|profile| Action::ApplyPhysicsProfile(profile.id)),
        }
    }
}

/// The mirrored canvas choice, refreshed by [`ArrangePane::sync`].
#[derive(Clone, Debug, PartialEq, Eq)]
struct Choice {
    layout: String,
    law: String,
    overlays: Vec<String>,
    kind: String,
    mass: String,
    depth: String,
    profile: String,
}

impl Choice {
    fn of(app: &App) -> Self {
        let canvas = &app.graph_runtimes;
        Self {
            layout: canvas
                .layout_strategy()
                .map(str::to_string)
                .unwrap_or_else(|| FREE_LAYOUT_ID.to_string()),
            law: canvas.physics_law().id().to_string(),
            overlays: canvas
                .physics_overlays()
                .iter()
                .map(|overlay| overlay.id().to_string())
                .collect(),
            kind: canvas.physics_kind_source().id().to_string(),
            mass: canvas.physics_mass_source().id().to_string(),
            depth: canvas.physics_depth_source().id().to_string(),
            profile: canvas
                .physics_profile_id()
                .unwrap_or(CUSTOM_PROFILE_ID)
                .to_string(),
        }
    }
}

/// The pane's rows as plain text, for the observe snapshot and `assert row`:
/// what a person reads off the pane, without rendering it.
pub fn arrange_rows(app: &App) -> Vec<String> {
    let choice = Choice::of(app);
    let label = |catalog: &[(&str, &str)], id: &str| -> String {
        catalog
            .iter()
            .find(|(known, _)| *known == id)
            .map(|(_, label)| (*label).to_string())
            .unwrap_or_else(|| id.to_string())
    };
    let layout = if choice.layout == FREE_LAYOUT_ID {
        "Free".to_string()
    } else {
        label(CANVAS_LAYOUT_STRATEGIES, &choice.layout)
    };
    let overlays = if choice.overlays.is_empty() {
        "none".to_string()
    } else {
        choice
            .overlays
            .iter()
            .map(|id| label(CANVAS_PHYSICS_OVERLAYS, id))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let profile = CANVAS_PHYSICS_PROFILES
        .iter()
        .find(|profile| profile.id == choice.profile)
        .map(|profile| profile.label.to_string())
        .unwrap_or_else(|| CUSTOM_PROFILE_ID.to_string());
    vec![
        format!("Arrangement: {layout}"),
        format!("Physics: {}", label(CANVAS_PHYSICS_LAWS, &choice.law)),
        format!("Overlays: {overlays}"),
        format!(
            "Kinds: {}",
            label(CANVAS_PHYSICS_KIND_SOURCES, &choice.kind)
        ),
        format!("Mass: {}", label(CANVAS_PHYSICS_MASS_SOURCES, &choice.mass)),
        format!(
            "Depth: {}",
            label(CANVAS_PHYSICS_DEPTH_SOURCES, &choice.depth)
        ),
        format!("Profile: {profile}"),
    ]
}

struct ArrangeState {
    choice: Choice,
    viewport_w: f32,
    viewport_h: f32,
}

type ArrangeView = Box<dyn AnyView<ArrangeState, ArrangeIntent, GenetCtx, GenetElement>>;
type ArrangeRunner =
    GenetAppRunner<ArrangeState, fn(&ArrangeState) -> ArrangeView, ArrangeView, ArrangeIntent>;

fn spec(id: &str, label: &str, control: SettingControl, value: SettingValue) -> SettingSpec {
    SettingSpec {
        id: id.to_string(),
        label: label.to_string(),
        scope: SettingScope::SessionDataspace,
        movement: SettingMovement::LocalOnly,
        mutability: SettingMutability::Live,
        security: SettingSecurity::Ordinary,
        control,
        value,
    }
}

fn choice_control(options: impl IntoIterator<Item = (String, String)>) -> SettingControl {
    SettingControl::Choice {
        options: options
            .into_iter()
            .map(|(value, label)| SettingOption { value, label })
            .collect(),
    }
}

fn catalog_options(catalog: &[(&str, &str)]) -> Vec<(String, String)> {
    catalog
        .iter()
        .map(|(id, label)| ((*id).to_string(), (*label).to_string()))
        .collect()
}

fn choice_row(
    id: &str,
    label: &str,
    options: Vec<(String, String)>,
    current: &str,
    intent: fn(String) -> ArrangeIntent,
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
        move |_state: &mut ArrangeState, value: SettingValue| match value {
            SettingValue::Text(id) => Some(intent(id)),
            _ => None,
        },
    ))
}

fn overlay_row(overlay: PhysicsOverlay, on: bool) -> ArrangeView {
    let spec = spec(
        &format!("overlay.{}", overlay.id()),
        overlay.label(),
        SettingControl::Toggle,
        SettingValue::Boolean(on),
    );
    Box::new(setting_row(
        &spec,
        overlay.label(),
        move |_state: &mut ArrangeState, value: SettingValue| match value {
            SettingValue::Boolean(on) => Some(ArrangeIntent::SetOverlay {
                id: overlay.id().to_string(),
                on,
            }),
            _ => None,
        },
    ))
}

fn section(title: &str) -> ArrangeView {
    Box::new(
        el::<_, ArrangeState, ArrangeIntent>("div", title.to_string())
            .attr("class", "list-section-title"),
    )
}

fn arrange_view(state: &ArrangeState) -> ArrangeView {
    let choice = &state.choice;
    let mut layouts = vec![(
        FREE_LAYOUT_ID.to_string(),
        "Free (physics alone)".to_string(),
    )];
    layouts.extend(catalog_options(CANVAS_LAYOUT_STRATEGIES));
    let mut profiles = vec![(CUSTOM_PROFILE_ID.to_string(), "Custom".to_string())];
    profiles.extend(
        CANVAS_PHYSICS_PROFILES
            .iter()
            .map(|profile| (profile.id.to_string(), profile.label.to_string())),
    );
    let overlay_rows = PhysicsOverlay::ALL
        .iter()
        .map(|overlay| {
            overlay_row(
                *overlay,
                choice.overlays.iter().any(|id| id == overlay.id()),
            )
        })
        .collect::<Vec<_>>();
    Box::new(
        el::<_, ArrangeState, ArrangeIntent>(
            "div",
            (
                section("Arrangement"),
                choice_row(
                    "arrangement",
                    "Arrangement",
                    layouts,
                    &choice.layout,
                    ArrangeIntent::SetLayout,
                ),
                section("Physics"),
                choice_row(
                    "physics.law",
                    "Law",
                    catalog_options(CANVAS_PHYSICS_LAWS),
                    &choice.law,
                    ArrangeIntent::SetLaw,
                ),
                choice_row(
                    "physics.profile",
                    "Profile",
                    profiles,
                    &choice.profile,
                    ArrangeIntent::ApplyProfile,
                ),
                section("Overlays"),
                overlay_rows,
                section("Sources"),
                choice_row(
                    "physics.kind-source",
                    "Kinds",
                    catalog_options(CANVAS_PHYSICS_KIND_SOURCES),
                    &choice.kind,
                    ArrangeIntent::SetKindSource,
                ),
                choice_row(
                    "physics.mass-source",
                    "Mass",
                    catalog_options(CANVAS_PHYSICS_MASS_SOURCES),
                    &choice.mass,
                    ArrangeIntent::SetMassSource,
                ),
                choice_row(
                    "physics.depth-source",
                    "Depth",
                    catalog_options(CANVAS_PHYSICS_DEPTH_SOURCES),
                    &choice.depth,
                    ArrangeIntent::SetDepthSource,
                ),
            ),
        )
        .attr("class", "pane")
        .attr(
            "style",
            format!(
                "width: {}px; height: {}px;",
                state.viewport_w, state.viewport_h
            ),
        ),
    )
}

/// The Arrange pane: a retained cambium runner over the active canvas's
/// arrangement and physics choice. Held by the shell like the other panes.
pub struct ArrangePane {
    dom: DomHandle,
    runner: ArrangeRunner,
    scroll: crate::ui::PaneScroll,
    layout: crate::ui::RetainedLayout,
}

impl Default for ArrangePane {
    fn default() -> Self {
        Self::new()
    }
}

impl ArrangePane {
    pub fn new() -> Self {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = ArrangeState {
            choice: Choice {
                layout: FREE_LAYOUT_ID.to_string(),
                law: PhysicsLaw::Springs.id().to_string(),
                overlays: Vec::new(),
                kind: PhysicsKindSource::Site.id().to_string(),
                mass: PhysicsMassSource::Degree.id().to_string(),
                depth: PhysicsDepthSource::Roots.id().to_string(),
                profile: CUSTOM_PROFILE_ID.to_string(),
            },
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        let runner = ArrangeRunner::new(
            dom.clone(),
            arrange_view as fn(&ArrangeState) -> ArrangeView,
            state,
        );
        Self {
            dom,
            runner,
            scroll: crate::ui::PaneScroll::new(),
            layout: crate::ui::RetainedLayout::new(),
        }
    }

    /// Mirror the active canvas's choice at the pane's size.
    pub fn sync(&mut self, app: &App, pane_w: f32, pane_h: f32) {
        let choice = Choice::of(app);
        self.runner.update(|state| {
            state.choice = choice;
            state.viewport_w = pane_w;
            state.viewport_h = pane_h;
        });
    }

    /// The pane's scene at its size, under the host's cambium sheet.
    pub fn scene(&mut self, w: u32, h: u32) -> netrender::Scene {
        self.layout.scene_scrolled(
            &mut self.dom.borrow_mut(),
            crate::ui::CAMBIUM_SHEET,
            w,
            h,
            &mut self.scroll,
        )
    }

    /// Wheel delta from the shell.
    pub fn scroll_by(&mut self, dx: f32, dy: f32) {
        self.scroll.nudge(dx, dy);
    }

    /// Whether the overlay bars still need repainting as they fade.
    pub fn bars_visible(&mut self) -> bool {
        self.scroll.bars_visible()
    }

    /// The retained DOM, for the shared probe harness.
    pub fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }

    /// Route a click at pane-local `(x, y)` into the rows; the applied values
    /// come back as the intents the shell lowers.
    pub fn click(&mut self, x: f32, y: f32, w: u32, h: u32) -> Vec<ArrangeIntent> {
        let hit = self.layout.hit_test_scrolled(
            &mut self.dom.borrow_mut(),
            crate::ui::CAMBIUM_SHEET,
            w,
            h,
            x,
            y,
            &self.scroll,
        );
        match hit {
            Some(node) => self.runner.dispatch_click(node, PointerClick::at((x, y))),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intents_lower_to_the_palette_actions_by_id() {
        assert_eq!(
            ArrangeIntent::SetLayout("free".into()).into_action(),
            Some(Action::SetLayoutStrategy(None))
        );
        assert_eq!(
            ArrangeIntent::SetLayout("grid.default".into()).into_action(),
            Some(Action::SetLayoutStrategy(Some("grid.default")))
        );
        assert_eq!(
            ArrangeIntent::SetLaw("orbit.gravity".into()).into_action(),
            Some(Action::SetPhysicsLaw("orbit.gravity"))
        );
        assert_eq!(
            ArrangeIntent::SetOverlay {
                id: "skeleton".into(),
                on: true
            }
            .into_action(),
            Some(Action::SetPhysicsOverlay("skeleton", true))
        );
        assert_eq!(
            ArrangeIntent::ApplyProfile("crystal".into()).into_action(),
            Some(Action::ApplyPhysicsProfile("crystal"))
        );
        assert_eq!(ArrangeIntent::SetLaw("plasma".into()).into_action(), None);
    }

    #[test]
    fn the_rows_read_the_canvas_back() {
        let mut app = App::test_stub();
        app.update(Action::SetPhysicsLaw("orbit.gravity"));
        app.update(Action::SetPhysicsOverlay("skeleton", true));
        let rows = arrange_rows(&app);
        assert!(rows.contains(&"Physics: Orbit".to_string()), "{rows:?}");
        assert!(rows.contains(&"Overlays: Skeleton".to_string()), "{rows:?}");
        assert!(rows.contains(&"Profile: custom".to_string()), "{rows:?}");
        app.update(Action::ApplyPhysicsProfile("crystal"));
        let rows = arrange_rows(&app);
        assert!(rows.contains(&"Physics: Stress".to_string()), "{rows:?}");
        assert!(rows.contains(&"Overlays: Grid".to_string()), "{rows:?}");
        assert!(rows.contains(&"Profile: Crystal".to_string()), "{rows:?}");
    }
}
