// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Retained shared Frisket furniture. Pane bodies keep their existing owners
//! during A4's draft; every aperture and header hit comes from this one DOM.
use crate::{
    panes::{FrisketLayout, PaneId},
    surface::Rect,
};
use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, Slot, WorkspaceModel,
    workspace_view,
};
use genet_scripted_dom::{NodeId, ScriptedDom};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use std::{cell::RefCell, rc::Rc};
use workbench::{DropTarget, Edge, TileId, TileTree, Workspace, WorkspaceEvent};

struct State {
    workspace: Workspace,
    current: Option<TileId>,
    events: Vec<WorkspaceEvent>,
}
type FrameView = Box<dyn AnyView<State, (), GenetCtx, GenetElement>>;
type Runner = GenetAppRunner<State, fn(&State) -> FrameView, FrameView, ()>;
fn view(state: &State) -> FrameView {
    Box::new(workspace_view(
        &WorkspaceModel {
            workspace: &state.workspace,
            current: state.current,
            float_layer_visible: true,
        },
        |state: &mut State, event| state.events.push(event),
        |_| Slot::Surface,
    ))
}

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
    dom: DomHandle,
    runner: Runner,
    layout: crate::ui::RetainedLayout,
    workspace: Workspace,
    current: Option<TileId>,
    size: (u32, u32),
    sheet: String,
    appearance: crate::shell_services::AppearanceConfig,
}
impl Default for OuterFrame {
    fn default() -> Self {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let workspace = Workspace::default();
        let runner = Runner::new(
            dom.clone(),
            view as fn(&State) -> FrameView,
            State {
                workspace: workspace.clone(),
                current: None,
                events: Vec::new(),
            },
        );
        let appearance = crate::shell_services::AppearanceConfig::default();
        let sheet = crate::frame_style::stylesheet(
            None,
            &appearance.theme_typography,
            "",
            appearance.zoom(),
        );
        Self {
            dom,
            runner,
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
    ) -> Result<Vec<(PaneId, Rect)>, String> {
        if &self.appearance != appearance {
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
        if self.workspace != workspace || self.current != current {
            self.runner.update(|state| {
                state.workspace = workspace.clone();
                state.current = current;
            });
            self.workspace = workspace;
            self.current = current;
        }
        self.size = size;
        self.scene();
        let dom = self.dom.borrow();
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
            &mut self.dom.borrow_mut(),
            &self.sheet,
            self.size.0,
            self.size.1,
        )
    }
    pub(crate) fn stylesheet(&self) -> &str {
        &self.sheet
    }
    pub(crate) fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }
    pub(crate) fn header_height(&self) -> f32 {
        let dom = self.dom.borrow();
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
            &mut self.dom.borrow_mut(),
            &self.sheet,
            self.size.0,
            self.size.1,
            x,
            y,
        )?;
        let dom = self.dom.borrow();
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
            &mut self.dom.borrow_mut(),
            &self.sheet,
            self.size.0,
            self.size.1,
            x,
            y,
        )?;
        let dom = self.dom.borrow();
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
