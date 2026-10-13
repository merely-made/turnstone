// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Retained shared Frisket furniture. Pane bodies keep their existing owners
//! during A4's draft; every aperture and header hit comes from this one DOM.
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
    pub(crate) controls: FrameControls,
    layout: crate::ui::RetainedLayout,
    workspace: Workspace,
    current: Option<TileId>,
    size: (u32, u32),
    sheet: String,
    appearance: crate::shell_services::AppearanceConfig,
}
impl Default for OuterFrame {
    fn default() -> Self {
        let controls = FrameControls::default();
        let workspace = Workspace::default();
        let appearance = crate::shell_services::AppearanceConfig::default();
        let sheet = crate::frame_style::stylesheet(
            None,
            &appearance.theme_typography,
            "",
            appearance.zoom(),
        );
        Self {
            controls,
            workspace,
            current: None,
            size: (1, 1),
            sheet,
            appearance,
            layout: crate::ui::RetainedLayout::new(),
        }
    }
}
impl OuterFrame {
    pub fn prepare(
        &mut self,
        layout: &FrisketLayout,
        current: Option<PaneId>,
        maximize: Option<PaneId>,
        size: (u32, u32),
        appearance: &crate::shell_services::AppearanceConfig,
        session: u128,
    ) -> Result<Vec<(PaneId, Rect)>, String> {
        if &self.appearance != appearance || self.size != size {
            self.controls.invalidate();
            self.sheet = crate::frame_style::stylesheet(
                appearance.theme_presentation.as_ref(),
                &appearance.theme_typography,
                &appearance.interface_font_properties,
                appearance.zoom(),
            );
            self.appearance = appearance.clone();
        }
        let frame = layout.outer_workspace()?;
        let mut workspace = frame.workspace.clone();
        if let Some(id) = maximize {
            if let Some(tile) = workspace.find(TileId(id.0)).cloned() {
                workspace = Workspace::new(TileTree::single(tile));
            }
        }
        let current = current.map(|id| TileId(id.0));
        self.controls.sync(session, workspace.clone(), current);
        self.workspace = workspace;
        self.current = current;
        self.size = size;
        self.scene();
        let dom = self.controls.dom.borrow();
        let mut out = Vec::new();
        for node in nodes(&dom) {
            if let Some(id) =
                attr(&dom, node, cambium::FRISKET_TILE_ATTR).and_then(|s| s.parse::<u64>().ok())
            {
                if let Some((x, y, w, h)) = self.layout.visible_rect(&dom, node) {
                    out.push((PaneId(id), Rect::new(x, y, w, h)));
                }
            }
        }
        Ok(out)
    }
    pub fn scene(&mut self) -> netrender::Scene {
        self.layout.scene(
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
        self.layout.visible_rect(&dom, node).is_some()
            && self
                .layout
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
        let focus = focused.then(|| self.controls.focus_node()).flatten();
        let (tree, map) = self.layout.accessibility_tree(&dom, focus)?;
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
    pub(crate) fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.controls.dom.borrow()
    }
    pub(crate) fn header_height(&self) -> f32 {
        let dom = self.controls.dom.borrow();
        nodes(&dom)
            .into_iter()
            .filter(|n| {
                attr(&dom, *n, "class")
                    .is_some_and(|class| class.split_whitespace().any(|c| c == "frisket-tabbar"))
            })
            .filter_map(|n| self.layout.painted_rect(&dom, n).map(|(_, _, _, h)| h))
            .fold(0.0, f32::max)
    }
    pub fn hit(&mut self, x: f32, y: f32) -> Option<HeaderHit> {
        let hit = self.layout.hit_test(
            &mut self.controls.dom.borrow_mut(),
            &self.sheet,
            self.size.0,
            self.size.1,
            x,
            y,
        )?;
        let dom = self.controls.dom.borrow();
        if let Some(id) = cambium::close_target(&*dom, hit) {
            return Some(HeaderHit::Close(id));
        }
        if let Some(id) = cambium::tab_target(&*dom, hit) {
            return Some(HeaderHit::Tab(id));
        }
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
        let (x, y, w, h) = self.layout.painted_rect(&dom, parent)?;
        let branches = dom
            .dom_children(parent)
            .into_iter()
            .filter(|n| {
                attr(&dom, *n, "class")
                    .is_some_and(|class| class.split_whitespace().any(|c| c == "frisket-branch"))
            })
            .map(|n| self.layout.painted_rect(&dom, n))
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
        let hit = self.layout.hit_test(
            &mut self.controls.dom.borrow_mut(),
            &self.sheet,
            self.size.0,
            self.size.1,
            x,
            y,
        )?;
        let dom = self.controls.dom.borrow();
        if let Some((stack, index)) =
            cambium::tab_drop_index(&*dom, hit, x, |node| self.layout.painted_rect(&dom, node))
        {
            return Some(DropTarget::Stack { stack, index });
        }
        let mut node = hit;
        loop {
            if let Some(id) =
                attr(&dom, node, cambium::FRISKET_TILE_ATTR).and_then(|s| s.parse::<u64>().ok())
            {
                let (rx, ry, w, h) = self.layout.painted_rect(&dom, node)?;
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
fn nodes(dom: &ScriptedDom) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut todo = vec![dom.document()];
    while let Some(node) = todo.pop() {
        out.push(node);
        todo.extend(dom.dom_children(node));
    }
    out
}
