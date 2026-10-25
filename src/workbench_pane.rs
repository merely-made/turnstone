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

fn workbench_sheet() -> &'static str {
    static SHEET: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        format!("{}\n{}\n.wb-single-reading .read-header {{ cursor:grab; }}", crate::ui::CAMBIUM_SHEET, cambium::SHELL_CSS)
    });
    &SHEET
}

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

fn workbench_view(state: &WorkbenchState) -> WbView {
    let mut children: Vec<WbView> = Vec::new();
    for (i, cell) in state.cells.iter().enumerate() {
        let items: Vec<_> = cell
            .labels
            .iter()
            .zip(&cell.members)
            .map(|(label, member)| TabItem::new(label).with_key(member.to_string()))
            .collect();
        let targets: Vec<_> = cell
            .members
            .iter()
            .filter_map(|member| {
                cell.target.map(|target| crate::reading_view::ReadTarget {
                    pane: target.pane,
                    member: *member,
                })
            })
            .collect();
        let strip = tab_bar_view(
            TabBar::new(&items, cell.strip.selected).with_label("Reading documents").with_names(cambium::TabBarNames {label:Some("reading-tab-label"),..Default::default()}),
            move |state: &mut WorkbenchState, selected| state.cells[i].strip.selected = selected,
            Some(move |state: &mut WorkbenchState, index: usize| {
                if let Some(target) = targets.get(index) {
                    state.read_actions.push(crate::action::Action::Reading(
                        crate::reading_view::ReadAction::Close(*target),
                    ));
                }
            }),
        );
        let body_h = cell.body.h;
        let body = el::<_, WorkbenchState, ()>("div", cell.hint.clone())
            .attr("class", "wb-body")
            .attr(
                "data-native-role",
                cell.appearance_role
                    .map(|role| role.to_string())
                    .unwrap_or_default(),
            )
            .attr(
                "data-native-node",
                cell.members
                    .get(cell.strip.selected)
                    .map(Uuid::to_string)
                    .unwrap_or_default(),
            )
            .attr("style", format!("height: {body_h}px;"));
        children.push(Box::new(placed_with(
            Placement::new(cell.rect.x, cell.rect.y),
            format!(
                "width: {}px; height: {}px; overflow: hidden;",
                cell.rect.w, cell.rect.h
            ),
            arrangement(
                cell.rect.w,
                cell.rect.h,
                vec![
                    if cell.tabs.w<=0.0 || cell.tabs.h<=0.0 {Box::new(el("div",()).attr("style","display:none;")) as WbView} else {Box::new(placed_with(
                        Placement::new(cell.tabs.x - cell.rect.x, cell.tabs.y - cell.rect.y),
                        format!(
                            "width:{}px;height:{}px;overflow:hidden;",
                            cell.tabs.w, cell.tabs.h
                        ),
                        el::<_, WorkbenchState, ()>("div", strip)
                            .attr(
                                "class",
                                if cell.vertical {
                                    "wb-tabs-vertical"
                                } else {
                                    "wb-tabs-horizontal"
                                },
                            )
                            .attr("style", "width:100%;height:100%;"),
                    )) as WbView},
                    Box::new(placed_with(
                        Placement::new(cell.toolbar.x - cell.rect.x, cell.toolbar.y - cell.rect.y),
                        format!("width:{}px;height:{}px;overflow:hidden;",cell.toolbar.w,cell.toolbar.h),
                        reading_toolbar(state, i),
                    )) as WbView,
                    Box::new(placed_with(
                        Placement::new(cell.body.x - cell.rect.x, cell.body.y - cell.rect.y),
                        format!(
                            "width:{}px;height:{}px;overflow:hidden;",
                            cell.body.w, cell.body.h
                        ),
                        body,
                    )) as WbView,
                ],
            ),
        )));
    }
    Box::new(
        el::<_, WorkbenchState, ()>(
            "div",
            arrangement(state.viewport_w, state.viewport_h, children),
        )
        .attr("class", "pane cambium-document-rail"),
    )
}

