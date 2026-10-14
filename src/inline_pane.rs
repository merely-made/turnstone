// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Retained inline presentation for a host pane with only a current label.
use cambium::{AnyView, GenetAppRunner, GenetCtx, GenetElement};
use genet_scripted_dom::NodeId;
struct State {
    label: String,
    width: u32,
    height: u32,
}
type View = Box<dyn AnyView<State, (), GenetCtx, GenetElement>>;
type Runner = GenetAppRunner<State, fn(&State) -> View, View>;
fn view(state: &State) -> View {
    Box::new(
        cambium::el::<_, State, ()>("div", state.label.clone())
            .attr("class", "pane")
            .attr(
                "style",
                format!(
                    "width:{}px;height:{}px;padding:12px;",
                    state.width, state.height
                ),
            ),
    )
}
pub(crate) struct InlinePane {
    runner: Runner,
    layout: crate::ui::RetainedLayout,
    // Drop the runner and its widget resources before final mount custody.
    mount: cambium::RetainedMount,
}
impl InlinePane {
    pub fn new(dom: cambium::DomHandle) -> Self {
        let mount = cambium::RetainedMount::new(dom.clone());
        let runner = Runner::new_at(
            dom,
            mount.node(),
            view as fn(&State) -> View,
            State {
                label: String::new(),
                width: 1,
                height: 1,
            },
        );
        Self {
            layout: crate::ui::RetainedLayout::new_at(Some(mount.node())),
            mount,
            runner,
        }
    }
    pub fn sync(&mut self, label: String, width: u32, height: u32) {
        self.runner.update(|s| {
            s.label = label;
            s.width = width;
            s.height = height;
        });
    }
    pub(crate) fn scene(&mut self, width: u32, height: u32) -> netrender::Scene {
        self.layout.scene_scrolled(
            &mut self.mount.dom().borrow_mut(),
            crate::ui::CAMBIUM_SHEET,
            width,
            height,
            &mut crate::ui::PaneScroll::new(),
        )
    }
    pub fn key_inline(&mut self, event: cambium::KeyEvent) -> Vec<()> {
        self.runner.dispatch_key(event)
    }
    pub fn blur_inline(&mut self) {
        self.runner.set_focus(None);
    }
    pub fn inline_accepts_text(&self) -> bool {
        false
    }
    pub fn inline_cursor_area(&self) -> Option<(f32, f32, f32, f32)> {
        None
    }
    pub fn inline_mount(&self) -> Option<cambium::RetainedMount> {
        Some(self.mount.clone())
    }
    pub(crate) fn unbind_inline(&mut self) {
        self.layout.unbind_inline();
    }
    pub fn bind_inline(&mut self, layout: &crate::ui::InlineLayoutHandle) {
        self.layout.bind_inline(layout, &self.mount);
    }
    pub fn inline_focus(&self) -> Option<NodeId> {
        self.runner.focus()
    }
    pub fn focus_inline(&mut self, _: NodeId) -> bool {
        false
    }
    pub fn inline_selector(
        &self,
        selector: &taproot::Selector,
    ) -> Result<Option<(f32, f32)>, &'static str> {
        self.layout
            .selector_point(&self.mount.dom().borrow(), selector)
    }
}
