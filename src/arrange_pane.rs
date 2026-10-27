// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Arrange pane: the graph's arrangement and its physics, as controls.
//!
//! The native counterpart of the web host's arrangement-and-physics section,
//! which calls the same `mere::canvas::physics_controls` actions and labels
//! its canvas verbs with `CanvasCommand::label`: one
//! choice row for the arrangement (the canvas's analytic strategies, or
//! Free — physics alone), one for the physics **law**, a toggle per
//! **overlay**, a choice for each attribute **source** (what Kinds reads a
//! kind from, what Orbit and the hub overlays weigh by, what Depth reads
//! depth from), and a choice for the named **profile**. The composing
//! sections (role, two-law mix, grouping, two-stage schedule, speed) live in
//! `compose`. Every row is a
//! shared `cambium::setting_row` over a spec built from the canvas
//! catalogs, so the plain names live in one place and a catalog change
//! reaches this pane without an edit here.
//!
//! An applied value leaves the pane as an [`ArrangeIntent`], which the shell
//! lowers to the same [`Action`] the command palette's `Physics:` /
//! `Overlay:` / `Profile:` rows fire — the pane and the palette are two
//! doors onto one spine, which is what lets a self-drive receipt act by
//! label and read the pane's rows back. The pane holds no truth of its own
//! beyond the composing forms' unapplied drafts: [`ArrangePane::sync`]
//! mirrors the active canvas each frame.

mod compose;

use std::cell::RefCell;
use std::rc::Rc;

use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, PointerClick, button, el,
    setting_row,
};
use genet_scripted_dom::ScriptedDom;
use layout_dom_api::LayoutDom;
use mere::canvas::controls::CanvasCommand;
use mere::canvas::physics_controls::{SPEED_PRESETS, TwoLawGrouping, TwoLawMix, TwoStageSchedule};
use mere::canvas::physics_form::{Control, FieldId, PhysicsForm, Readback};
use mere::canvas::{CANVAS_LAYOUT_STRATEGIES, PhysicsOverlay};
use mere_surface_api::settings::{
    SettingControl, SettingMovement, SettingMutability, SettingOption, SettingScope,
    SettingSecurity, SettingSpec, SettingValue,
};

use crate::action::Action;
use crate::app::App;

/// The arrangement choice's id for "no analytic arrangement": physics alone.
pub const FREE_LAYOUT_ID: &str = "free";

/// The canvas verbs the pane offers as buttons, in the order it shows them.
const VERBS: [CanvasCommand; 8] = [
    CanvasCommand::ResetToSeed,
    CanvasCommand::SetSeed,
    CanvasCommand::RestoreRecipeSeed,
    CanvasCommand::ToggleForegroundPin,
    CanvasCommand::DismissRetained,
    CanvasCommand::GrowTiles,
    CanvasCommand::ShrinkTiles,
    CanvasCommand::ToggleTerritory,
];