fn reading_toolbar(state: &WorkbenchState, index: usize) -> WbView {
    let cell = &state.cells[index];
    let Some(target) = cell.target else { return Box::new(el("div", ())); };
    let mut ui = cell.find.clone();
    ui.available = cell.find_enabled;
    let find = Some(&ui);
    let close = cambium::shell_control(
        "×",
        format!(
            "Close reading: {}",
            cell.labels
                .get(cell.strip.selected)
                .map(String::as_str)
                .unwrap_or("document")
        ),
        Default::default(),
        move |state: &mut WorkbenchState, _| {
            state.read_actions.push(crate::action::Action::Reading(
                crate::reading_view::ReadAction::Close(target),
            ))
        },
    );
    let toolbar=cambium::read_toolbar_with_trailing(
        cell.preferences.text_size,
        cell.preferences.column_em,
        cell.text_controls,
        find,
        if cell.tabs.w>0.0 && cell.tabs.h>0.0 {Vec::new()} else {vec![close]},
        move |state: &mut WorkbenchState, event| {
            use crate::reading_view::ReadAction;
            let action = match event {
                cambium::ReadEvent::Command(command) => ReadAction::Command(target, command),
                cambium::ReadEvent::Query(input) => ReadAction::Query(target, input),
            };
            state.read_actions.push(crate::action::Action::Reading(action));
        });
    Box::new(el("div",toolbar).attr("class",if cell.tabs.w<=0.0 {"wb-single-reading"} else {"wb-tabbed-reading"}))
}

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

impl WorkbenchPane {
    pub fn new() -> Self {
        Self::new_in(None)
    }

