// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The cambium seam: a cambium view rendered into a turnstone pane surface, and
//! the pane's pointer events routed back into it. Rung 5 slice D, the toolkit
//! adoption.
//!
//! cambium builds a `ScriptedDom` — the exact type turnstone's chrome already lays
//! out through Livery/Buckram into a paint list — so a cambium view drops into the
//! compose-at-surface-rect seam slice A built. A `GenetAppRunner` holds the view's
//! state and renders it into a `DomHandle`; turnstone composites that DOM, and
//! routes a pane-local click back through retained Livery hit testing into
//! `dispatch_click`, which returns the Actions the view emitted. Those lower
//! through turnstone's ordinary spine — the same path a keypress takes.
//!
//! That round trip (render -> composite -> hit-test -> dispatch -> Action) is the
//! general pane-event path every cambium pane reuses; the Roster's `data_grid` is
//! its first consumer. Turnstone is cambium's first pixel host, so this integration
//! is new: the catalog example is a headless acceptance test.

use std::cell::RefCell;
use std::rc::Rc;

use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, GridColumn, GridSpec, PointerClick,
    TabStrip, data_grid, el, lens, on_click, tab_strip,
};
use genet_scripted_dom::ScriptedDom;
use layout_dom_api::LayoutDom;

use crate::app::App;
use crate::roster_view::{RosterGridRow, roster_grid_rows_for_pane};

/// The Roster's tabs — `mere::roster`'s four data views over one grid. Only
/// Nodes has a gatherer today; the rest render an honest empty state until
/// theirs land (Links needs the edge families walked into `build_link_rows`).
const ROSTER_TABS: [&str; 4] = ["Nodes", "Links", "Graphlets", "Fields"];

/// The Roster pane's state: the tab strip, the flat node rows, and the pane
/// size — the height the grid virtualizes against, and both dimensions for the
/// pane's own backdrop.
struct RosterState {
    binding: crate::pane_binding::BindingViewState,
    tabs: TabStrip,
    rows: Vec<RosterGridRow>,
    viewport_w: f32,
    viewport_h: f32,
}

/// What a Roster grid interaction produces. The shell lowers `Navigate` as
/// `Action::OpenAddress`, so a grid click reaches the graph through the same
/// spine as a keypress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RosterAction {
    Binding(crate::pane_binding::BindingIntent),
    Navigate(String),
}

// Opt `RosterAction` into cambium's bubbling-action marker, so an `on_click`
// handler may return it directly (the blanket `OptionalAction<A>` impl). Without
// the marker only `()` handlers compile — cambium seals this deliberately, so an
// app names the types it bubbles.
impl cambium::Action for RosterAction {}

type RosterView = Box<dyn AnyView<RosterState, RosterAction, GenetCtx, GenetElement>>;
type RosterRunner =
    GenetAppRunner<RosterState, fn(&RosterState) -> RosterView, RosterView, RosterAction>;

/// Two columns: the node's display name and its address.
fn roster_spec() -> GridSpec {
    GridSpec {
        columns: vec![
            GridColumn::new("Node", 240.0),
            GridColumn::new("Address", 360.0),
        ],
        row_height: 26.0,
        header_height: 28.0,
        overscan: 4,
    }
}

