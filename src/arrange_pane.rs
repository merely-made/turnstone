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

#[cfg(test)]
#[path = "arrange_pane/tests.rs"]
mod tests;

#[path = "arrange_pane/arrange_intent_into_action.rs"]
mod arrange_intent_into_action;
#[path = "arrange_pane/field_option.rs"]
mod field_option;
#[path = "arrange_pane/impl_cambium_action_for_arrange_intent.rs"]
mod impl_cambium_action_for_arrange_intent;
use field_option::field_option;
#[path = "arrange_pane/field_label.rs"]
mod field_label;
use field_label::field_label;
#[path = "arrange_pane/arrange_rows.rs"]
mod arrange_rows;
#[path = "arrange_pane/choice_of.rs"]
mod choice_of;
pub use arrange_rows::arrange_rows;
#[path = "arrange_pane/spec.rs"]
mod spec;
use spec::spec;
#[path = "arrange_pane/choice_control.rs"]
mod choice_control;
use choice_control::choice_control;
#[path = "arrange_pane/catalog_options.rs"]
mod catalog_options;
use catalog_options::catalog_options;
#[path = "arrange_pane/choice_row.rs"]
mod choice_row;
use choice_row::choice_row;
#[path = "arrange_pane/section.rs"]
mod section;
use section::section;
#[path = "arrange_pane/scene_button.rs"]
mod scene_button;
use scene_button::scene_button;
#[path = "arrange_pane/apply_button.rs"]
mod apply_button;
use apply_button::apply_button;
#[path = "arrange_pane/arrange_view.rs"]
mod arrange_view;
use arrange_view::arrange_view;
#[path = "arrange_pane/physics_refusal_tree.rs"]
mod physics_refusal_tree;
pub(crate) use physics_refusal_tree::physics_refusal_tree;
#[path = "arrange_pane/arrange_pane_new.rs"]
mod arrange_pane_new;
#[path = "arrange_pane/impl_default_for_arrange_pane.rs"]
mod impl_default_for_arrange_pane;