    pub(crate) fn new_in(mount: Option<cambium::RetainedMount>) -> Self {
        let dom: DomHandle = mount
            .as_ref()
            .map(cambium::RetainedMount::dom)
            .unwrap_or_else(|| Rc::new(RefCell::new(ScriptedDom::new())));
        let state = WorkbenchState {
            cells: Vec::new(),
            read_actions: Vec::new(),
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        let runner = WbRunner::new_at(
            dom.clone(),
            mount
                .as_ref()
                .map_or_else(|| dom.borrow().document(), cambium::RetainedMount::node),
            workbench_view as fn(&WorkbenchState) -> WbView,
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
            tiling: WorkbenchTiling::default(),
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
    pub(crate) fn key_inline(&mut self, event: cambium::KeyEvent) -> Vec<WbActivate> {
        let _: Vec<()> = self.runner.dispatch_key(event);
        self.runner
            .state()
            .cells
            .iter()
            .filter(|cell| cell.strip.selected != cell.synced_active)
            .filter_map(|cell| cell.members.get(cell.strip.selected).copied())
            .map(WbActivate)
            .collect()
    }
    pub(crate) fn take_read_actions(&mut self) -> Vec<crate::action::Action> {
        let mut actions = Vec::new();
        self.runner.update(|state| actions = std::mem::take(&mut state.read_actions));
        actions
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

    /// Refresh from app truth at the pane's size: walk the model's geometry
    /// into pane-local rects, resolve tab labels off graph truth, mirror each
    /// cell's active tab into its strip.
    pub fn sync(&mut self, app: &App, pane_id: crate::panes::PaneId, pane_w: f32, pane_h: f32) {
        let geom = app
            .workbench_for_pane(pane_id)
            .map(|workbench| workbench.to_arrangement().1)
            .unwrap_or(None);
        // The tiling is the plan's geometry, in physical pixels, and input
        // routing reads it as such; the pane's DOM is in layout pixels.
        let scale = app.pane_layout_scale(pane_id);
        let logical = |rect: Rect| {
            Rect::new(
                rect.x / scale,
                rect.y / scale,
                rect.w / scale,
                rect.h / scale,
            )
        };
        self.tiling = crate::workbench_tiling::place_reading_workbench(
            app,
            pane_id,
            geom.as_ref(),
            Rect::new(0.0, 0.0, pane_w, pane_h),
        );
        let graph = app
            .graph_for_pane(pane_id)
            .and_then(|graph| app.graph_runtimes.canvas(graph));
        let label_of = |member: Uuid| -> String {
            graph
                .into_iter()
                .flat_map(|canvas| canvas.graph().nodes())
                .find(|(_, n)| n.id == member)
                .map(|(_, n)| {
                    if n.title.trim().is_empty() {
                        n.url().to_string()
                    } else {
                        n.title.clone()
                    }
                })
                .unwrap_or_else(|| "gone".to_string())
        };
        let cells: Vec<CellView> = self
            .tiling
            .cells
            .iter()
            .map(|c| {
                let labels: Vec<String> = c.members.iter().map(|&m| label_of(m)).collect();
                // The honest body hint: what the ACTIVE tile's content is
                // doing, when there is nothing composited to show for it.
                let hint = match c.active_member().map(|m| app.content.get(m)) {
                    Some(Some(NodeContent::Live)) => String::new(),
                    Some(Some(NodeContent::Requested)) => "content loading".to_string(),
                    Some(Some(NodeContent::AwaitingInput)) => "content needs input".to_string(),
                    Some(Some(NodeContent::AwaitingIdentity)) => {
                        "content needs a client identity".to_string()
                    },
                    Some(Some(NodeContent::AwaitingTrust)) => {
                        "content needs a certificate decision".to_string()
                    },
                    Some(Some(NodeContent::Failed(err))) => format!("failed: {err}"),
                    Some(None) => "content off — Toggle live content".to_string(),
                    None => String::new(),
                };
                CellView {
                    toolbar: logical(c.toolbar_rect()),
                    target: c
                        .active_member()
                        .map(|member| crate::reading_view::ReadTarget {
                            pane: pane_id,
                            member,
                        }),
                    find_enabled: c.active_member().is_some_and(|member| {
                        app.content
                            .facts(member)
                            .is_some_and(|facts| facts.capabilities.find_in_page.is_available())
                    }),
                    text_controls: c.active_member().is_some_and(|member| {
                        app.has_reading_controls(crate::reading_view::ReadTarget {
                            pane: pane_id,
                            member,
                        })
                    }),
                    preferences: c
                        .active_member()
                        .map(|member| {
                            app.reading.preferences(crate::reading_view::ReadTarget {
                                pane: pane_id,
                                member,
                            })
                        })
                        .unwrap_or_default(),
                    find: c
                        .active_member()
                        .and_then(|member| {
                            app.reading.find.get(&crate::reading_view::ReadTarget {
                                pane: pane_id,
                                member,
                            })
                        })
                        .cloned()
                        .unwrap_or_default(),
                    tabs: logical(c.tab_rect()),
                    body: logical(c.body()),
                    vertical: matches!(
                        c.tab_edge,
                        crate::chrome_layout::TabEdge::Left | crate::chrome_layout::TabEdge::Right
                    ),
                    appearance_role: c.active_member().map(|node| {
                        crate::surface::appearance::workbench_appearance_role(pane_id, node)
                    }),
                    members: c.members.clone(),
                    rect: logical(c.rect),
                    labels,
                    strip: TabStrip::new(c.active).with_label("Reading documents"),
                    synced_active: c.active,
                    hint,
                }
            })
            .collect();
        self.runner.update(|state| {
            state.cells = cells;
            state.viewport_w = pane_w / scale;
            state.viewport_h = pane_h / scale;
        });
    }

    /// The last synced walk (pane-local rects). The shell's press routing
    /// reads cells and divider bands from here so input and paint agree.
    pub fn tiling(&self) -> &WorkbenchTiling {
        &self.tiling
    }

    pub fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }

    /// The pane's scene at its size, under the host's cambium sheet.
    pub fn scene(&mut self, w: u32, h: u32, scale: f32) -> netrender::Scene {
        self.layout.set_scale(scale);
        self.layout.scene_scrolled(
            &mut self.dom.borrow_mut(),
            workbench_sheet(),
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

    /// The member whose TAB sits under pane-local `(x, y)`, from the laid-out
    /// DOM (the ask-the-layout rule: tab x-positions are flex + text
    /// measurement, so only the layout knows them). The tab-drag gesture's
    /// press resolution.
    pub fn tab_at(&self, x: f32, y: f32, _w: u32, _h: u32) -> Option<Uuid> {
        if self.is_close_at(x,y) {return None;}
        let dom = self.dom.borrow();
        // Resolve the host-owned stable key on the actual painted tab. Hidden
        // singleton strips do not shift the identity of another cell's tabs.
        let root = self
            .mount
            .as_ref()
            .map_or_else(|| dom.document(), cambium::RetainedMount::node);
        let tabs = dom.all_with_class(root, "tab");
        for &tab in &tabs {
            if let Some((tx, ty, tw, th)) = self.layout.visible_rect(&dom, tab)
                && x >= tx
                && x < tx + tw
                && y >= ty
                && y < ty + th
            {
                let member=dom.attribute(tab,&layout_dom_api::Namespace::from(""),&layout_dom_api::LocalName::from("data-tabkey"))
                    .and_then(|key|Uuid::parse_str(key).ok())?;
                return self.runner.state().cells.iter().any(|cell|cell.members.contains(&member)).then_some(member);
            }
        }
        // A singleton retains a grab region in the blank part of its Read
        // header. Aa, Find and Close keep their own pointer ownership.
        let within=|node|self.layout.visible_rect(&dom,node).is_some_and(|(rx,ry,rw,rh)|x>=rx&&x<rx+rw&&y>=ry&&y<ry+rh);
        if dom.all_with_class(root,"read-header").into_iter().any(within)
            && !dom.all_with_class(root,"shell-control").into_iter().any(within) {
            return self.runner.state().cells.iter().find(|cell|cell.members.len()==1 && cell.tabs.w<=0.0 && x>=cell.rect.x&&x<cell.rect.x+cell.rect.w&&y>=cell.rect.y&&y<cell.rect.y+cell.rect.h).map(|cell|cell.members[0]);
        }
        None
    }

    /// Closing a view must not activate its stack or steal another pane's
    /// document context before the targeted close action is delivered.
    pub(crate) fn is_close_at(&self,x:f32,y:f32)->bool {
        let dom=self.dom.borrow();
        let root=self.mount.as_ref().map_or_else(||dom.document(),cambium::RetainedMount::node);
        ["tab-close","read-header-trailing"].into_iter().any(|class|
            dom.all_with_class(root,class).into_iter().any(|node|
                self.layout.visible_rect(&dom,node).is_some_and(|(cx,cy,cw,ch)|x>=cx&&x<cx+cw&&y>=cy&&y<cy+ch)))
    }

    /// Resolve only visible painted tabs, retaining their real member hit.
    pub(crate) fn selector_point(
        &self,
        selector: &taproot::Selector,
    ) -> Result<Option<(f32, f32)>, &'static str> {
        if !selector.matches_surface("workbench") {
            return Ok(None);
        }
        let dom = self.dom.borrow();
        if !self.layout.has_current_snapshot(&dom) {
            return Err("retained Workbench snapshot is unavailable or stale");
        }
        let root = self.mount.as_ref().map_or_else(|| dom.document(), cambium::RetainedMount::node);
        let tabs = dom.all_with_class(root, "tab");
        let mut point = None;
        for node in self.layout.matching_nodes(&dom, selector) {

            let Some((x, y, width, height)) = self.layout.visible_rect(&dom, node) else {
                continue;
            };
            if width <= 0.0 || height <= 0.0 {
                continue;
            }
            if point.is_some() {
                return Err("multiple visible Workbench tabs match");
            }
            // The floating toolbar can cover a tab's centre; use the visible
            // top strip, then let the shell verify the actual surface hit.
            point = Some((x + width / 2.0, y + if tabs.contains(&node) { height / 8.0 } else { height / 2.0 }));
        }
        Ok(point)
    }

    /// Route a click at pane-local `(x, y)` into the view (a tab click moves
    /// its strip's selection); report each cell whose selection moved away
    /// from the synced model state as an activation for the shell to lower.
    pub fn click(&mut self, x: f32, y: f32, w: u32, h: u32) -> Vec<WbActivate> {
        let hit = self.layout.hit_test_scrolled(
            &mut self.dom.borrow_mut(),
            workbench_sheet(),
            w,
            h,
            x,
            y,
            &self.scroll,
        );
        if let Some(node) = hit {
            let _: Vec<()> = self.runner.dispatch_click(node, PointerClick::at((x, y)));
        }
        self.runner
            .state()
            .cells
            .iter()
            .filter(|c| c.strip.selected != c.synced_active)
            .filter_map(|c| c.members.get(c.strip.selected).copied())
            .map(WbActivate)
            .collect()
    }

    /// Resolve a selector to a point within this pane's DOM at window rect
    /// `rect`, using this pane's retained semantics and geometry (the
    /// scenario's `drag-tab` aims through this).
    pub fn resolve(&self, sel: &taproot::Selector, rect: [f32; 4]) -> Option<(f32, f32)> {
        if !sel.matches_surface("workbench") {
            return None;
        }
        let dom = self.dom.borrow();
        self.layout
            .first_selector_point(&dom, sel)
            .map(|(x, y)| (rect[0] + x, rect[1] + y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use layout_dom_api::LayoutDomMut;
    #[test]
    fn closing_a_background_tab_uses_its_own_target_and_never_starts_a_drag() {
        let (mut app,a,b)=app_with_two_tiles();
        app.update(Action::WorkbenchStackOnto {dragged:b,target:a});
        app.update(Action::WorkbenchActivate(b));
        let owner=app.frisket.iter_leaves().find(|(_,c,_)|matches!(c,crate::panes::PaneContent::Workbench)).unwrap().0;
        let mut pane=WorkbenchPane::new();pane.sync(&app,owner,800.,600.);pane.scene(800,600,1.0);
        let selector=taproot::Selector::role("button").on_surface("workbench").containing("Close mere://alpha");
        let (x,y)=pane.selector_point(&selector).unwrap().unwrap();
        assert_eq!(pane.tab_at(x,y,800,600),None);
        assert!(pane.click(x,y,800,600).is_empty());
        let actions=pane.take_read_actions();
        assert!(matches!(actions.as_slice(),[Action::Reading(crate::reading_view::ReadAction::Close(crate::reading_view::ReadTarget {pane,member}))] if *pane==owner && *member==a));
        for action in actions {app.update(action);}
        assert!(!app.workbench_for_pane(owner).unwrap().has_tile(a));
        assert!(app.workbench_for_pane(owner).unwrap().has_tile(b));
        assert_eq!(app.browser_command_target().map(|(m,_)|m),Some(b));
    }

    fn pane_over(app: &App, w: f32, h: f32) -> WorkbenchPane {
        let mut pane = WorkbenchPane::new();
        pane.sync(app, app.default_graph_pane(), w, h);
        pane.scene(w as u32, h as u32, 1.0);
        pane
    }

    fn app_with_two_tiles() -> (App, Uuid, Uuid) {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("mere://alpha".to_string()));
        let a = app.graph_runtimes.focused_member().unwrap();
        app.update(Action::OpenInWorkbench);
        app.update(Action::OpenAddress("mere://beta".to_string()));
        let b = app.graph_runtimes.focused_member().unwrap();
        app.update(Action::OpenInWorkbench);
        (app, a, b)
    }

    #[test]
    fn native_read_toolbar_routes_controls_outside_its_document_hole() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("mere://reading".into()));
        app.update(Action::OpenInWorkbench);
        let member=app.graph_runtimes.focused_member().unwrap();
        app.apply_update(crate::action::Update::ContentSpawned{node:member,facts:Some(crate::content::ContentFacts{engine:"genet.reader".into(),lane:crate::content::ContentLane::Document,structure:None,lineage:None,capabilities:Default::default()})});
        let pane_id=app.frisket.iter_leaves().find(|(_,content,_)|matches!(content,crate::panes::PaneContent::Workbench)).unwrap().0;
        let mut chrome=app.shell_chrome_config().clone(); chrome.layout_scales.insert(0,2.0); app.set_shell_chrome_config(chrome);
        let mut pane=WorkbenchPane::new(); pane.sync(&app,pane_id,1200.,800.);pane.scene(1200,800,2.0);
        let selector=taproot::Selector::role("button").on_surface("workbench").containing("Larger reading text");
        assert!(pane.selector_point(&selector).unwrap().is_none(),"appearance controls start folded");
        let aa=taproot::Selector::role("button").on_surface("workbench").containing("Reading appearance");
        let (x,y)=pane.selector_point(&aa).unwrap().unwrap();pane.click(x,y,1200,800);
        for action in pane.take_read_actions() {app.update(action);}
        pane.sync(&app,pane_id,1200.,800.);pane.scene(1200,800,2.0);
        let point=pane.selector_point(&selector).unwrap().unwrap();
        let cell=&pane.tiling().cells[0];
        assert!(cell.toolbar_rect().contains(point.0,point.1));
        assert!(!cell.body().contains(point.0,point.1));
        let target=cell.active_member().map(|member|crate::reading_view::ReadTarget{pane:pane_id,member}).unwrap();
        pane.click(point.0,point.1,1200,800);
        let actions=pane.take_read_actions();
        assert_eq!(actions.len(),1);
        for action in actions { app.update(action); }
        assert_eq!(app.reading.preferences(target).text_size,20);
        assert_eq!(app.shell_chrome_config().appearance.ui_zoom,1.1,"native density is not a persisted zoom edit");
    }

    #[test]
    fn all_document_tab_edges_keep_painted_hits_and_native_apertures_aligned() {
        use crate::chrome_layout::TabEdge;
        let (mut app, a, b) = app_with_two_tiles();
        app.update(Action::WorkbenchStackOnto {
            dragged: b,
            target: a,
        });
        for edge in [TabEdge::Top, TabEdge::Bottom, TabEdge::Left, TabEdge::Right] {
            let mut config = app.shell_chrome_config().clone();
            config.layout.document_tabs = edge;
            app.set_shell_chrome_config(config);
            let pane = pane_over(&app, 800.0, 600.0);
            let cell = &pane.tiling().cells[0];
            let dom = pane.dom.borrow();
            let node = dom.all_with_class(dom.document(), "wb-body")[0];
            let (x, y, w, h) = pane.layout.visible_rect(&dom, node).unwrap();
            let expected = cell.body();
            for (actual, want) in [x, y, w, h]
                .into_iter()
                .zip([expected.x, expected.y, expected.w, expected.h])
            {
                assert!((actual - want).abs() < 0.1, "{edge:?}: {actual} != {want}");
            }
            drop(dom);
            for (name, member) in [("alpha", a), ("beta", b)] {
                let point = pane
                    .selector_point(
                        &taproot::Selector::class("tab")
                            .containing(name)
                            .on_surface("workbench"),
                    )
                    .unwrap()
                    .unwrap();
                assert!(cell.tab_rect().contains(point.0, point.1));
                assert_eq!(pane.tab_at(point.0, point.1, 800, 600), Some(member));
            }
        }
    }

    #[test]
    fn long_page_titles_cannot_claim_the_siblings_tab() {
        let (mut app, a, b) = app_with_two_tiles();
        app.update(Action::WorkbenchStackOnto {dragged:b,target:a});
        for (member, name) in [(a, "Scry A"), (b, "Scry B")] {
            app.update(Action::ContentTitleChanged {
                member,
                title: format!("{name} {}", "long page state ".repeat(64)),
            });
        }
        let pane = pane_over(&app, 800.0, 600.0);
        for (member, name) in [(a, "Scry A"), (b, "Scry B")] {
            let point = pane
                .selector_point(
                    &taproot::Selector::class("tab")
                        .containing(name)
                        .on_surface("workbench"),
                )
                .unwrap()
                .expect("the visible tab has a target");
            let cell = pane
                .tiling()
                .cells
                .iter()
                .find(|cell| cell.members.contains(&member))
                .unwrap();
            assert!(
                cell.rect.contains(point.0, point.1),
                "tab hit stays in its owning cell"
            );
            assert_eq!(pane.tab_at(point.0, point.1, 800, 600), Some(member));
        }
        assert!(
            pane.selector_point(&taproot::Selector::class("tab").on_surface("workbench"))
                .is_err()
        );
    }

    #[test]
    fn singleton_cells_hide_strips_and_keep_one_header_close_per_reading() {
        let (app,_,_)=app_with_two_tiles();let pane=pane_over(&app,800.,600.);
        assert_eq!(pane.tiling().cells.len(),2);
        let dom=pane.dom.borrow();let root=dom.document();
        assert!(dom.all_with_class(root,"tab").is_empty());
        assert_eq!(dom.all_with_class(root,"read-header-trailing").len(),2);
        for cell in &pane.tiling().cells {assert_eq!(cell.tab_rect().w,0.);assert_eq!(cell.toolbar_rect().y,cell.rect.y);}
    }
    #[test]
    fn a_singleton_keeps_a_header_grab_region_clear_of_appearance_and_close_controls() {
        let (app,a,_)=app_with_two_tiles();let pane=pane_over(&app,800.,600.);let cell=&pane.tiling().cells[0];
        let toolbar=cell.toolbar_rect();assert_eq!(pane.tab_at(toolbar.x+toolbar.w*0.5,toolbar.y+8.,800,600),Some(a));
        let close=taproot::Selector::role("button").on_surface("workbench").containing("Close reading: mere://alpha");
        let (x,y)=pane.selector_point(&close).unwrap().unwrap();assert_eq!(pane.tab_at(x,y,800,600),None);
    }
    #[test]
    fn tabs_after_a_hidden_singleton_still_resolve_to_their_actual_members() {
        let (mut app,_,b)=app_with_two_tiles();
        app.update(Action::OpenAddress("mere://gamma".into()));let c=app.graph_runtimes.focused_member().unwrap();
        app.update(Action::OpenInWorkbench);app.update(Action::WorkbenchStackOnto {dragged:c,target:b});
        let pane=pane_over(&app,800.,600.);
        for (name,member) in [("beta",b),("gamma",c)] {
            let (x,y)=pane.selector_point(&taproot::Selector::class("tab").on_surface("workbench").containing(name)).unwrap().unwrap();
            assert_eq!(pane.tab_at(x,y,800,600),Some(member));
        }
    }
    #[test]
    fn a_multi_tab_cell_has_tab_closes_and_no_duplicate_header_close() {
        let (mut app,a,b)=app_with_two_tiles();app.update(Action::WorkbenchStackOnto {dragged:b,target:a});
        let pane=pane_over(&app,800.,600.);let dom=pane.dom.borrow();let root=dom.document();
        assert_eq!(dom.all_with_class(root,"tab-close").len(),2);
        assert!(dom.all_with_class(root,"read-header-trailing").is_empty());
    }

    #[test]
    fn selector_resolution_waits_for_current_paint() {
        let (mut app,a,b)=app_with_two_tiles();app.update(Action::WorkbenchStackOnto {dragged:b,target:a});
        let mut pane = WorkbenchPane::new();
        pane.sync(&app, app.default_graph_pane(), 800.0, 600.0);
        let selector = taproot::Selector::class("tab").containing("alpha");
        let rect = [0.0, 0.0, 800.0, 600.0];
        assert_eq!(
            pane.resolve(&selector, rect),
            None,
            "no painted snapshot yet"
        );
        pane.scene(800, 600, 1.0);
        assert!(pane.resolve(&selector, rect).is_some());

        let tab = {
            let dom = pane.dom.borrow();
            taproot::matching_classes(&dom, &selector)[0]
        };
        pane.dom.borrow_mut().set_attribute(
            tab,
            cambium::attr_qual("data-paint-revision"),
            "changed",
        );
        assert_eq!(pane.resolve(&selector, rect), None, "old geometry is stale");
        pane.scene(800, 600, 1.0);
        assert!(pane.resolve(&selector, rect).is_some());
    }

    /// The press resolution: a point inside a drawn tab maps back to its
    /// member — the tab-drag gesture's anchor.
    #[test]
    fn a_tab_point_resolves_to_its_member() {
        let (mut app,a,b)=app_with_two_tiles();app.update(Action::WorkbenchStackOnto {dragged:b,target:a});
        let pane = pane_over(&app, 800.0, 600.0);
        let (ax, ay) = pane
            .resolve(
                &taproot::Selector::class("tab").containing("alpha"),
                [0.0, 0.0, 800.0, 600.0],
            )
            .unwrap();
        assert_eq!(pane.tab_at(ax, ay, 800, 600), Some(a));
        let (bx, by) = pane
            .resolve(
                &taproot::Selector::class("tab").containing("beta"),
                [0.0, 0.0, 800.0, 600.0],
            )
            .unwrap();
        assert_eq!(pane.tab_at(bx, by, 800, 600), Some(b));
        assert_eq!(
            pane.tab_at(400.0, 300.0, 800, 600),
            None,
            "a body point is no tab"
        );
    }

    /// A click on an inactive tab of a stacked cell reports the activation
    /// for the shell to lower — the strip's own selection is the signal.
    #[test]
    fn clicking_an_inactive_tab_reports_activation() {
        let (mut app, a, b) = app_with_two_tiles();
        app.update(Action::WorkbenchStackOnto {
            dragged: b,
            target: a,
        });
        // b landed active; a's tab is the inactive one.
        let mut pane = pane_over(&app, 800.0, 600.0);
        assert_eq!(pane.tiling().cells.len(), 1);
        let (x, y) = pane
            .resolve(
                &taproot::Selector::class("tab").containing("alpha"),
                [0.0, 0.0, 800.0, 600.0],
            )
            .expect("alpha's tab is drawn in the stack");
        let acts = pane.click(x, y, 800, 600);
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].0, a, "clicking alpha's tab activates alpha");
    }
}