/// The Roster pane: a tab strip over the active tab's view. The strip is
/// cambium's (consumer-pulled by this pane), lensed onto our `tabs` field; the
/// Nodes tab is the node grid, the others an honest empty state until their
/// gatherers land.
fn roster_pane(state: &RosterState) -> RosterView {
    let strip = lens(
        |tabs: &mut TabStrip| tab_strip::<RosterAction>(tabs, &ROSTER_TABS),
        |state: &mut RosterState| &mut state.tabs,
    );
    let body: RosterView = match state.tabs.selected {
        0 => roster_grid(state),
        _ => Box::new(
            el::<_, RosterState, RosterAction>(
                "div",
                format!(
                    "{} has no gatherer yet",
                    ROSTER_TABS[state.tabs.selected.min(ROSTER_TABS.len() - 1)]
                ),
            )
            .attr("class", "pane-empty"),
        ),
    };
    // The pane paints its own backdrop at the pane's full size. A tab whose body
    // is text-sized (an empty state) covers only a few rows of pixels, and what
    // sits under a pane is not the pane's colour — so without this the bare gap
    // shows through. Geometry inline, per the sheet contract; the colour is the
    // host sheet's `.pane`, the same one the hand-DOM panes wear.
    Box::new(
        el::<_, RosterState, RosterAction>(
            "div",
            (
                crate::pane_binding::binding_view(
                    &state.binding,
                    |s: &mut RosterState| &mut s.binding,
                    RosterAction::Binding,
                ),
                strip,
                body,
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

/// The Nodes tab: the graph's node manifest as a grid. Cells own their text and
/// their target url (cambium's runner retains the view, so a cell cannot borrow
/// `state`); a click on either column emits `Navigate` for that row.
fn roster_grid(state: &RosterState) -> RosterView {
    let cell_rows = state.rows.clone();
    let class_rows = state.rows.clone();
    data_grid(
        &roster_spec(),
        state.rows.len(),
        (state.viewport_h - crate::ui::TABLIST_HEIGHT).max(0.0),
        0.0,
        move |row, col| {
            let (text, url) = match cell_rows.get(row) {
                Some(r) if col == 0 => (r.title.clone(), r.url.clone()),
                Some(r) => (r.url.clone(), r.url.clone()),
                None => (String::new(), String::new()),
            };
            // `roster-cell` is the taproot hit target: a stable class whose
            // direct child text is the cell's, so `click-row` resolves a grid
            // row the same way it resolves a Trail `list-row`. A block `div`
            // (not an inline `span`) so it has a box `absolute_rect` resolves AND
            // fills the cell width, so the resolved centre lands on the
            // clickable rather than in empty space past short text. No sheet rule
            // — `.grid-cell` already styles it; this class is targeting only.
            Box::new(on_click(
                el::<_, RosterState, RosterAction>("div", text).attr("class", "roster-cell"),
                move |_state: &mut RosterState, _click: PointerClick| {
                    RosterAction::Navigate(url.clone())
                },
            )) as RosterView
        },
        // Sort-by-column is caller state; a follow-on. Header clicks are inert.
        |_state: &mut RosterState, _col: usize| {},
        move |row| {
            class_rows
                .get(row)
                .filter(|r| r.selected)
                .map(|_| "grid-row-selected".to_string())
        },
    )
}

/// The pane-local y at the centre of grid row `idx` — below the tab strip and
/// the grid's sticky header. A test helper now that `click-row` resolves off the
/// DOM through taproot (`grid_dom_is_hit_testable` uses it to aim a probe
/// click at a known row); kept because it pins the grid's row math to the layout.
pub fn grid_row_center_y(idx: usize) -> f32 {
    let spec = roster_spec();
    crate::ui::TABLIST_HEIGHT
        + spec.header_height
        + idx as f32 * spec.row_height
        + spec.row_height / 2.0
}

/// The label of Roster tab `idx`, clamped. The observation rendering of
/// `App::roster_tab`, which is an index because the strip's selection is.
pub fn tab_label(idx: usize) -> &'static str {
    ROSTER_TABS[idx.min(ROSTER_TABS.len() - 1)]
}

/// The Roster pane's cambium grid: a retained runner over the node rows. Held by
/// the shell (the runner is `!Send`, like the content sessions) so its state and
/// DOM persist between the frame that draws it and the click that hits it.
pub struct RosterGrid {
    dom: DomHandle,
    runner: RosterRunner,
    scroll: crate::ui::PaneScroll,
    /// Kept across frames. Rebuilding a layout per paint re-cascaded and
    /// re-shaped the whole pane to draw an unchanged screen; see
    /// [`crate::ui::RetainedLayout`] for the measurement.
    layout: crate::ui::RetainedLayout,
    // Drop the runner and its widget resources before final mount custody.
    mount: Option<cambium::RetainedMount>,
}

impl RosterGrid {
    pub fn new() -> Self {
        Self::new_in(None)
    }

    pub(crate) fn new_in(mount: Option<cambium::RetainedMount>) -> Self {
        let dom: DomHandle = mount
            .as_ref()
            .map(cambium::RetainedMount::dom)
            .unwrap_or_else(|| Rc::new(RefCell::new(ScriptedDom::new())));
        let state = RosterState {
            binding: Default::default(),
            tabs: TabStrip::new(0).with_label("Roster"),
            rows: Vec::new(),
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        let runner = RosterRunner::new_at(
            dom.clone(),
            mount
                .as_ref()
                .map_or_else(|| dom.borrow().document(), cambium::RetainedMount::node),
            roster_pane as fn(&RosterState) -> RosterView,
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
    pub(crate) fn key_inline(&mut self, event: cambium::KeyEvent) -> Vec<RosterAction> {
        if matches!(event.key, cambium::Key::Named(cambium::NamedKey::Escape))
            && self.runner.state().binding.expanded
        {
            self.runner.update(|state| state.binding.expanded = false);
            return Vec::new();
        }
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

    /// Refresh the grid from graph truth and the pane's size. Rebuilds the view
    /// (and its DOM) through the runner. The size is the whole pane; the grid
    /// virtualizes against what the tab strip leaves it.
    pub fn sync(&mut self, app: &App, pane_id: crate::panes::PaneId, pane_w: f32, pane_h: f32) {
        let rows = roster_grid_rows_for_pane(app, pane_id);
        self.runner.update(|state| {
            state.binding.sync(app, pane_id);
            state.rows = rows;
            state.viewport_w = pane_w;
            state.viewport_h = pane_h;
        });
    }

    /// Which tab is active, and its label.
    pub fn selected_tab(&self) -> (usize, &'static str) {
        let i = self.runner.state().tabs.selected;
        (i, ROSTER_TABS[i.min(ROSTER_TABS.len() - 1)])
    }

    /// Resolve a selector to a point within this pane's DOM at window rect
    /// `rect` (`[x, y, w, h]`), using this pane's retained semantics and
    /// geometry. Returns the window-space centre of the first visible match.
    pub fn resolve(&self, sel: &taproot::Selector, rect: [f32; 4]) -> Option<(f32, f32)> {
        if !sel.matches_surface("roster") {
            return None;
        }
        let dom = self.dom.borrow();
        self.layout
            .first_selector_point(&dom, sel)
            .map(|(x, y)| (rect[0] + x, rect[1] + y))
    }

    /// Borrow this pane's DOM for the shared driver's `with_surfaces`. The Ref
    /// guard lives for the caller's scope, which is why the trait hands surfaces
    /// to a visitor rather than returning them.
    pub fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }

    /// The grid's scene at the pane's size, under the host's cambium sheet.
    pub fn scene(&mut self, w: u32, h: u32, scale: f32) -> netrender::Scene {
        self.layout.set_scale(scale);
        self.layout.scene_scrolled(
            &mut self.dom.borrow_mut(),
            crate::ui::cambium_shell_sheet(),
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

    /// Route a click at pane-local `(x, y)` into the view: lay the DOM out at the
    /// pane's size, hit-test the point to a DOM node, and dispatch. Returns the
    /// Actions the view emitted (empty when the point hit nothing interactive).
    pub fn click(&mut self, x: f32, y: f32, w: u32, h: u32) -> Vec<RosterAction> {
        let hit = self.layout.hit_test_scrolled(
            &mut self.dom.borrow_mut(),
            crate::ui::cambium_shell_sheet(),
            w,
            h,
            x,
            y,
            &self.scroll,
        );
        // The borrow is dropped: dispatch rebuilds the view into the same DOM.
        let actions = match hit {
            Some(node) => self.runner.dispatch_click(node, PointerClick::at((x, y))),
            None => Vec::new(),
        };
        tracing::debug!(
            x,
            y,
            w,
            h,
            hit = ?hit.map(|n| n.raw()),
            rows = self.runner.state().rows.len(),
            viewport_h = self.runner.state().viewport_h,
            actions = actions.len(),
            "roster grid click"
        );
        actions
    }
}

#[cfg(test)]
#[path = "cambium_pane/tests.rs"]
mod tests;
