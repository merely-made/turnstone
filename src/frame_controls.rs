// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Retained Frisket control dispatch. The host still owns Workspace mutation.
use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, Key, KeyEvent, NamedKey,
    PointerClick, Slot, WorkspaceModel,
};
use genet_scripted_dom::{NodeId, ScriptedDom};
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
use workbench::{TileEvent, TileId, TilePath, TileTree, Workspace, WorkspaceEvent};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    /// An actual in-house body node; input stays with its retained pane owner.
    Body {
        tile: TileId,
        node: NodeId,
    },
    Tab(TileId),
    Close(TileId),
    Divider {
        path: TilePath,
        index: usize,
        left: Vec<TileId>,
        right: Vec<TileId>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameRoute {
    pub admission: u64,
    pub revision: u64,
    pub control: Control,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FrameStamp {
    pub admission: u64,
    pub revision: u64,
    pub focus: Option<Control>,
}
struct State {
    workspace: Workspace,
    current: Option<TileId>,
    events: Vec<WorkspaceEvent>,
    mounts: std::collections::HashMap<TileId, cambium::RetainedMount>,
}
type View = Box<dyn AnyView<State, (), GenetCtx, GenetElement>>;
type Runner = GenetAppRunner<State, fn(&State) -> View, View, ()>;
fn view(s: &State) -> View {
    let mounts = s.mounts.clone();
    Box::new(cambium::workspace_view(
        &WorkspaceModel {
            workspace: &s.workspace,
            current: s.current,
            float_layer_visible: true,
        },
        |s: &mut State, event| s.events.push(event),
        move |tile| match mounts.get(&tile.id) {
            Some(mount) => Slot::View(Box::new(
                cambium::component(
                    mount.clone(),
                    |_| (),
                    |_, _, _| {},
                    |mount, _| Box::new(mount.view::<(), ()>()),
                    |_: &mut State, _: ()| {},
                )
                .probe_id(format!("pane:{}", tile.id.0)),
            )),
            None => Slot::Surface,
        },
    ))
}
static NEXT: AtomicU64 = AtomicU64::new(1);
pub(crate) struct FrameControls {
    pub dom: DomHandle,
    runner: Runner,
    scope: Option<u128>,
    admission: u64,
    revision: u64,
    focus: Option<Control>,
    body_cursor: u64,
}
impl Default for FrameControls {
    fn default() -> Self {
        let dom = Rc::new(RefCell::new(ScriptedDom::new()));
        let root = dom.borrow().document();
        Self::new_at(dom, root)
    }
}
impl FrameControls {
    pub(crate) fn withdraw_mounts(&self, source: NodeId) {
        for mount in self.runner.state().mounts.values() {
            mount.withdraw_from(source);
        }
    }
    pub(crate) fn inline_tiles(&self) -> std::collections::HashSet<TileId> {
        self.runner.state().mounts.keys().copied().collect()
    }
    pub(crate) fn new_at(dom: DomHandle, root: NodeId) -> Self {
        let runner = Runner::new_at(
            dom.clone(),
            root,
            view as fn(&State) -> View,
            State {
                workspace: Workspace::default(),
                current: None,
                events: Vec::new(),
                mounts: Default::default(),
            },
        );
        Self {
            dom,
            runner,
            scope: None,
            admission: NEXT.fetch_add(1, Ordering::Relaxed),
            revision: 0,
            focus: None,
            body_cursor: 0,
        }
    }
    pub(crate) fn set_mounts(
        &mut self,
        mounts: std::collections::HashMap<TileId, cambium::RetainedMount>,
    ) {
        let previous = &self.runner.state().mounts;
        let changed = previous.len() != mounts.len()
            || mounts.iter().any(|(id, mount)| {
                previous
                    .get(id)
                    .is_none_or(|old| old.node() != mount.node())
            });
        if changed {
            self.invalidate();
            self.runner.update(|s| s.mounts = mounts);
        }
    }
    pub(crate) fn root(&self) -> NodeId {
        self.runner.root()
    }
    pub fn sync(&mut self, scope: u128, workspace: Workspace, current: Option<TileId>) {
        if self.scope != Some(scope) {
            self.scope = Some(scope);
            self.admission = NEXT.fetch_add(1, Ordering::Relaxed);
            self.focus = None;
            self.runner.set_focus(None);
            self.invalidate();
        }
        if self.runner.state().workspace != workspace || self.runner.state().current != current {
            self.invalidate();
            self.runner.update(|s| {
                s.workspace = workspace;
                s.current = current;
                s.events.clear();
            });
        }
        self.enhance();
        self.observe_body_mutations();
        if self.focus.as_ref().is_some_and(|c| self.node(c).is_none()) {
            self.focus = self
                .order()
                .into_iter()
                .find(|c| matches!(c,Control::Tab(id) if Some(*id)==current))
                .or_else(|| {
                    self.order()
                        .into_iter()
                        .find(|c| matches!(c, Control::Tab(_)))
                });
        }
        self.apply_focus();
    }
    pub fn invalidate(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("frame revision exhausted");
    }
    pub(crate) fn observe_body_mutations(&mut self) {
        use layout_dom_api::DomMutation;
        let dom = self.dom.borrow();
        let (base, pending) = dom.pending_mutations();
        let end = base + pending.len() as u64;
        let start = self
            .body_cursor
            .saturating_sub(base)
            .min(pending.len() as u64) as usize;
        let belongs = |node| {
            dom.is_live(node)
                && self.runner.state().mounts.values().any(|mount| {
                    cambium_rootstock::WindowDom::new(&dom, mount.node()).contains(node)
                })
        };
        let changed = self.body_cursor < base
            || pending[start..].iter().any(|mutation| match mutation {
                DomMutation::Inserted { node, parent } => belongs(*node) || belongs(*parent),
                DomMutation::Removed {
                    node,
                    former_parent,
                } => belongs(*node) || belongs(*former_parent),
                DomMutation::Moved {
                    node,
                    from_parent,
                    to_parent,
                } => belongs(*node) || belongs(*from_parent) || belongs(*to_parent),
                DomMutation::AttributeChanged { node, .. }
                | DomMutation::CharacterDataChanged { node }
                | DomMutation::SubtreeReplaced { node }
                | DomMutation::FormControlStateChanged { node }
                | DomMutation::FormControlInteractionStateChanged { node }
                | DomMutation::OptionStateChanged { node }
                | DomMutation::FormControlCustomValidityChanged { node } => belongs(*node),
            });
        drop(dom);
        self.body_cursor = end;
        if changed {
            self.invalidate();
        }
    }
    pub fn stamp(&self) -> FrameStamp {
        FrameStamp {
            admission: self.admission,
            revision: self.revision,
            focus: self.focus.clone(),
        }
    }
    pub fn focus(&self) -> Option<&Control> {
        self.focus.as_ref()
    }
    pub fn focus_node(&self) -> Option<NodeId> {
        self.focus.as_ref().and_then(|c| self.node(c))
    }
    pub fn set_focus(&mut self, control: Option<Control>) -> bool {
        if control.as_ref().is_some_and(|c| self.node(c).is_none()) {
            return false;
        }
        self.focus = control;
        self.apply_focus();
        true
    }
    pub fn route(&self, control: Control) -> FrameRoute {
        FrameRoute {
            admission: self.admission,
            revision: self.revision,
            control,
        }
    }
    pub fn admits(&self, route: &FrameRoute) -> bool {
        route.admission == self.admission
            && route.revision == self.revision
            && self.node(&route.control).is_some()
    }
    pub fn control_at(&self, node: NodeId) -> Option<Control> {
        let dom = self.dom.borrow();
        for (tile, mount) in &self.runner.state().mounts {
            if cambium_rootstock::WindowDom::new(&dom, mount.node()).contains(node) {
                return Some(Control::Body { tile: *tile, node });
            }
        }
        if has_class(&dom, node, "frisket-close") {
            return cambium::close_target(&*dom, node).map(Control::Close);
        }
        if has_class(&dom, node, "frisket-tab") {
            return cambium::tab_target(&*dom, node).map(Control::Tab);
        }
        let target = cambium::divider_target(&*dom, node)?;
        if !has_class(&dom, node, "frisket-divider") {
            return None;
        }
        self.divider(&target.path, target.index)
    }
    pub fn node(&self, control: &Control) -> Option<NodeId> {
        nodes(&self.dom.borrow(), self.root())
            .into_iter()
            .find(|n| self.control_at(*n).as_ref() == Some(control))
    }
    pub fn order(&self) -> Vec<Control> {
        let dom = self.dom.borrow();
        nodes(&dom, self.root())
            .into_iter()
            .filter_map(|n| {
                let mut ancestor=Some(n);
                while let Some(at)=ancestor {
                    if attr(&dom,at,"data-main-view-header").as_deref()==Some("true") { return None; }
                    ancestor=dom.parent(at);
                }
                let control = self.control_at(n)?;
                if matches!(control, Control::Body { .. }) {
                    return None;
                }
                if matches!(control, Control::Tab(_))
                    && attr(&dom, n, "aria-selected").as_deref() != Some("true")
                {
                    return None;
                }
                Some(control)
            })
            .collect()
    }
    fn apply_focus(&mut self) {
        let node = self.focus_node();
        self.runner.set_focus(node);
        let mut dom = self.dom.borrow_mut();
        for n in nodes(&dom, self.root()) {
            if Some(n) == node {
                if attr(&dom, n, "data-frame-focus").as_deref() != Some("true") {
                    dom.set_attribute(n, qual("data-frame-focus"), "true");
                }
            } else if attr(&dom, n, "data-frame-focus").is_some() {
                dom.remove_attribute(n, qual("data-frame-focus"));
            }
        }
    }
    fn events(&mut self) -> Vec<WorkspaceEvent> {
        let mut events = Vec::new();
        self.runner
            .update(|s| events = std::mem::take(&mut s.events));
        events
    }
    pub fn click(&mut self, control: &Control) -> Vec<WorkspaceEvent> {
        let Some(node) = self.node(control) else {
            return Vec::new();
        };
        self.set_focus(Some(control.clone()));
        if matches!(control, Control::Divider { .. }) {
            return Vec::new();
        }
        self.runner
            .dispatch_click(node, PointerClick::at((0.0, 0.0)));
        self.events()
    }
    /// `visible` comes from the same retained fragments/semantics as paint.
    /// Returns an exit when Tab passes the first/last header control.
    pub fn key(&mut self, event: KeyEvent, visible: &[Control]) -> (Vec<WorkspaceEvent>, bool) {
        let Some(control) = self.focus.clone().filter(|c| visible.contains(c)) else {
            self.set_focus(None);
            return (Vec::new(), true);
        };
        if matches!(event.key, Key::Named(NamedKey::Escape)) {
            self.set_focus(None);
            return (Vec::new(), true);
        }
        if matches!(event.key, Key::Named(NamedKey::Tab)) {
            let index = visible.iter().position(|c| *c == control).unwrap();
            let next = if event.mods.shift {
                index.checked_sub(1)
            } else {
                index.checked_add(1).filter(|i| *i < visible.len())
            };
            self.set_focus(next.map(|i| visible[i].clone()));
            return (Vec::new(), next.is_none());
        }
        if (event.mods.ctrl || event.mods.meta)
            && matches!(&event.key,Key::Character(s) if s.eq_ignore_ascii_case("w"))
        {
            return (
                match control {
                    Control::Tab(id) | Control::Close(id) => vec![TileEvent::Closed(id).into()],
                    _ => Vec::new(),
                },
                false,
            );
        }
        if event.mods.alt
            && let Control::Tab(id) = control
            && let Some(float) = self
                .runner
                .state()
                .workspace
                .floating()
                .iter()
                .find(|f| f.tile.id == id)
        {
            let mut rect = float.rect;
            let step = if event.mods.ctrl { 0.1 } else { 0.01 };
            match event.key {
                Key::Named(NamedKey::ArrowLeft) => {
                    if event.mods.shift {
                        rect.width = (rect.width - step).max(0.01)
                    } else {
                        rect.x -= step
                    }
                },
                Key::Named(NamedKey::ArrowRight) => {
                    if event.mods.shift {
                        rect.width = (rect.width + step).min(1.0)
                    } else {
                        rect.x += step
                    }
                },
                Key::Named(NamedKey::ArrowUp) => {
                    if event.mods.shift {
                        rect.height = (rect.height - step).max(0.01)
                    } else {
                        rect.y -= step
                    }
                },
                Key::Named(NamedKey::ArrowDown) => {
                    if event.mods.shift {
                        rect.height = (rect.height + step).min(1.0)
                    } else {
                        rect.y += step
                    }
                },
                _ => return (Vec::new(), false),
            }
            rect.x = rect.x.clamp(0.0, (1.0 - rect.width).max(0.0));
            rect.y = rect.y.clamp(0.0, (1.0 - rect.height).max(0.0));
            return (
                vec![workbench::FloatEvent::SetRect { tile: id, rect }.into()],
                false,
            );
        }
        if let Control::Divider { .. } = control {
            if event.mods.ctrl || event.mods.alt || event.mods.meta {
                return (Vec::new(), false);
            }
            let row = self
                .divider_tree(&control)
                .is_some_and(|(axis, _)| axis == workbench::SplitAxis::Row);
            let step = if event.mods.shift { 0.1 } else { 0.01 };
            let delta = match event.key {
                Key::Named(NamedKey::ArrowLeft) if row => Some(-step),
                Key::Named(NamedKey::ArrowRight) if row => Some(step),
                Key::Named(NamedKey::ArrowUp) if !row => Some(-step),
                Key::Named(NamedKey::ArrowDown) if !row => Some(step),
                Key::Named(NamedKey::Home) => Some(-1.0),
                Key::Named(NamedKey::End) => Some(1.0),
                _ => None,
            };
            return (
                delta
                    .and_then(|d| self.resize(&control, d))
                    .into_iter()
                    .collect(),
                false,
            );
        }
        self.runner.dispatch_key(event);
        let events = self.events();
        if let Some(id) = events.iter().find_map(|e| match e {
            WorkspaceEvent::Tile(TileEvent::Activated(id)) => Some(*id),
            _ => None,
        }) {
            self.focus = Some(Control::Tab(id));
        }
        (events, false)
    }
    pub fn resize(&self, control: &Control, delta: f32) -> Option<WorkspaceEvent> {
        let Control::Divider { path, index, .. } = control else {
            return None;
        };
        let (_, children) = self.divider_tree(control)?;
        let mut fractions: Vec<_> = children.iter().map(|c| c.fraction).collect();
        let pair = fractions[*index] + fractions[*index + 1];
        let left = (fractions[*index] + pair * delta).clamp(pair * 0.05, pair * 0.95);
        fractions[*index] = left;
        fractions[*index + 1] = pair - left;
        Some(
            TileEvent::DividerMoved {
                split: path.clone(),
                fractions,
            }
            .into(),
        )
    }
    pub fn divider_value(&self, control: &Control) -> Option<f64> {
        let Control::Divider { index, .. } = control else {
            return None;
        };
        let (_, children) = self.divider_tree(control)?;
        Some(
            (100.0 * children[*index].fraction
                / (children[*index].fraction + children[*index + 1].fraction)) as f64,
        )
    }
    fn divider_tree(
        &self,
        control: &Control,
    ) -> Option<(workbench::SplitAxis, &[workbench::TileBranch])> {
        let Control::Divider {
            path,
            index,
            left,
            right,
        } = control
        else {
            return None;
        };
        let TileTree::Split { axis, children } =
            tree_at(self.runner.state().workspace.tiled(), path)?
        else {
            return None;
        };
        let ids = |t: &TileTree| t.tiles().into_iter().map(|t| t.id).collect::<Vec<_>>();
        if ids(&children.get(*index)?.tree) != *left
            || ids(&children.get(index + 1)?.tree) != *right
        {
            return None;
        }
        Some((*axis, children))
    }
    fn divider(&self, path: &TilePath, index: usize) -> Option<Control> {
        let TileTree::Split { children, .. } =
            tree_at(self.runner.state().workspace.tiled(), path)?
        else {
            return None;
        };
        let ids = |t: &TileTree| t.tiles().into_iter().map(|t| t.id).collect();
        Some(Control::Divider {
            path: path.clone(),
            index,
            left: ids(&children.get(index)?.tree),
            right: ids(&children.get(index + 1)?.tree),
        })
    }
    fn enhance(&self) {
        let controls: Vec<_> = nodes(&self.dom.borrow(), self.root())
            .into_iter()
            .map(|n| (n, self.control_at(n)))
            .collect();
        let mut dom = self.dom.borrow_mut();
        for (node, control) in controls {
            if matches!(control, Some(Control::Body { .. })) {
                continue;
            }
            if let Some(Control::Divider { index, .. }) = control.as_ref() {
                if let Some((_, children)) = self.divider_tree(control.as_ref().unwrap()) {
                    let pair = children[*index].fraction + children[*index + 1].fraction;
                    for (name, value) in [
                        ("tabindex", "0".into()),
                        ("aria-valuemin", "5".into()),
                        ("aria-valuemax", "95".into()),
                        (
                            "aria-valuenow",
                            (100.0 * children[*index].fraction / pair).to_string(),
                        ),
                        (
                            "aria-label",
                            format!("Resize panes at divider {}", index + 1),
                        ),
                    ] {
                        if attr(&dom, node, name).as_deref() != Some(&value) {
                            dom.set_attribute(node, qual(name), &value);
                        }
                    }
                }
            }
            if let Some(id) = attr(&dom, node, cambium::FRISKET_TILE_ATTR)
                .filter(|_| cambium::slot_kind(&*dom, node).is_some())
            {
                let label = id
                    .parse::<u64>()
                    .ok()
                    .and_then(|id| self.runner.state().workspace.find(TileId(id)))
                    .map(|tile| format!("{} pane content", tile.title))
                    .unwrap_or_else(|| "Pane content".into());
                if attr(&dom, node, "role").as_deref() != Some("group") {
                    dom.set_attribute(node, qual("role"), "group");
                }
                if attr(&dom, node, "aria-label").as_deref() != Some(label.as_str()) {
                    dom.set_attribute(node, qual("aria-label"), &label);
                }
            }
        }
    }
}
pub(crate) fn nodes(dom: &ScriptedDom, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        out.push(node);
        let children = dom.dom_children(node).collect::<Vec<_>>();
        pending.extend(children.into_iter().rev());
    }
    out
}
fn qual(name: &str) -> QualName {
    QualName::new(None, Namespace::default(), LocalName::from(name))
}
fn attr(dom: &ScriptedDom, node: NodeId, name: &str) -> Option<String> {
    dom.attribute(node, &Namespace::default(), &LocalName::from(name))
        .map(str::to_owned)
}
fn has_class(dom: &ScriptedDom, node: NodeId, class: &str) -> bool {
    attr(dom, node, "class").is_some_and(|c| c.split_whitespace().any(|c| c == class))
}
fn tree_at<'a>(mut tree: &'a TileTree, path: &TilePath) -> Option<&'a TileTree> {
    for index in &path.0 {
        let TileTree::Split { children, .. } = tree else {
            return None;
        };
        tree = &children.get(*index)?.tree;
    }
    Some(tree)
}