/// What an Arrange interaction produces. Ids, never labels: the shell lowers
/// each to an [`Action`] through [`ArrangeIntent::into_action`].
#[derive(Clone, Debug, PartialEq)]
pub enum ArrangeIntent {
    /// An arrangement id from the canvas catalog, or [`FREE_LAYOUT_ID`].
    SetLayout(String),
    /// One of [`VERBS`].
    Verb(CanvasCommand),
    SetRole(String),
    SetRoleGroups(String),
    SetSpeed(String),
    ApplyMix(TwoLawMix),
    ApplyGrouping(TwoLawGrouping),
    ApplySchedule(TwoStageSchedule),
    UseSingleLaw,
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
            Self::Verb(verb) => Action::from_canvas_verb(verb),
            Self::SetRole(id) => field_option(FieldId::Role, &id).map(Action::SetPhysicsRole),
            Self::SetRoleGroups(id) => {
                field_option(FieldId::RoleGroups, &id).map(Action::SetPhysicsRoleGroups)
            },
            Self::SetSpeed(id) => field_option(FieldId::Speed, &id).map(Action::SetPhysicsSpeed),
            Self::ApplyMix(mix) => Some(Action::ApplyPhysicsMix(mix)),
            Self::ApplyGrouping(grouping) => Some(Action::ApplyPhysicsGrouping(grouping)),
            Self::ApplySchedule(schedule) => Some(Action::ApplyPhysicsSchedule(schedule)),
            Self::UseSingleLaw => Some(Action::UseSingleLaw),
            ArrangeIntent::SetLayout(id) if id == FREE_LAYOUT_ID => {
                Some(Action::SetLayoutStrategy(None))
            },
            ArrangeIntent::SetLayout(id) => CANVAS_LAYOUT_STRATEGIES
                .iter()
                .find(|(known, _)| *known == id)
                .map(|(known, _)| Action::SetLayoutStrategy(Some(known))),
            ArrangeIntent::SetLaw(id) => {
                field_option(FieldId::PhysicsLaw, &id).map(Action::SetPhysicsLaw)
            },
            ArrangeIntent::SetOverlay { id, on } => PhysicsOverlay::ALL
                .iter()
                .find(|overlay| overlay.id() == id)
                .map(|overlay| Action::SetPhysicsOverlay(overlay.id(), on)),
            ArrangeIntent::SetKindSource(id) => {
                field_option(FieldId::PhysicsKind, &id).map(Action::SetPhysicsKindSource)
            },
            ArrangeIntent::SetMassSource(id) => {
                field_option(FieldId::PhysicsMass, &id).map(Action::SetPhysicsMassSource)
            },
            ArrangeIntent::SetDepthSource(id) => {
                field_option(FieldId::PhysicsDepth, &id).map(Action::SetPhysicsDepthSource)
            },
            ArrangeIntent::ApplyProfile(id) => field_option(FieldId::PhysicsProfile, &id)
                .filter(|profile| *profile != "custom")
                .map(Action::ApplyPhysicsProfile),
        }
    }
}

fn field_option(field_id: FieldId, id: &str) -> Option<&'static str> {
    let Control::Choice { options, .. } = PhysicsForm::default().field(field_id).control else {
        return None;
    };
    options
        .into_iter()
        .map(|(known, _)| known)
        .find(|known| *known == id)
}

fn field_label(field_id: FieldId, id: &str) -> String {
    if field_id == FieldId::PhysicsProfile && id == "custom" {
        return "custom".to_string();
    }
    match PhysicsForm::default().field(field_id).control {
        Control::Choice { options, .. } => options
            .into_iter()
            .find(|(known, _)| *known == id)
            .map(|(_, label)| label.to_string())
            .unwrap_or_else(|| id.to_string()),
        _ => id.to_string(),
    }
}

/// The mirrored canvas choice, refreshed by [`ArrangePane::sync`].
#[derive(Clone, Debug, PartialEq)]
struct Choice {
    layout: String,
    /// What the canvas authored for the composing fields: the roles, the
    /// speed and the mix, grouping and schedule.
    readback: Readback,
    status: Option<String>,
    refusal: Option<String>,
    saved_seed: bool,
    regions: usize,
}

impl Choice {
    fn of(app: &App) -> Self {
        Self::for_graph(
            app,
            app.scene_command_pane()
                .and_then(|pane| app.graph_for_pane(pane)),
        )
    }

