// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ArrangePane {
    pub fn new() -> Self {
        Self::new_in(None)
    }

    pub(crate) fn new_in(mount: Option<cambium::RetainedMount>) -> Self {
        let dom: DomHandle = mount
            .as_ref()
            .map(cambium::RetainedMount::dom)
            .unwrap_or_else(|| Rc::new(RefCell::new(ScriptedDom::new())));
        let state = ArrangeState {
            choice: Choice {
                layout: FREE_LAYOUT_ID.to_string(),
                readback: Readback::default(),
                status: None,
                refusal: None,
                saved_seed: false,
                regions: 0,
            },
            form: PhysicsForm::default(),
            form_note: String::new(),
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        let runner = ArrangeRunner::new_at(
            dom.clone(),
            mount
                .as_ref()
                .map_or_else(|| dom.borrow().document(), cambium::RetainedMount::node),
            arrange_view as fn(&ArrangeState) -> ArrangeView,
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

    pub(crate) fn key_inline(&mut self, event: cambium::KeyEvent) -> Vec<ArrangeIntent> {
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

    /// Mirror the active canvas's choice at the pane's size.
    pub fn sync(&mut self, app: &App, pane_w: f32, pane_h: f32) {
        self.sync_choice(Choice::of(app), pane_w, pane_h);
    }

    pub(crate) fn sync_for_pane(
        &mut self,
        app: &App,
        pane: crate::panes::PaneId,
        pane_w: f32,
        pane_h: f32,
    ) {
        self.sync_choice(
            Choice::for_graph(app, app.graph_for_pane(pane)),
            pane_w,
            pane_h,
        );
    }

    pub(super) fn sync_choice(&mut self, choice: Choice, pane_w: f32, pane_h: f32) {
        self.runner.update(|state| {
            if !state.choice.readback.same_composition(&choice.readback) {
                state.form.follow_composition(&choice.readback);
            }
            state.form.follow_roles(&choice.readback);
            state.form.follow_speed(&choice.readback);
            state.form.follow_physics(&choice.readback);
            state.choice = choice;
            state.viewport_w = pane_w;
            state.viewport_h = pane_h;
        });
    }

    /// The pane's scene at its size, under the host's cambium sheet.
    pub fn scene(&mut self, w: u32, h: u32, scale: f32) -> netrender::Scene {
        self.layout.set_scale(scale);
        self.layout.scene_scrolled(
            &mut self.dom.borrow_mut(),
            crate::ui::CAMBIUM_SHEET,
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

    /// The retained DOM, for the shared probe harness.
    pub fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }

    /// Route a click at pane-local `(x, y)` into the rows; the applied values
    /// come back as the intents the shell lowers.
    pub fn click(&mut self, x: f32, y: f32, w: u32, h: u32) -> Vec<ArrangeIntent> {
        let hit = self.layout.hit_test_scrolled(
            &mut self.dom.borrow_mut(),
            crate::ui::CAMBIUM_SHEET,
            w,
            h,
            x,
            y,
            &self.scroll,
        );
        match hit {
            Some(node) => self.runner.dispatch_click(node, PointerClick::at((x, y))),
            None => Vec::new(),
        }
    }
}
