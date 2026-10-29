// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! One retained Frisket composition per window. In-house bodies share its
//! document, cascade, fragments and paint; native/foreign producers use slots.
use crate::frame_controls::{Control, FrameControls, FrameRoute, FrameStamp};
use crate::{
    panes::{FrisketLayout, PaneId},
    surface::Rect,
};
use genet_scripted_dom::{NodeId, ScriptedDom};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use workbench::{DropTarget, Edge, TileId, TileTree, Workspace};

#[derive(Clone, Debug)]
pub enum HeaderHit {
    Tab(TileId),
    Close(TileId),
    Divider {
        target: cambium::DividerTarget,
        area: Rect,
        axis: workbench::SplitAxis,
        shares: Vec<f32>,
        gaps: Vec<f32>,
    },
}

pub struct OuterFrame {
    window_scope: NodeId,
    window_root: NodeId,
    workspace_root: NodeId,
    workspace_area: Option<Rect>,
    main_view_switch: bool,
    scrolls: Vec<(u64, u32, u32)>,
    pub(crate) body_focus: Option<NodeId>,
    pub(crate) controls: FrameControls,
    pub(crate) layout: crate::ui::InlineLayoutHandle,
    workspace: Workspace,
    current: Option<TileId>,
    /// Physical window size and the layout scale the frame is laid out at:
    /// its DOM is in layout pixels, `size / scale`.
    size: (u32, u32),
    scale: f32,
    /// The scale `prepare` last composed at.
    laid_scale: f32,
    sheet: String,
    appearance: crate::shell_services::AppearanceConfig,
}
impl Default for OuterFrame {
    fn default() -> Self {
        Self::new(cambium::DomHandle::new(std::cell::RefCell::new(
            ScriptedDom::new(),
        )))
    }
}
impl OuterFrame {
    pub(crate) fn new(dom: cambium::DomHandle) -> Self {
        use layout_dom_api::LayoutDomMut;
        let scope = dom.borrow_mut().create_element(cambium::html_qual("div"));
        let root = dom.borrow_mut().create_element(cambium::html_qual("div"));
        dom.borrow_mut()
            .set_attribute(root, cambium::attr_qual("class"), "turnstone-window-root");
        dom.borrow_mut().set_attribute(
            root,
            cambium::attr_qual("style"),
            "position:relative;width:1px;height:1px;box-sizing:border-box;",
        );
        let document = dom.borrow().document();
        dom.borrow_mut().append_child(document, scope);
        dom.borrow_mut().append_child(scope, root);
        let workspace_root = dom.borrow_mut().create_element(cambium::html_qual("div"));
        dom.borrow_mut().append_child(root, workspace_root);
        let controls = FrameControls::new_at(dom, workspace_root);
        let workspace = Workspace::default();
        let appearance = crate::shell_services::AppearanceConfig::default();
        let sheet = crate::frame_style::stylesheet(
            None,
            &appearance.tabard_mode(),
            &appearance.theme_typography,
            "",
            appearance.zoom(),
        );
        let sheet = format!("{}\n{}", crate::ui::cambium_sheet(&appearance), sheet);
        Self {
            window_scope: scope,
            window_root: root,
            workspace_root,
            workspace_area: None,
            main_view_switch: true,
            scrolls: Vec::new(),
            body_focus: None,
            controls,
            workspace,
            current: None,
            size: (1, 1),
            scale: 1.0,
            laid_scale: 1.0,
            sheet,
            appearance,
            layout: std::rc::Rc::new(std::cell::RefCell::new(crate::ui::RetainedLayout::new_at(
                Some(scope),
            ))),
        }
    }
}
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
        for (id,(content,_)) in &frame.panes {
            if let Some(tile)=workspace.tile_mut(TileId(*id)) {
                if matches!(tile.title.as_str(),"orrery"|"Orrery"|"inspector"|"Inspector"|"workbench"|"Workbench") {tile.title=content.label().into();}
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
                                matches!(c, crate::panes::PaneContent::Workbench | crate::panes::PaneContent::Overmap(_))
                            })
                        });
                        Some((pair || stack.tabs.len()==1) && stack.tabs.get(stack.active).is_some_and(main))
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
    /// Resolve a producer to its actual retained aperture. Inline Workbench
    /// bodies supply their source identity; graph/inset sources use the existing
    /// native hole in the shared Frisket tree.
    pub(crate) fn native_source_node(
        &self,
        app: &crate::app::App,
        surface: &crate::surface::Surface,
        slot: usize,
    ) -> Option<NodeId> {
        use crate::surface::{SurfaceId, SurfaceKind};
        let dom = self.controls.dom.borrow();
        let slots = body_slots(&dom, self.controls.root());
        let matches_role = |node, role| {
            SurfaceId::content_appearance(
                node,
                role ^ (slot as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
            ) == surface.id
        };
        let candidates = match surface.kind {
            SurfaceKind::Graph(pane) | SurfaceKind::Pane(pane) => slots
                .iter()
                .filter(|(id, _, _)| id.0 == pane.0)
                .map(|(_, _, node)| *node)
                .collect::<Vec<_>>(),
            SurfaceKind::Content(node) => {
                let mut candidates = Vec::new();
                for (id, _, aperture) in &slots {
                    let role = match app.pane_content(PaneId(id.0)) {
                        Some(crate::panes::PaneContent::Tile(source)) if *source == node => {
                            Some(0x1000_0000_0000_0000 | id.0)
                        },
                        Some(crate::panes::PaneContent::Orrery) => {
                            Some(0x3000_0000_0000_0000 | id.0)
                        },
                        _ => None,
                    };
                    if role.is_some_and(|role| matches_role(node, role)) {
                        candidates.push(*aperture);
                    }
                }
                let name = node.to_string();
                candidates.extend(nodes_at(&dom, self.controls.root()).into_iter().filter(
                    |candidate| {
                        attr(&dom, *candidate, "data-native-node").as_deref() == Some(name.as_str())
                            && attr(&dom, *candidate, "data-native-role")
                                .and_then(|value| value.parse().ok())
                                .is_some_and(|role| matches_role(node, role))
                    },
                ));
                candidates
            },
            _ => Vec::new(),
        };
        // A source UUID and rectangle are never appearance authority. Equal
        // overlapping appearances retain distinct roles; ambiguity is refused.
        (candidates.len() == 1).then(|| candidates[0])
    }
    pub(crate) fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.controls.dom.borrow()
    }
    pub(crate) fn header_height(&self) -> f32 {
        let dom = self.controls.dom.borrow();
        nodes_at(&dom, self.controls.root())
            .into_iter()
            .filter(|n| {
                attr(&dom, *n, "class")
                    .is_some_and(|class| class.split_whitespace().any(|c| c == "frisket-tabbar"))
            })
            .filter_map(|n| {
                self.layout
                    .borrow()
                    .painted_rect(&dom, n)
                    .map(|(_, _, _, h)| h)
            })
            .fold(0.0, f32::max)
    }
    pub fn hit(&mut self, x: f32, y: f32) -> Option<HeaderHit> {
        let hit = self.layout.borrow_mut().hit_test(
            &mut self.controls.dom.borrow_mut(),
            &self.sheet,
            self.size.0,
            self.size.1,
            x,
            y,
        )?;
        let dom = self.controls.dom.borrow();
        // Body widgets share the tab-bar vocabulary (including numeric
        // data-tabkey values). Their controls belong to the retained runner,
        // even when an ancestor or descendant resembles frame furniture.
        if cambium::slot_kind(&*dom, hit).is_none() {
            if let Some(id) = cambium::close_target(&*dom, hit) {
                return Some(HeaderHit::Close(id));
            }
            if let Some(id) = cambium::tab_target(&*dom, hit) {
                return Some(HeaderHit::Tab(id));
            }
        }
        // Keep the layout seam narrow while admitting a comfortable pointer
        // band on either side. Only a divider in the hit's own split may win:
        // a floating pane covering a seam must keep its input. Reverse walk
        // order gives nested dividers precedence at intersections.
        let hit = self
            .controls
            .order()
            .into_iter()
            .rev()
            .find_map(|control| {
                let Control::Divider { .. } = control else {
                    return None;
                };
                let node = self.controls.node(&control)?;
                let (rx, ry, rw, rh) = self.layout.borrow().painted_rect(&dom, node)?;
                let vertical = attr(&dom, node, "aria-orientation").as_deref() == Some("vertical");
                if !crate::pane::divider_grab_rect(Rect::new(rx, ry, rw, rh), vertical)
                    .contains(x, y)
                {
                    return None;
                }
                let parent = dom.parent(node)?;
                let mut ancestor = Some(hit);
                while let Some(at) = ancestor {
                    if at == parent {
                        return Some(node);
                    }
                    ancestor = dom.parent(at);
                }
                None
            })
            .unwrap_or(hit);
        let target = cambium::divider_target(&*dom, hit)?;
        let tree = tree_at(self.workspace.tiled(), &target.path.0)?;
        let TileTree::Split { axis, children } = tree else {
            return None;
        };
        let mut parent = dom.parent(hit)?;
        // Divider targets can be descendants. Find the containing split box.
        loop {
            if attr(&dom, parent, "class")
                .is_some_and(|s| s.split_whitespace().any(|c| c == "frisket-split"))
            {
                break;
            }
            parent = dom.parent(parent)?;
        }
        let (x, y, w, h) = self.layout.borrow().painted_rect(&dom, parent)?;
        let branches = dom
            .dom_children(parent)
            .into_iter()
            .filter(|n| {
                attr(&dom, *n, "class")
                    .is_some_and(|class| class.split_whitespace().any(|c| c == "frisket-branch"))
            })
            .map(|n| self.layout.borrow().painted_rect(&dom, n))
            .collect::<Option<Vec<_>>>()?;
        if branches.len() != children.len() {
            return None;
        }
        let row = *axis == workbench::SplitAxis::Row;
        let first = branches.first()?;
        let last = branches.last()?;
        // Branch-to-branch gaps include authored divider width and margins;
        // using their span also excludes split padding from distributable space.
        let gaps = branches
            .windows(2)
            .map(|pair| {
                if row {
                    pair[1].0 - pair[0].0 - pair[0].2
                } else {
                    pair[1].1 - pair[0].1 - pair[0].3
                }
            })
            .collect();
        let area = if row {
            Rect::new(first.0, y, last.0 + last.2 - first.0, h)
        } else {
            Rect::new(x, first.1, w, last.1 + last.3 - first.1)
        };
        Some(HeaderHit::Divider {
            target,
            area,
            axis: *axis,
            shares: children.iter().map(|c| c.fraction).collect(),
            gaps,
        })
    }
    pub fn drop_target(&mut self, x: f32, y: f32) -> Option<DropTarget> {
        if x < 0.0 || y < 0.0 || x >= self.size.0 as f32 || y >= self.size.1 as f32 {
            return Some(DropTarget::Outside);
        }
        let hit = self.layout.borrow_mut().hit_test(
            &mut self.controls.dom.borrow_mut(),
            &self.sheet,
            self.size.0,
            self.size.1,
            x,
            y,
        )?;
        let dom = self.controls.dom.borrow();
        if let Some((stack, index)) = cambium::tab_drop_index(&*dom, hit, x, |node| {
            self.layout.borrow().painted_rect(&dom, node)
        }) {
            return Some(DropTarget::Stack { stack, index });
        }
        let mut node = hit;
        loop {
            if let Some(id) =
                attr(&dom, node, cambium::FRISKET_TILE_ATTR).and_then(|s| s.parse::<u64>().ok())
            {
                let (rx, ry, w, h) = self.layout.borrow().painted_rect(&dom, node)?;
                let dx = (x - rx) / w.max(1.0);
                let dy = (y - ry) / h.max(1.0);
                let edge = if dx < 0.25 {
                    Edge::Left
                } else if dx > 0.75 {
                    Edge::Right
                } else if dy < 0.5 {
                    Edge::Top
                } else {
                    Edge::Bottom
                };
                return Some(DropTarget::Edge {
                    tile: TileId(id),
                    edge,
                });
            }
            node = dom.parent(node)?;
        }
    }
    /// Explicit Ctrl-docking inspects tiled apertures beneath the dragged float.
    pub(crate) fn float_dock_target(&self, x: f32, y: f32) -> Option<workbench::FloatDockTarget> {
        if self.workspace.tiled().is_empty()
            && self
                .workspace_area
                .unwrap_or(Rect::full(self.size.0, self.size.1))
                .contains(x, y)
        {
            return Some(workbench::FloatDockTarget::TiledRoot);
        }
        let dom = self.controls.dom.borrow();
        for (id, _, node) in body_slots(&dom, self.controls.root()) {
            if self.workspace.tiled().find(id).is_none() {
                continue;
            }
            let (rx, ry, w, h) = self.layout.borrow().visible_rect(&dom, node)?;
            if !Rect::new(rx, ry, w, h).contains(x, y) {
                continue;
            }
            let dx = (x - rx) / w.max(1.0);
            let dy = (y - ry) / h.max(1.0);
            let edge = if dx < 0.25 {
                Some(Edge::Left)
            } else if dx > 0.75 {
                Some(Edge::Right)
            } else if dy < 0.25 {
                Some(Edge::Top)
            } else if dy > 0.75 {
                Some(Edge::Bottom)
            } else {
                None
            };
            return Some(match edge {
                Some(edge) => workbench::FloatDockTarget::Beside { target: id, edge },
                None => workbench::FloatDockTarget::Tab { target: id },
            });
        }
        None
    }
}
impl Drop for OuterFrame {
    fn drop(&mut self) {
        use layout_dom_api::LayoutDomMut;
        let handle = self.controls.dom.clone();
        // Removing a root releases its entire subtree in Genet. Move live
        // bodies home first, preserving destination-first window custody.
        self.controls.withdraw_mounts(self.window_root);
        let mut dom = handle.borrow_mut();
        if dom.is_live(self.window_scope) {
            dom.remove(self.window_scope);
        }
    }
}
fn tree_at<'a>(mut tree: &'a TileTree, path: &[usize]) -> Option<&'a TileTree> {
    for i in path {
        let TileTree::Split { children, .. } = tree else {
            return None;
        };
        tree = &children.get(*i)?.tree;
    }
    Some(tree)
}
fn attr(dom: &ScriptedDom, node: NodeId, name: &str) -> Option<String> {
    dom.attribute(node, &Namespace::default(), &LocalName::from(name))
        .map(str::to_owned)
}
fn nodes_at(dom: &ScriptedDom, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut todo = vec![root];
    while let Some(node) = todo.pop() {
        out.push(node);
        todo.extend(dom.dom_children(node));
    }
    out
}

