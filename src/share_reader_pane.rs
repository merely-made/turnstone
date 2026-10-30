// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Cambium projection for importing and reading one private Knot ticket.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, TextInput, clickable, el, lens,
    text, text_field_typed,
};
use genet_scripted_dom::ScriptedDom;
use layout_dom_api::LayoutDom;

use crate::share_reader_service::{KnotShareReaderService, SharedKnotSnapshot};

struct SharedKnotPaneState {
    service: Option<Arc<KnotShareReaderService>>,
    snapshot: SharedKnotSnapshot,
    ticket: TextInput,
    local_status: String,
    viewport_w: f32,
    viewport_h: f32,
}

type SharedKnotPaneView = Box<dyn AnyView<SharedKnotPaneState, (), GenetCtx, GenetElement>>;
type SharedKnotPaneRunner = GenetAppRunner<
    SharedKnotPaneState,
    fn(&SharedKnotPaneState) -> SharedKnotPaneView,
    SharedKnotPaneView,
    (),
>;

fn open_ticket(state: &mut SharedKnotPaneState, _: cambium::PointerClick) {
    let ticket = state.ticket.text().trim().to_string();
    if ticket.is_empty() {
        state.local_status = "Paste the private handoff ticket first.".into();
    } else if let Some(service) = &state.service {
        service.open(ticket);
        state.local_status = "Opening private share…".into();
    } else {
        state.local_status = "Private-share reader could not start on this device.".into();
    }
}

