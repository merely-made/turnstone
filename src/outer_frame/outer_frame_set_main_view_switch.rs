// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl OuterFrame {
    pub(crate) fn set_main_view_switch(&mut self, visible: bool) {
        if self.main_view_switch != visible {
            self.main_view_switch = visible;
            self.controls.invalidate();
        }
    }

    /// The window's layout scale: the frame's DOM and sheet are in layout
    /// pixels, `physical / scale`, and its scene is rasterized at `scale`.
    pub(crate) fn set_layout_scale(&mut self, scale: f32) {
        self.scale = crate::ui::clean_scale(scale);
    }

    pub(crate) fn set_workspace_area(&mut self, area: Rect) {
        if self.workspace_area != Some(area) {
            self.workspace_area = Some(area);
            self.controls.invalidate();
        }
    }

    pub fn prepare(
        &mut self,
        layout: &FrisketLayout,
        current: Option<PaneId>,
        maximize: Option<PaneId>,
        size: (u32, u32),
        appearance: &crate::shell_services::AppearanceConfig,
        session: u128,
    ) -> Result<Vec<(PaneId, Rect)>, String> {
        let scale = self.scale;
        if self.size != size || self.laid_scale != scale {
            // WindowDom presents the scope as a synthetic document. Its real
            // styled child owns the viewport box and authored backdrop even
            // when every station is floating.
            use layout_dom_api::LayoutDomMut;
            self.controls.dom.borrow_mut().set_attribute(
                self.window_root,
                cambium::attr_qual("style"),
                &format!(
                    "position:relative;width:{}px;height:{}px;box-sizing:border-box;",
                    size.0 as f32 / scale,
                    size.1 as f32 / scale
                ),
            );
        }
        self.layout.borrow_mut().set_scale(scale);
        {
            use layout_dom_api::LayoutDomMut;
            // The plan's area is physical; the frame is laid out in layout
            // pixels.
            let area = self.workspace_area.unwrap_or(Rect::full(size.0, size.1));
            let style = format!(
                "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;",
                area.x / scale,
                area.y / scale,
                area.w / scale,
                area.h / scale
            );
            let changed = attr(&self.controls.dom.borrow(), self.workspace_root, "style")
                .as_deref()
                != Some(style.as_str());
            if changed {
                self.controls.dom.borrow_mut().set_attribute(
                    self.workspace_root,
                    cambium::attr_qual("style"),
                    &style,
                );
            }
        }
        if &self.appearance != appearance || self.size != size || self.laid_scale != scale {
            self.controls.invalidate();
            self.sheet = crate::frame_style::stylesheet(
                appearance.theme_presentation.as_ref(),
                &appearance.tabard_mode(),
                &appearance.theme_typography,
                &appearance.interface_font_properties,
                appearance.zoom(),
            );
            self.sheet = format!(
                "{}\n{}\n.frisket-divider{{flex-basis:1px;}}",
                crate::ui::cambium_sheet(appearance),
                self.sheet
            );
            self.appearance = appearance.clone();
        }
        let frame = layout.outer_workspace()?;
        let mut workspace = frame.workspace.clone();
        for (id, (content, _)) in &frame.panes {
            if let Some(tile) = workspace.tile_mut(TileId(*id)) {
                if matches!(
                    tile.title.as_str(),
                    "orrery" | "Orrery" | "inspector" | "Inspector" | "workbench" | "Workbench"
                ) {
                    tile.title = content.label().into();
                }
            }
        }
        if let Some(id) = maximize {
            if let Some(tile) = workspace.find(TileId(id.0)).cloned() {
                workspace = Workspace::new(TileTree::single(tile));
            }
        }
        let current = current.map(|id| TileId(id.0));
        self.controls.sync(session, workspace.clone(), current);
        // Read/Map owns the primary view selection. Its canonical shared
        // stack does not need a second row of frame tabs above document tabs.
        {
            use layout_dom_api::LayoutDomMut;
            let mut dom = self.controls.dom.borrow_mut();
            for node in nodes_at(&dom, self.controls.root()) {
                if !attr(&dom, node, "class")
                    .is_some_and(|s| s.split_whitespace().any(|c| c == "frisket-tabbar"))
                {
                    continue;
                }
                let hidden = attr(&dom, node, "data-stack")
                    .and_then(|path| {
                        let path = cambium::decode_pane_path(&path);
                        let TileTree::Stack(stack) = tree_at(workspace.tiled(), &path.0)? else {
                            return None;
                        };
                        let main = |tile: &workbench::Tile| {
                            frame.panes.get(&tile.id.0).is_some_and(|(content, _)| {
                                matches!(
                                    content,
                                    crate::panes::PaneContent::Orrery
                                        | crate::panes::PaneContent::Workbench
                                        | crate::panes::PaneContent::Overmap(_)
                                )
                            })
                        };
                        let pair = stack.tabs.iter().any(|t| {
                            frame.panes.get(&t.id.0).is_some_and(|(c, _)| {
                                matches!(c, crate::panes::PaneContent::Orrery)
                            })
                        }) && stack.tabs.iter().any(|t| {
                            frame.panes.get(&t.id.0).is_some_and(|(c, _)| {
                                matches!(
                                    c,
                                    crate::panes::PaneContent::Workbench
                                        | crate::panes::PaneContent::Overmap(_)
                                )
                            })
                        });
                        Some(
                            (pair || stack.tabs.len() == 1)
                                && stack.tabs.get(stack.active).is_some_and(main),
                        )
                    })
                    .unwrap_or(false);
                let value = if hidden && self.main_view_switch {
                    "true"
                } else {
                    "false"
                };
                if attr(&dom, node, "data-main-view-header").as_deref() != Some(value) {
                    dom.set_attribute(node, cambium::attr_qual("data-main-view-header"), value);
                }
            }
        }
        self.workspace = workspace;
        self.current = current;
        self.size = size;
        self.laid_scale = scale;
        self.scene();
        let scrolls = self.layout.borrow().scroll_stamp();
        if self.scrolls != scrolls {
            self.controls.invalidate();
            self.scrolls = scrolls;
        }
        let dom = self.controls.dom.borrow();
        let mut out = Vec::new();
        let active = frame.active_tiled_ids();
        for (id, _, node) in body_slots(&dom, self.controls.root()) {
            // Retained inactive bodies may still have cached fragments. Only
            // the workspace's active tabs own native content placements.
            if !active.contains(&id) || maximize.is_some_and(|pane| pane.0 != id.0) {
                continue;
            }
            if self.workspace.floating().iter().any(|f| f.tile.id == id) {
                continue;
            }
            if let Some((x, y, w, h)) = self.layout.borrow().visible_rect(&dom, node) {
                out.push((PaneId(id.0), Rect::new(x, y, w, h)));
            }
        }
        Ok(out)
    }

    /// Nearer floats last, with their actual retained outer and body apertures.
    pub(crate) fn floats(&self) -> Vec<(PaneId, Rect, Rect)> {
        let dom = self.controls.dom.borrow();
        let slots = body_slots(&dom, self.controls.root());
        self.workspace
            .visible_floating(true)
            .into_iter()
            .filter_map(|float| {
                let node = nodes_at(&dom, self.controls.root()).into_iter().find(|n| {
                    attr(&dom, *n, "class")
                        .is_some_and(|c| c.split_whitespace().any(|c| c == "workspace-float"))
                        && attr(&dom, *n, cambium::FRISKET_TILE_ATTR)
                            == Some(float.tile.id.0.to_string())
                })?;
                let (x, y, w, h) = self.layout.borrow().visible_rect(&dom, node)?;
                let body = slots.iter().find(|(id, _, _)| *id == float.tile.id)?.2;
                let (bx, by, bw, bh) = self.layout.borrow().visible_rect(&dom, body)?;
                Some((
                    PaneId(float.tile.id.0),
                    Rect::new(x, y, w, h),
                    Rect::new(bx, by, bw, bh),
                ))
            })
            .collect()
    }

    pub fn scene(&mut self) -> netrender::Scene {
        match self.scene_resources() {
            Ok(translated) => translated.scene,
            Err(error) => {
                let (w, h) = crate::ui::logical_size(self.size, self.scale);
                crate::ui::pane_scene(&format!("Frame cannot render: {error}"), w, h)
            },
        }
    }

    /// Resolve only frame-owned furniture. Mounted pane descendants share the
    /// same DOM and fragments, but their selectors belong to the pane owner.
    pub(crate) fn selector_point(
        &mut self,
        selector: &taproot::Selector,
    ) -> Result<Option<(f32, f32)>, &'static str> {
        if !selector.matches_surface("frame") {
            return Ok(None);
        }
        let selected = {
            let dom = self.controls.dom.borrow();
            let layout = self.layout.borrow();
            if !layout.has_current_snapshot(&dom) {
                return Err("retained frame snapshot is unavailable or stale");
            }
            let mut selected = None;
            for node in layout.matching_nodes(&dom, selector) {
                let Some(control) = self.controls.control_at(node) else {
                    continue;
                };
                if matches!(control, Control::Body { .. }) {
                    continue;
                }
                let Some((x, y, width, height)) = layout.visible_rect(&dom, node) else {
                    continue;
                };
                if width <= 0.0 || height <= 0.0 {
                    continue;
                }
                if selected.is_some() {
                    return Err("multiple visible frame controls match");
                }
                selected = Some(((x + width / 2.0, y + height / 2.0), control));
            }
            selected
        };
        let Some((point, control)) = selected else {
            return Ok(None);
        };
        let physical = self.hit(point.0, point.1);
        let same = match (control, physical) {
            (Control::Tab(expected), Some(HeaderHit::Tab(actual))) => expected == actual,
            (Control::Close(expected), Some(HeaderHit::Close(actual))) => expected == actual,
            (Control::Divider { path, index, .. }, Some(HeaderHit::Divider { target, .. })) => {
                path == target.path && index == target.index
            },
            _ => false,
        };
        if !same {
            return Err("frame control is covered or no longer hit-testable");
        }
        Ok(Some(point))
    }

    pub(crate) fn scene_resources(
        &mut self,
    ) -> Result<paint_list_render::TranslatedDisplayList, paint_list_render::TranslationError> {
        let _reactive_work = crate::reactive_render::WorkScope::window();
        self.layout.borrow_mut().scene_resources(
            &mut self.controls.dom.borrow_mut(),
            &self.sheet,
            self.size.0,
            self.size.1,
        )
    }

    pub(crate) fn stylesheet(&self) -> &str {
        &self.sheet
    }

    pub(crate) fn stamp(&self) -> FrameStamp {
        self.controls.stamp()
    }

    pub(crate) fn visible_control(&self, control: &Control) -> bool {
        let Some(node) = self.controls.node(control) else {
            return false;
        };
        let dom = self.controls.dom.borrow();
        self.layout.borrow().visible_rect(&dom, node).is_some()
            && self
                .layout
                .borrow()
                .document_projection(&dom, None)
                .is_some_and(|p| {
                    p.nodes()
                        .iter()
                        .any(|n| n.id.get() == dom.opaque_id(node) && !n.state.hidden)
                })
    }

    pub(crate) fn focus_control(&mut self, control: Option<Control>) -> bool {
        if control.as_ref().is_some_and(|c| !self.visible_control(c)) {
            return false;
        }
        self.controls.set_focus(control)
    }

    pub(crate) fn key(
        &mut self,
        event: cambium::KeyEvent,
    ) -> (Vec<workbench::WorkspaceEvent>, bool) {
        let visible = self
            .controls
            .order()
            .into_iter()
            .filter(|c| self.visible_control(c))
            .collect::<Vec<_>>();
        self.controls.key(event, &visible)
    }

    pub(crate) fn accessibility(
        &self,
        focused: bool,
    ) -> Option<crate::frame_a11y::FrameProjection> {
        let dom = self.controls.dom.borrow();
        let focus = if focused {
            self.controls.focus_node()
        } else {
            self.body_focus
        };
        let (tree, map) = self.layout.borrow().accessibility_tree(&dom, focus)?;
        Some(crate::frame_a11y::project(
            tree,
            map,
            &self.controls,
            focused,
        ))
    }

    pub(crate) fn accessibility_action(
        &mut self,
        route: &FrameRoute,
        action: accesskit::Action,
    ) -> Option<Vec<workbench::WorkspaceEvent>> {
        if !self.controls.admits(route) || !self.visible_control(&route.control) {
            return None;
        }
        let control = &route.control;
        if matches!(control, Control::Body { .. }) {
            return None;
        }
        match action {
            accesskit::Action::Focus => {
                self.controls.set_focus(Some(control.clone()));
                // Frisket tabs use automatic selection; close/divider focus
                // remains focus-only and never performs their default action.
                Some(if matches!(control, Control::Tab(_)) {
                    self.controls.click(control)
                } else {
                    Vec::new()
                })
            },
            accesskit::Action::Click if !matches!(control, Control::Divider { .. }) => {
                Some(self.controls.click(control))
            },
            accesskit::Action::Increment => Some(vec![self.controls.resize(control, 0.01)?]),
            accesskit::Action::Decrement => Some(vec![self.controls.resize(control, -0.01)?]),
            _ => None,
        }
    }

    pub(crate) fn body_target(
        &self,
        route: &FrameRoute,
        action: accesskit::Action,
    ) -> Option<Rect> {
        let Control::Body { node, .. } = route.control else {
            return None;
        };
        if !self.controls.admits(route) || !self.visible_control(&route.control) {
            return None;
        }
        let projection = self.accessibility(false)?;
        let global = projection
            .routes
            .iter()
            .find_map(|(id, current)| (current == route).then_some(*id))?;
        if !projection
            .nodes
            .iter()
            .any(|(id, node)| *id == global && node.supports_action(action))
        {
            return None;
        }
        let dom = self.controls.dom.borrow();
        let (x, y, w, h) = self.layout.borrow().visible_rect(&dom, node)?;
        Some(Rect::new(x, y, w, h))
    }
}