fn body_slots(dom: &ScriptedDom, root: NodeId) -> Vec<(TileId, cambium::SlotKind, NodeId)> {
    nodes_at(dom, root)
        .into_iter()
        .filter_map(|node| {
            let class = attr(dom, node, "class")?;
            if !class.split_whitespace().any(|c| c == "frisket-content") {
                return None;
            }
            let id = attr(dom, node, cambium::FRISKET_TILE_ATTR)?.parse().ok()?;
            Some((TileId(id), cambium::slot_kind(dom, node)?, node))
        })
        .collect()
}

#[cfg(test)]
mod native_aperture_tests {
    use super::*;
    use crate::surface::{Surface, SurfaceId, SurfaceKind};
    use layout_dom_api::LayoutDomMut;

    #[test]
    fn frame_selector_rejects_body_slots_and_uses_header_hit() {
        let mut app = crate::app::App::test_stub();
        app.frisket.root = crate::panes::PaneNode::Leaf {
            pane_id: PaneId(90),
            content: crate::panes::PaneContent::Roster,
            graph_id: crate::panes::GraphId::nil(),
        };
        app.active_pane = Some(PaneId(90));
        let mut frame = OuterFrame::default();
        frame
            .prepare(
                &app.frisket,
                app.active_pane,
                None,
                (900, 600),
                &app.shell_chrome_config().appearance,
                app.session_id.as_uuid().as_u128(),
            )
            .expect("retained frame");
        assert!(
            frame
                .selector_point(&taproot::Selector::class("frisket-tab"))
                .expect("painted header")
                .is_some()
        );
        assert_eq!(
            frame
                .selector_point(&taproot::Selector::class("frisket-content"))
                .expect("body slot is not frame furniture"),
            None
        );
    }

