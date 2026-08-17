//! Cambium projection for importing and reading one private Knot ticket.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, TextInput, clickable, el, lens,
    text, text_field_typed,
};
use genet_layout::{IncrementalLayout, ScrollOffsets};
use genet_scripted_dom::{NodeId, ScriptedDom};
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
                el::<_, SharedKnotPaneState, ()>("div", "Shared Knot")
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
}

impl SharedKnotPane {
    pub fn new(service: Option<Arc<KnotShareReaderService>>) -> Self {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = SharedKnotPaneState {
            service,
            snapshot: SharedKnotSnapshot::default(),
            ticket: TextInput::new(String::new()),
            local_status: String::new(),
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        let runner = SharedKnotPaneRunner::new(
            dom.clone(),
            shared_knot_pane_view as fn(&SharedKnotPaneState) -> SharedKnotPaneView,
            state,
        );
        Self {
            dom,
            runner,
            scroll: crate::ui::PaneScroll::new(),
            layout: crate::ui::RetainedLayout::new(),
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

    pub fn scene(&mut self, w: u32, h: u32) -> netrender::Scene {
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