fn shared_knot_pane_view(state: &SharedKnotPaneState) -> SharedKnotPaneView {
    let document = state.snapshot.document.as_ref().map_or_else(
        || {
            Box::new(el::<_, SharedKnotPaneState, ()>(
                "div",
                "No shared document is open.",
            )) as SharedKnotPaneView
        },
        |document| {
            let body = String::from_utf8(document.body.clone())
                .unwrap_or_else(|_| "This shared document is not valid UTF-8 text.".into());
            Box::new(
                el::<_, SharedKnotPaneState, ()>(
                    "div",
                    (
                        el::<_, SharedKnotPaneState, ()>(
                            "div",
                            format!(
                                "{} · head {}",
                                document.media_type,
                                &knot::hex32(&document.operation)[..8]
                            ),
                        )
                        .attr("class", "list-row muted"),
                        el::<_, SharedKnotPaneState, ()>("pre", body).attr("class", "list-row"),
                    ),
                )
                .attr("class", "pane"),
            ) as SharedKnotPaneView
        },
    );
    let ticket_input = Box::new(lens(
        |ticket: &mut TextInput| text_field_typed(ticket),
        |state: &mut SharedKnotPaneState| &mut state.ticket,
    )) as SharedKnotPaneView;
    let status = if state.local_status.is_empty() {
        state.snapshot.status.clone()
    } else {
        state.local_status.clone()
    };
    Box::new(
        el::<_, SharedKnotPaneState, ()>(
            "div",
            (
                el::<_, SharedKnotPaneState, ()>("div", "Shared document")
                    .attr("class", "list-section-title"),
                el::<_, SharedKnotPaneState, ()>("div", state.snapshot.route.clone())
                    .attr("class", "list-row muted"),
                el::<_, SharedKnotPaneState, ()>(
                    "div",
                    format!("Reader key: {}", state.snapshot.reader_key),
                )
                .attr("class", "list-row"),
                el::<_, SharedKnotPaneState, ()>(
                    "div",
                    "Give this key to the publisher before they issue your private ticket.",
                )
                .attr("class", "list-row muted"),
                el::<_, SharedKnotPaneState, ()>("div", ticket_input).attr("class", "setting-row"),
                Box::new(clickable(
                    el::<_, SharedKnotPaneState, ()>("button", text("Open private share"))
                        .attr("class", "setting-apply"),
                    open_ticket,
                )) as SharedKnotPaneView,
                el::<_, SharedKnotPaneState, ()>("div", status).attr("class", "list-row muted"),
                document,
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

/// Retained local reader panel. A ticket is held only in this runner's live
/// text input, never in a session projection.
pub struct SharedKnotPane {
    dom: DomHandle,
    runner: SharedKnotPaneRunner,
    scroll: crate::ui::PaneScroll,
    /// Kept across frames. Rebuilding a layout per paint re-cascaded and
    /// re-shaped the whole pane to draw an unchanged screen; see
    /// [`crate::ui::RetainedLayout`] for the measurement.
    layout: crate::ui::RetainedLayout,
    // Drop the runner and its widget resources before final mount custody.
    mount: Option<cambium::RetainedMount>,
}

impl SharedKnotPane {
    pub fn new(service: Option<Arc<KnotShareReaderService>>) -> Self {
        Self::new_in(None, service)
    }

    pub(crate) fn new_in(
        mount: Option<cambium::RetainedMount>,
        service: Option<Arc<KnotShareReaderService>>,
    ) -> Self {
        let dom: DomHandle = mount
            .as_ref()
            .map(cambium::RetainedMount::dom)
            .unwrap_or_else(|| Rc::new(RefCell::new(ScriptedDom::new())));
        let state = SharedKnotPaneState {
            service,
            snapshot: SharedKnotSnapshot::default(),
            ticket: TextInput::new(String::new()),
            local_status: String::new(),
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        let runner = SharedKnotPaneRunner::new_at(
            dom.clone(),
            mount
                .as_ref()
                .map_or_else(|| dom.borrow().document(), cambium::RetainedMount::node),
            shared_knot_pane_view as fn(&SharedKnotPaneState) -> SharedKnotPaneView,
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

    pub fn sync(&mut self, pane_w: f32, pane_h: f32) {
        self.runner.update(|state| {
            if let Some(service) = &state.service {
                state.snapshot = service.snapshot();
                state.local_status.clear();
            }
            state.viewport_w = pane_w;
            state.viewport_h = pane_h;
        });
    }

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
    pub(crate) fn key_inline(&mut self, event: cambium::KeyEvent) {
        let _: Vec<()> = self.runner.dispatch_key(event);
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

    /// Wheel delta from the shell.
    pub fn scroll_by(&mut self, dx: f32, dy: f32) {
        self.scroll.nudge(dx, dy);
    }

    /// Whether the overlay bars still need repainting as they fade.
    pub fn bars_visible(&mut self) -> bool {
        self.scroll.bars_visible()
    }

    pub fn click(&mut self, x: f32, y: f32, w: u32, h: u32) {
        let hit = self.layout.hit_test_scrolled(
            &mut self.dom.borrow_mut(),
            crate::ui::CAMBIUM_SHEET,
            w,
            h,
            x,
            y,
            &self.scroll,
        );
        if let Some(node) = hit {
            let _: Vec<()> = self
                .runner
                .dispatch_click(node, cambium::PointerClick::at((x, y)));
        }
    }

    pub fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layout_dom_api::LayoutDomMut;

    #[test]
    fn inline_ticket_field_routes_keys_composition_and_caret_in_shared_layout() {
        let dom = Rc::new(RefCell::new(ScriptedDom::new()));
        let document = dom.borrow().document();
        let mount = cambium::RetainedMount::new(dom.clone());
        dom.borrow_mut().append_child(document, mount.node());
        let mut pane = SharedKnotPane::new_in(Some(mount), None);
        pane.sync(640.0, 480.0);
        let layout = Rc::new(RefCell::new(crate::ui::RetainedLayout::new_at(Some(
            document,
        ))));
        pane.bind_inline(&layout);
        layout
            .borrow_mut()
            .scene(&mut dom.borrow_mut(), crate::ui::CAMBIUM_SHEET, 640, 480);
        let target = taproot::matching_estimated_fixture(
            &*dom.borrow(),
            &taproot::Selector::role("textbox"),
        )[0];
        assert!(pane.focus_inline(target));
        layout
            .borrow_mut()
            .scene(&mut dom.borrow_mut(), crate::ui::CAMBIUM_SHEET, 640, 480);
        assert!(
            pane.inline_accepts_text(),
            "the measured field owns editable semantics"
        );
        pane.key_inline(cambium::KeyEvent::new(cambium::Key::Character(
            "fixture-".into(),
        )));
        pane.key_inline(cambium::KeyEvent::new(cambium::Key::Composition(
            cambium::CompositionEvent::Preedit {
                text: "retained".into(),
                selection: Some((0, 8)),
            },
        )));
        assert_eq!(pane.runner.state().ticket.text(), "fixture-");
        pane.key_inline(cambium::KeyEvent::new(cambium::Key::Composition(
            cambium::CompositionEvent::Commit("retained".into()),
        )));
        assert_eq!(pane.runner.state().ticket.text(), "fixture-retained");
        layout
            .borrow_mut()
            .scene(&mut dom.borrow_mut(), crate::ui::CAMBIUM_SHEET, 640, 480);
        assert!(
            pane.inline_cursor_area().is_some(),
            "native candidate placement reads the actual shared caret"
        );
        pane.blur_inline();
        assert!(!pane.inline_accepts_text());
        assert_eq!(
            pane.runner.state().ticket.text(),
            "fixture-retained",
            "blur preserves controlled text"
        );
    }
}
