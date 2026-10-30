// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Workbench pane's view half (rung 5 slice E, the cambium path): each
//! cell of the walked tiling drawn as a placed box wearing cambium's
//! `tab_strip`, with a body hint underneath. The tile CONTENT does not render
//! here — a live tile's document composites as its own surface at the cell's
//! body rect (the shell's surface plan), exactly like the rung-4 content
//! inset; this DOM is the workbench's furniture (tab bars, cell backdrops,
//! and the honest hint when a tile has no live content to show).
//!
//! `workbench_tiling` is the geometry half (platen's `TreeGeometry` walked
//! into rects); this holds the retained cambium runner over it. Tab clicks
//! land in each cell's `TabStrip` (the strip owns its selection); the shell
//! reads the diff back as `WorkbenchActivate` — the same mirror-out pattern
//! the Roster's strip uses, generalized to N strips.

use std::cell::RefCell;
use std::rc::Rc;

use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, PointerClick, TabBar, TabItem,
    TabStrip, arrangement, el, placed_with, tab_bar_view,
};
use genet_scripted_dom::ScriptedDom;
use layout_dom_api::LayoutDom;
use sprigging::Placement;
use uuid::Uuid;

use crate::app::App;
use crate::content::NodeContent;
use crate::surface::Rect;
use crate::workbench_tiling::{WorkbenchTiling, place_workbench_at};

/// One cell as the view holds it: the walked placement's members and rect,
/// the tab labels, the strip state (selection mirrors the model's active
/// tab at sync; a click moves it and the shell lowers the diff), and the
/// body hint.
struct CellView {
    tabs: Rect,
    body: Rect,
    vertical: bool,
    appearance_role: Option<u64>,
    members: Vec<Uuid>,
    rect: Rect,
    labels: Vec<String>,
    strip: TabStrip,
    /// The synced model-active index, for the click diff.
    synced_active: usize,
    /// The body hint ("content off", "failed: ..."), empty when live content
    /// composites over the body.
    hint: String,
    toolbar: Rect,
    target: Option<crate::reading_view::ReadTarget>,
    preferences: tabard::ReadingPreferences,
    text_controls: bool,
    find_enabled: bool,
    find: cambium::ReadFind,
}

struct WorkbenchState {
    cells: Vec<CellView>,
    read_actions: Vec<crate::action::Action>,
    viewport_w: f32,
    viewport_h: f32,
}

type WbView = Box<dyn AnyView<WorkbenchState, (), GenetCtx, GenetElement>>;
type WbRunner = GenetAppRunner<WorkbenchState, fn(&WorkbenchState) -> WbView, WbView, ()>;

/// An activation the pane reports back after a click: make this member's tab
/// the active one (the shell lowers `Action::WorkbenchActivate`).
pub struct WbActivate(pub Uuid);

/// The Workbench pane: a retained cambium runner over the walked tiling.
/// Held by the shell like the other panes.
pub struct WorkbenchPane {
    dom: DomHandle,
    runner: WbRunner,
    scroll: crate::ui::PaneScroll,
    /// Kept across frames. Rebuilding a layout per paint re-cascaded and
    /// re-shaped the whole pane to draw an unchanged screen; see
    /// [`crate::ui::RetainedLayout`] for the measurement.
    layout: crate::ui::RetainedLayout,
    /// The last synced walk, pane-local — the shell's input routing asks it
    /// for cells and divider bands (the same walk the frame drew).
    tiling: WorkbenchTiling,
    // Drop the runner and its widget resources before final mount custody.
    mount: Option<cambium::RetainedMount>,
}

#[cfg(test)]
#[path = "workbench_pane/tests.rs"]
mod tests;

#[path = "workbench_pane/workbench_sheet.rs"]
mod workbench_sheet;
use workbench_sheet::workbench_sheet;
#[path = "workbench_pane/workbench_view.rs"]
mod workbench_view;
use workbench_view::workbench_view;
#[path = "workbench_pane/reading_toolbar.rs"]
mod reading_toolbar;
use reading_toolbar::reading_toolbar;
#[path = "workbench_pane/workbench_pane_new.rs"]
mod workbench_pane_new;