    fn for_graph(app: &App, graph: Option<crate::panes::GraphId>) -> Self {
        let canvas = graph
            .and_then(|graph| app.graph_runtimes.canvas(graph))
            .unwrap_or(&app.graph_runtimes);
        let readback = Readback::of(canvas);
        Self {
            layout: canvas
                .layout_strategy()
                .map(str::to_string)
                .unwrap_or_else(|| FREE_LAYOUT_ID.to_string()),
            readback,
            status: graph
                .and_then(|graph| app.graph_runtimes.get(graph))
                .and_then(|runtime| runtime.physics_status.clone()),
            saved_seed: canvas.has_saved_seed(),
            regions: canvas.strategy_regions().len(),
            refusal: app.physics_refusal().map(ToString::to_string),
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
    let physics = &choice.readback.physics;
    let overlays = if physics.overlays.is_empty() {
        "none".to_string()
    } else {
        physics
            .overlays
            .iter()
            .map(|overlay| overlay.label().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut rows = vec![
        format!("Arrangement: {layout}"),
        format!(
            "Seed: {}",
            if choice.saved_seed { "saved" } else { "recipe" }
        ),
        format!("Regions: {}", choice.regions),
        format!(
            "Physics: {}",
            field_label(FieldId::PhysicsLaw, physics.law.id())
        ),
        format!("Overlays: {overlays}"),
        format!(
            "Kinds: {}",
            field_label(FieldId::PhysicsKind, physics.kind.id())
        ),
        format!(
            "Mass: {}",
            field_label(FieldId::PhysicsMass, physics.mass.id())
        ),
        format!(
            "Depth: {}",
            field_label(FieldId::PhysicsDepth, physics.depth.id())
        ),
        format!(
            "Profile: {}",
            field_label(FieldId::PhysicsProfile, choice.readback.physics_profile)
        ),
        format!("Role: {}", choice.readback.role.label()),
        format!("Role groups: {}", choice.readback.role_groups.label()),
        format!("Speed: {}", SPEED_PRESETS[choice.readback.speed].1),
    ];
    rows.extend(compose::authored_rows(&choice.readback));
    if let Some(status) = choice.status {
        rows.push(format!("Physics status: {status}"));
    }
    if let Some(refusal) = choice.refusal {
        rows.push(format!("Physics refused: {refusal}"));
    }
    rows
}

struct ArrangeState {
    choice: Choice,
    /// The composing fields: the model, with the values the pane holds
    /// between presses.
    form: PhysicsForm,
    /// Why the last press made no request, until the next one does.
    form_note: String,
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

fn section(title: &str) -> ArrangeView {
    Box::new(
        el::<_, ArrangeState, ArrangeIntent>("div", title.to_string())
            .attr("class", "list-section-title"),
    )
}

fn scene_button(verb: CanvasCommand) -> ArrangeView {
    apply_button(verb.label(), move |_| Some(ArrangeIntent::Verb(verb)))
}

/// A button whose press turns the pane's state into an intent.
fn apply_button(
    label: &'static str,
    intent: impl Fn(&mut ArrangeState) -> Option<ArrangeIntent> + 'static,
) -> ArrangeView {
    Box::new(
        button(label, move |state: &mut ArrangeState, _: PointerClick| {
            intent(state)
        })
        .attr("class", "setting-apply"),
    )
}

fn arrange_view(state: &ArrangeState) -> ArrangeView {
    let choice = &state.choice;
    let mut layouts = vec![(
        FREE_LAYOUT_ID.to_string(),
        "Free (physics alone)".to_string(),
    )];
    layouts.extend(catalog_options(CANVAS_LAYOUT_STRATEGIES));
    let profile_field = state.form.field(FieldId::PhysicsProfile);
    let Control::Choice {
        options,
        current: profile,
    } = profile_field.control
    else {
        unreachable!("PhysicsProfile is a choice")
    };
    let profiles = options
        .into_iter()
        .map(|(id, label)| {
            (
                id.to_string(),
                if id == "custom" {
                    "Custom".to_string()
                } else {
                    label.to_string()
                },
            )
        })
        .collect();
    let overlay_rows = PhysicsOverlay::ALL
        .iter()
        .map(|overlay| {
            let overlay = *overlay;
            compose::live_toggle_with_key(
                state.form.field(FieldId::PhysicsOverlay(overlay)),
                &format!("overlay.{}", overlay.id()),
                move |on| ArrangeIntent::SetOverlay {
                    id: overlay.id().to_string(),
                    on,
                },
            )
        })
        .collect::<Vec<_>>();
    Box::new(
        el::<_, ArrangeState, ArrangeIntent>(
            "div",
            (
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
                        el::<_, ArrangeState, ArrangeIntent>(
                            "div",
                            format!(
                                "{} seed · {} regions",
                                if choice.saved_seed { "Saved" } else { "Recipe" },
                                choice.regions
                            ),
                        )
                        .attr("role", "status"),
                        VERBS.into_iter().map(scene_button).collect::<Vec<_>>(),
                    ),
                ),
                section("Physics"),
                el::<_, ArrangeState, ArrangeIntent>(
                    "div",
                    choice.refusal.clone().unwrap_or_default(),
                )
                .attr("role", "status")
                .attr("aria-live", "polite"),
                compose::live_choice_with_key(
                    state.form.field(FieldId::PhysicsLaw),
                    "physics.law",
                    ArrangeIntent::SetLaw,
                ),
                choice_row(
                    "physics.profile",
                    "Profile",
                    profiles,
                    profile,
                    ArrangeIntent::ApplyProfile,
                ),
                section("Overlays"),
                overlay_rows,
                section("Sources"),
                compose::live_choice_with_key(
                    state.form.field(FieldId::PhysicsKind),
                    "physics.kind-source",
                    ArrangeIntent::SetKindSource,
                ),
                compose::live_choice_with_key(
                    state.form.field(FieldId::PhysicsMass),
                    "physics.mass-source",
                    ArrangeIntent::SetMassSource,
                ),
                compose::live_choice_with_key(
                    state.form.field(FieldId::PhysicsDepth),
                    "physics.depth-source",
                    ArrangeIntent::SetDepthSource,
                ),
                compose::views(state),
                el::<_, ArrangeState, ArrangeIntent>(
                    "div",
                    choice.status.clone().unwrap_or_default(),
                )
                .attr("role", "status")
                .attr("aria-live", "polite")
                .attr("aria-label", "Physics status"),
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

/// The current app-owned refusal, independent of the retained DOM's last sync.
/// This read-only status uses pane bounds; Arrange controls remain unqualified.
pub(crate) fn physics_refusal_tree(
    app: &App,
    pane: crate::panes::PaneId,
    placement: crate::surface::Rect,
) -> Option<uxtree::UxTree> {
    let refusal = app.physics_refusal()?;
    let id = uxtree::node_id_for_path(&format!("turnstone/arrange/{}/physics-refusal", pane.0));
    let mut node = accesskit::Node::new(accesskit::Role::Status);
    node.set_label(format!("Physics refused: {refusal}"));
    node.set_live(accesskit::Live::Polite);
    node.set_bounds(accesskit::Rect::new(
        f64::from(placement.x),
        f64::from(placement.y),
        f64::from(placement.x + placement.w),
        f64::from(placement.y + placement.h),
    ));
    Some(uxtree::UxTree {
        root: id,
        nodes: vec![(id, node)],
    })
}

/// The Arrange pane: a retained cambium runner over the active canvas's
/// arrangement and physics choice. Held by the shell like the other panes.
pub struct ArrangePane {
    dom: DomHandle,
    runner: ArrangeRunner,
    scroll: crate::ui::PaneScroll,
    layout: crate::ui::RetainedLayout,
    // Drop the runner and its widget resources before final mount custody.
    mount: Option<cambium::RetainedMount>,
}

impl Default for ArrangePane {
    fn default() -> Self {
        Self::new()
    }
}

impl ArrangePane {
    pub fn new() -> Self {
        Self::new_in(None)
    }

    pub(crate) fn new_in(mount: Option<cambium::RetainedMount>) -> Self {
        let dom: DomHandle = mount
            .as_ref()
            .map(cambium::RetainedMount::dom)
            .unwrap_or_else(|| Rc::new(RefCell::new(ScriptedDom::new())));
        let state = ArrangeState {
            choice: Choice {
                layout: FREE_LAYOUT_ID.to_string(),
                readback: Readback::default(),
                status: None,
                refusal: None,
                saved_seed: false,
                regions: 0,
            },
            form: PhysicsForm::default(),
            form_note: String::new(),
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        let runner = ArrangeRunner::new_at(
            dom.clone(),
            mount
                .as_ref()
                .map_or_else(|| dom.borrow().document(), cambium::RetainedMount::node),
            arrange_view as fn(&ArrangeState) -> ArrangeView,
            state,
        );
        Self {
            mount: mount.clone(),
            dom,
            runner,
            scroll: crate::ui::PaneScroll::new(),
            layout: crate::ui::RetainedLayout::new_at(
                mount.as_ref().map(cambium::RetainedMount::node),
            ),
        }
    }

    pub(crate) fn inline_focus(&self) -> Option<genet_scripted_dom::NodeId> {
        self.runner.focus()
    }
    pub(crate) fn focus_inline(&mut self, node: genet_scripted_dom::NodeId) -> bool {
        let Some(mount) = &self.mount else {
            return false;
        };
        if !cambium_rootstock::WindowDom::new(&self.dom.borrow(), mount.node()).contains(node) {
            return false;
        }
        self.runner.set_focus(Some(node));
        true
    }
    pub(crate) fn inline_selector(
        &self,
        selector: &taproot::Selector,
    ) -> Result<Option<(f32, f32)>, &'static str> {
        self.layout.selector_point(&self.dom.borrow(), selector)
    }
    pub(crate) fn key_inline(&mut self, event: cambium::KeyEvent) -> Vec<ArrangeIntent> {
        self.runner.dispatch_key(event)
    }
    pub(crate) fn blur_inline(&mut self) {
        self.runner.set_focus(None);
    }
    pub(crate) fn inline_accepts_text(&self) -> bool {
        self.layout
            .editable_focus(&self.dom.borrow(), self.runner.focus())
    }
    pub(crate) fn inline_cursor_area(&self) -> Option<(f32, f32, f32, f32)> {
        self.layout
            .cursor_area(&self.dom.borrow(), self.runner.focus())
    }
    pub(crate) fn inline_mount(&self) -> Option<cambium::RetainedMount> {
        self.mount.clone()
    }
    pub(crate) fn unbind_inline(&mut self) {
        self.layout.unbind_inline();
    }
    pub(crate) fn bind_inline(&mut self, layout: &crate::ui::InlineLayoutHandle) {
        if let Some(mount) = &self.mount {
            self.layout.bind_inline(layout, mount);
        }
    }

    /// Mirror the active canvas's choice at the pane's size.
    pub fn sync(&mut self, app: &App, pane_w: f32, pane_h: f32) {
        self.sync_choice(Choice::of(app), pane_w, pane_h);
    }

    pub(crate) fn sync_for_pane(
        &mut self,
        app: &App,
        pane: crate::panes::PaneId,
        pane_w: f32,
        pane_h: f32,
    ) {
        self.sync_choice(
            Choice::for_graph(app, app.graph_for_pane(pane)),
            pane_w,
            pane_h,
        );
    }

    fn sync_choice(&mut self, choice: Choice, pane_w: f32, pane_h: f32) {
        self.runner.update(|state| {
            if !state.choice.readback.same_composition(&choice.readback) {
                state.form.follow_composition(&choice.readback);
            }
            state.form.follow_roles(&choice.readback);
            state.form.follow_speed(&choice.readback);
            state.form.follow_physics(&choice.readback);
            state.choice = choice;
            state.viewport_w = pane_w;
            state.viewport_h = pane_h;
        });
    }

    /// The pane's scene at its size, under the host's cambium sheet.
    pub fn scene(&mut self, w: u32, h: u32, scale: f32) -> netrender::Scene {
        self.layout.set_scale(scale);
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
    use mere::canvas::physics_controls::{read_two_law_mix, speed_preset};
    use mere::canvas::{PhysicsLaw, Role};

    #[test]
    fn refused_overlays_are_visible_and_keep_the_applied_law() {
        let mut app = App::test_stub();
        app.update(Action::SetPhysicsOverlay(
            PhysicsOverlay::GridSnap.id(),
            true,
        ));
        app.update(Action::SetPhysicsLaw(PhysicsLaw::Density.id()));
        let refusal = app
            .physics_refusal()
            .expect("Density refused the existing overlay");
        assert_eq!(refusal.law, PhysicsLaw::Density);
        assert_eq!(refusal.refused, vec![PhysicsOverlay::GridSnap]);
        assert_eq!(app.graph_runtimes.physics_law(), PhysicsLaw::Density);
        assert!(app.graph_runtimes.physics_overlays().is_empty());
        assert!(
            arrange_rows(&app)
                .iter()
                .any(|row| row.starts_with("Physics refused:"))
        );
        assert!(app.take_events().iter().any(|event| matches!(event,
            crate::observe::AppEvent::PhysicsRefused(refusal)
                if refusal.law == PhysicsLaw::Density
        )));

        app.update(Action::SetPhysicsOverlay(
            PhysicsOverlay::Skeleton.id(),
            true,
        ));
        assert!(app.graph_runtimes.physics_overlays().is_empty());
        assert_eq!(
            app.physics_refusal().unwrap().refused,
            vec![PhysicsOverlay::Skeleton]
        );
        app.update(Action::ApplyPhysicsProfile("crystal"));
        assert!(app.physics_refusal().is_none());
        assert!(
            !arrange_rows(&app)
                .iter()
                .any(|row| row.starts_with("Physics refused:"))
        );
    }

    #[test]
    fn current_refusal_attaches_to_the_platform_tree_and_clears_without_dom_sync() {
        use crate::panes::{PaneContent, PaneKindId, kind};
        use std::collections::HashMap;

        let mut app = App::test_stub();
        app.update(Action::SummonPane(PaneKindId::new(kind::ARRANGE)));
        let pane = app.frisket.iter_leaves().find_map(|(pane, content, _)| {
            matches!(content, PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::ARRANGE)
                .then_some(pane)
        }).expect("Arrange pane");
        app.update(Action::SetPhysicsOverlay(
            PhysicsOverlay::GridSnap.id(),
            true,
        ));
        app.update(Action::SetPhysicsLaw(PhysicsLaw::Density.id()));
        let bounds = crate::surface::Rect::new(100.0, 40.0, 480.0, 400.0);
        let contribution = physics_refusal_tree(&app, pane, bounds).unwrap();
        let status_id = contribution.root;
        assert_ne!(
            status_id,
            physics_refusal_tree(&app, crate::panes::PaneId(pane.0 + 1), bounds)
                .unwrap()
                .root,
            "each pane owns a distinct stable identity"
        );
        let (tree, routes) = crate::a11y::project_app_with_routes_and_contributions(
            &app,
            HashMap::from([(pane, contribution)]),
        );
        let status = &tree
            .nodes
            .iter()
            .find(|(id, _)| *id == status_id)
            .unwrap()
            .1;
        assert_eq!(status.role(), accesskit::Role::Status);
        assert_eq!(status.live(), Some(accesskit::Live::Polite));
        assert_eq!(
            status.bounds(),
            Some(accesskit::Rect::new(100.0, 40.0, 580.0, 440.0))
        );
        let label = format!("Physics refused: {}", app.physics_refusal().unwrap());
        assert_eq!(status.label(), Some(label.as_str()));
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.children().contains(&status_id))
        );
        assert!(
            !routes.contains_key(&status_id),
            "read-only status has no action route"
        );

        app.update(Action::SetPhysicsOverlay(
            PhysicsOverlay::Skeleton.id(),
            true,
        ));
        assert_eq!(
            physics_refusal_tree(&app, pane, bounds).unwrap().root,
            status_id
        );
        app.update(Action::ApplyPhysicsProfile("crystal"));
        let contributions = physics_refusal_tree(&app, pane, bounds)
            .map(|tree| HashMap::from([(pane, tree)]))
            .unwrap_or_default();
        let (cleared, routes) =
            crate::a11y::project_app_with_routes_and_contributions(&app, contributions);
        assert!(!cleared.nodes.iter().any(|(id, _)| *id == status_id));
        assert!(!routes.contains_key(&status_id));
    }

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
        assert_eq!(
            ArrangeIntent::ApplyProfile("custom".into()).into_action(),
            None
        );
    }

    #[test]
    fn composing_intents_lower_and_reach_the_canvas() {
        let mut app = App::test_stub();
        let mix = TwoLawMix {
            first: PhysicsLaw::Charge,
            second: PhysicsLaw::Springs,
            first_share: 0.5,
            second_share: 2.0,
        };
        for intent in [
            ArrangeIntent::SetRole("pinned".into()),
            ArrangeIntent::SetSpeed("5".into()),
            ArrangeIntent::ApplyMix(mix),
        ] {
            app.update(intent.into_action().expect("a known id lowers"));
        }
        let canvas = &app.graph_runtimes;
        assert_eq!(canvas.arrangement_roles().default, Role::Pinned);
        assert_eq!(SPEED_PRESETS[speed_preset(canvas)].0, "5");
        assert_eq!(read_two_law_mix(canvas), Ok(Some(mix)));
        let rows = arrange_rows(&app);
        for expected in ["Role: Pinned", "Speed: 5x", "Mix: Charge 0.5 + Springs 2"] {
            assert!(
                rows.iter().any(|row| row.starts_with(expected)),
                "{expected}: {rows:?}"
            );
        }
        assert!(
            rows.iter()
                .any(|row| row.starts_with("Physics status: Physics mix set"))
        );
        app.update(ArrangeIntent::UseSingleLaw.into_action().unwrap());
        assert_eq!(read_two_law_mix(&app.graph_runtimes), Ok(None));
        assert_eq!(ArrangeIntent::SetRole("boss".into()).into_action(), None);
        assert_eq!(ArrangeIntent::SetSpeed("3".into()).into_action(), None);
    }

    #[test]
    fn flat_and_composed_profiles_follow_shared_root_aware_readback() {
        use mere::canvas::StageStop;
        use mere::canvas::physics_controls::{
            StageDraft, TwoLawGrouping, TwoLawMix, TwoStageSchedule,
        };

        let mut flat = App::test_stub();
        for overlay in PhysicsOverlay::ALL {
            flat.update(Action::SetPhysicsOverlay(overlay.id(), false));
        }
        flat.update(Action::SetPhysicsLaw(PhysicsLaw::Charge.id()));
        assert!(arrange_rows(&flat).contains(&"Profile: Gas".to_string()));

        let mix = TwoLawMix {
            first: PhysicsLaw::Charge,
            second: PhysicsLaw::Springs,
            first_share: 1.0,
            second_share: 1.0,
        };
        let grouping = TwoLawGrouping {
            partition: mere::canvas::PhysicsKindSource::Component,
            outer: PhysicsLaw::Charge,
            inner: PhysicsLaw::Springs,
            outer_share: 1.0,
        };
        let schedule = TwoStageSchedule {
            first: StageDraft {
                law: PhysicsLaw::Charge,
                stop: StageStop::Frames(60),
                capture: None,
            },
            second: StageDraft {
                law: PhysicsLaw::Springs,
                stop: StageStop::LawDone,
                capture: None,
            },
            overlays: Vec::new(),
        };
        for composition in [
            Action::ApplyPhysicsMix(mix),
            Action::ApplyPhysicsGrouping(grouping),
            Action::ApplyPhysicsSchedule(schedule),
        ] {
            let mut app = App::test_stub();
            app.update(composition);
            let rows = arrange_rows(&app);
            assert!(rows.contains(&"Physics: Charge".to_string()), "{rows:?}");
            assert!(rows.contains(&"Profile: custom".to_string()), "{rows:?}");
        }
    }

    #[test]
    fn canvas_verbs_share_one_label_with_the_catalogue_and_the_palette() {
        for verb in VERBS {
            let action =
                Action::from_canvas_verb(verb).expect("the pane offers what the shell has");
            assert_eq!(action.canvas_verb(), Some(verb));
            let id = crate::action::shared_command_id(&action).expect("a shared id");
            assert_eq!(cambium::catalogue::label(id), Some(verb.label()), "{id}");
            assert_eq!(ArrangeIntent::Verb(verb).into_action(), Some(action));
        }
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