    #[test]
    fn read_map_stack_has_no_redundant_header_but_lens_keeps_its_controls() {
        let mut app = crate::app::App::test_stub();
        app.update(crate::action::Action::OpenAddress(
            "https://example.test/a".into(),
        ));
        app.update(crate::action::Action::SetWorkspaceMap(false));
        let mut frame = OuterFrame::default();
        frame
            .prepare(
                &app.frisket,
                app.active_pane,
                None,
                (900, 600),
                &app.shell_chrome_config().appearance,
                app.session_id.as_uuid().as_u128(),
            )
            .unwrap();
        assert_eq!(frame.header_height(), 0.0);
        assert!(
            frame
                .selector_point(&taproot::Selector::class("frisket-tab"))
                .unwrap()
                .is_none()
        );
        frame.prepare(&app.frisket,app.active_pane,app.active_pane,(900,600),&app.shell_chrome_config().appearance,app.session_id.as_uuid().as_u128()).unwrap();
        assert_eq!(frame.header_height(),0.0,"maximizing Read keeps the shared toolbar as its header");
        app.update(crate::action::Action::SetWorkspaceMap(true));
        let bodies = frame.prepare(&app.frisket, app.active_pane, None,
            (900, 600), &app.shell_chrome_config().appearance,
            app.session_id.as_uuid().as_u128()).unwrap();
        assert_eq!(bodies.len(), 1, "retained reader has no Map aperture");
        assert_eq!(bodies[0].0, app.default_graph_pane());
        frame.set_main_view_switch(false);
        frame
            .prepare(
                &app.frisket,
                app.active_pane,
                None,
                (900, 600),
                &app.shell_chrome_config().appearance,
                app.session_id.as_uuid().as_u128(),
            )
            .unwrap();
        assert!(
            frame.header_height() > 0.0,
            "a lens without Read/Map retains frame tabs"
        );
    }
    #[test]
    fn compact_headers_and_hairline_dividers_keep_generous_pointer_targets() {
        for side in [
            crate::panes::InsertSide::Right,
            crate::panes::InsertSide::Below,
        ] {
            let mut app = crate::app::App::test_stub();
            app.frisket.summon_leaf(
                &[],
                side,
                crate::panes::PaneNode::Leaf {
                    pane_id: PaneId(90),
                    content: crate::panes::PaneContent::Roster,
                    graph_id: crate::panes::GraphId::nil(),
                },
            );
            let mut frame = OuterFrame::default();
            frame
                .prepare(
                    &app.frisket,
                    app.active_pane,
                    None,
                    (900, 600),
                    &app.shell_chrome_config().appearance,
                    app.session_id.as_uuid().as_u128(),
                )
                .unwrap();
            assert!(
                frame.header_height() <= 36.0,
                "compact default header: {}",
                frame.header_height()
            );
            let control = frame
                .controls
                .order()
                .into_iter()
                .find(|c| matches!(c, Control::Divider { .. }))
                .unwrap();
            let node = frame.controls.node(&control).unwrap();
            let (x, y, w, h) = frame
                .layout
                .borrow()
                .painted_rect(&frame.controls.dom.borrow(), node)
                .unwrap();
            let vertical = matches!(side, crate::panes::InsertSide::Right);
            assert_eq!(if vertical { w } else { h }, 1.0);
            for offset in [-5.0, 0.0, 5.0] {
                let (px, py) = if vertical {
                    (x + w / 2.0 + offset, y + h / 2.0)
                } else {
                    (x + w / 2.0, y + h / 2.0 + offset)
                };
                assert!(
                    matches!(frame.hit(px, py), Some(HeaderHit::Divider { .. })),
                    "grab at {offset} on {side:?}"
                );
            }
        }
    }
    #[test]
    fn the_final_floating_canvas_keeps_one_actual_native_aperture() {
        let mut app = crate::app::App::test_stub();
        let pane = PaneId(0);
        app.active_pane = Some(pane);
        app.update(crate::action::Action::FloatActivePane);
        let mut frame = OuterFrame::default();
        let docked = frame
            .prepare(
                &app.frisket,
                app.active_pane,
                None,
                (1180, 800),
                &app.shell_chrome_config().appearance,
                app.session_id.as_uuid().as_u128(),
            )
            .unwrap();
        assert!(docked.is_empty(), "the empty stack has no source placement");
        let floats = frame.floats();
        assert_eq!(floats.len(), 1);
        assert_eq!(floats[0].0, pane);
        let surface = Surface {
            id: SurfaceId::graph(pane),
            kind: SurfaceKind::Graph(pane),
            rect: floats[0].2,
        };
        let slots = body_slots(&frame.controls.dom.borrow(), frame.controls.root());
        assert_eq!(
            slots.len(),
            1,
            "only the real floating owner has a marked body"
        );
        assert_eq!(
            frame.native_source_node(&app, &surface, 0),
            Some(slots[0].2)
        );
        frame.sheet.push_str(
            "\n.workspace-float{opacity:0.55;box-shadow:10px 12px 18px rgba(255,64,0,0.75);}",
        );
        let translated = frame.scene_resources().unwrap();
        let root_rect = frame
            .layout
            .borrow()
            .painted_rect(&frame.controls.dom.borrow(), frame.window_root)
            .unwrap();
        assert_eq!(
            root_rect,
            (0.0, 0.0, 1180.0, 800.0),
            "all-floating stations keep the actual window root viewport"
        );
        assert_eq!(translated.box_shadow_masks.len(), 1);
        let mask = &translated.box_shadow_masks[0];
        assert!(mask.dim > 0 && mask.blur_radius_px > 0.0);
        let scene: serde_json::Value =
            serde_json::from_str(&translated.scene.snapshot_json()).unwrap();
        let ops = scene["ops"].as_array().unwrap();
        assert!(
            ops.iter().any(|op| {
                let rect = &op["Rect"];
                rect["x0"].as_f64() == Some(0.0)
                    && rect["y0"].as_f64() == Some(0.0)
                    && rect["x1"].as_f64() == Some(1180.0)
                    && rect["y1"].as_f64() == Some(800.0)
                    && rect["color"][3].as_f64() == Some(1.0)
            }),
            "the authored window backdrop is emitted outside floated apertures"
        );
        assert!(
            ops.iter()
                .any(|op| op["Image"]["key"].as_u64() == Some(mask.key)),
            "the actual mask resource stays paired with its scene image"
        );
        assert!(
            ops.iter().any(|op| op["PushLayer"]["alpha"]
                .as_f64()
                .is_some_and(|alpha| (alpha - 0.55).abs() < 0.00001)),
            "authored float opacity stays in the scene layer stack"
        );
    }
    #[test]
    fn identical_source_rectangles_keep_exact_appearance_owners() {
        let app = crate::app::App::test_stub();
        let frame = OuterFrame::default();
        let node = uuid::Uuid::from_u128(71);
        let slot = 2;
        let roles = [0x2000_0000_0000_0011, 0x2000_0000_0000_0022];
        let mut owners = Vec::new();
        {
            let mut dom = frame.controls.dom.borrow_mut();
            for role in roles {
                let owner = dom.create_element(cambium::html_qual("div"));
                dom.set_attribute(
                    owner,
                    cambium::attr_qual("data-native-node"),
                    &node.to_string(),
                );
                dom.set_attribute(
                    owner,
                    cambium::attr_qual("data-native-role"),
                    &role.to_string(),
                );
                dom.append_child(frame.controls.root(), owner);
                owners.push(owner);
            }
        }
        for (index, role) in roles.into_iter().enumerate() {
            let surface = Surface {
                id: SurfaceId::content_appearance(
                    node,
                    role ^ (slot as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
                ),
                kind: SurfaceKind::Content(node),
                rect: Rect::new(40.0, 60.0, 240.0, 180.0),
            };
            assert_eq!(
                frame.native_source_node(&app, &surface, slot),
                Some(owners[index])
            );
            assert_eq!(frame.native_source_node(&app, &surface, slot + 1), None);
        }
        let duplicate = frame
            .controls
            .dom
            .borrow_mut()
            .create_element(cambium::html_qual("div"));
        {
            let mut dom = frame.controls.dom.borrow_mut();
            dom.set_attribute(
                duplicate,
                cambium::attr_qual("data-native-node"),
                &node.to_string(),
            );
            dom.set_attribute(
                duplicate,
                cambium::attr_qual("data-native-role"),
                &roles[0].to_string(),
            );
            dom.append_child(frame.controls.root(), duplicate);
        }
        let ambiguous = Surface {
            id: SurfaceId::content_appearance(
                node,
                roles[0] ^ (slot as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
            ),
            kind: SurfaceKind::Content(node),
            rect: Rect::new(40.0, 60.0, 240.0, 180.0),
        };
        assert_eq!(frame.native_source_node(&app, &ambiguous, slot), None);
    }
}
