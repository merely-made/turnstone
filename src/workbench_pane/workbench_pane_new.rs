// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
        self.runner
            .update(|state| actions = std::mem::take(&mut state.read_actions));
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
        if self.is_close_at(x, y) {
            return None;
        }
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
                let member = dom
                    .attribute(
                        tab,
                        &layout_dom_api::Namespace::from(""),
                        &layout_dom_api::LocalName::from("data-tabkey"),
                    )
                    .and_then(|key| Uuid::parse_str(key).ok())?;
                return self
                    .runner
                    .state()
                    .cells
                    .iter()
                    .any(|cell| cell.members.contains(&member))
                    .then_some(member);
            }
        }
        // A singleton retains a grab region in the blank part of its Read
        // header. Aa, Find and Close keep their own pointer ownership.
        let within = |node| {
            self.layout
                .visible_rect(&dom, node)
                .is_some_and(|(rx, ry, rw, rh)| x >= rx && x < rx + rw && y >= ry && y < ry + rh)
        };
        if dom
            .all_with_class(root, "read-header")
            .into_iter()
            .any(within)
            && !dom
                .all_with_class(root, "shell-control")
                .into_iter()
                .any(within)
        {
            return self
                .runner
                .state()
                .cells
                .iter()
                .find(|cell| {
                    cell.members.len() == 1
                        && cell.tabs.w <= 0.0
                        && x >= cell.rect.x
                        && x < cell.rect.x + cell.rect.w
                        && y >= cell.rect.y
                        && y < cell.rect.y + cell.rect.h
                })
                .map(|cell| cell.members[0]);
        }
        None
    }

    /// Closing a view must not activate its stack or steal another pane's
    /// document context before the targeted close action is delivered.
    pub(crate) fn is_close_at(&self, x: f32, y: f32) -> bool {
        let dom = self.dom.borrow();
        let root = self
            .mount
            .as_ref()
            .map_or_else(|| dom.document(), cambium::RetainedMount::node);
        ["tab-close", "read-header-trailing"]
            .into_iter()
            .any(|class| {
                dom.all_with_class(root, class).into_iter().any(|node| {
                    self.layout
                        .visible_rect(&dom, node)
                        .is_some_and(|(cx, cy, cw, ch)| {
                            x >= cx && x < cx + cw && y >= cy && y < cy + ch
                        })
                })
            })
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
        let root = self
            .mount
            .as_ref()
            .map_or_else(|| dom.document(), cambium::RetainedMount::node);
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
            point = Some((
                x + width / 2.0,
                y + if tabs.contains(&node) {
                    height / 8.0
                } else {
                    height / 2.0
                },
            ));
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
