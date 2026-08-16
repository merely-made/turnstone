//! Cambium projection of this device's resident receipt cards.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use cambium::{AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, clickable, el, text};
use genet_layout::{IncrementalLayout, ScrollOffsets};
use genet_scripted_dom::{NodeId, ScriptedDom};

use crate::device_receipts_service::{
    DeviceReceiptsService, DeviceReceiptsSnapshot, ReceiptCardView,
};

struct DeviceReceiptsPaneState {
    service: Option<Arc<DeviceReceiptsService>>,
    snapshot: DeviceReceiptsSnapshot,
    local_status: String,
    viewport_w: f32,
    viewport_h: f32,
}

type DeviceReceiptsPaneView = Box<dyn AnyView<DeviceReceiptsPaneState, (), GenetCtx, GenetElement>>;
type DeviceReceiptsPaneRunner = GenetAppRunner<
    DeviceReceiptsPaneState,
    fn(&DeviceReceiptsPaneState) -> DeviceReceiptsPaneView,
    DeviceReceiptsPaneView,
    (),
>;

fn refresh(state: &mut DeviceReceiptsPaneState, _: cambium::PointerClick) {
    if let Some(service) = &state.service {
        service.refresh();
        state.local_status = "Reading this device's cards…".into();
    } else {
        state.local_status = "The device receipts reader could not start.".into();
    }
}

fn card_view(card: &ReceiptCardView) -> DeviceReceiptsPaneView {
    let badges = if card.badges.is_empty() {
        String::new()
    } else {
        format!(" · {}", card.badges.join(" · "))
    };
    let mut rows: Vec<DeviceReceiptsPaneView> = vec![Box::new(
        el::<_, DeviceReceiptsPaneState, ()>("div", format!("{}{badges}", card.title))
            .attr("class", "list-section-title"),
    )];
    for (label, value) in &card.values {
        rows.push(Box::new(
            el::<_, DeviceReceiptsPaneState, ()>("div", format!("{label}: {value}"))
                .attr("class", "list-row"),
        ));
    }
    // A size here means the bytes were actually read out of the resident
    // store on the last refresh, not merely promised by the card.
    for (index, size) in card.capture_bytes.iter().enumerate() {
        rows.push(Box::new(
            el::<_, DeviceReceiptsPaneState, ()>(
                "div",
                format!("capture {}: {} bytes, readable", index + 1, size),
            )
            .attr("class", "list-row muted"),
        ));
    }
    Box::new(el::<_, DeviceReceiptsPaneState, ()>("div", rows).attr("class", "list-row"))
}

fn device_receipts_pane_view(state: &DeviceReceiptsPaneState) -> DeviceReceiptsPaneView {
    let status = if state.local_status.is_empty() {
        state.snapshot.status.clone()
    } else {
        state.local_status.clone()
    };
    let cards: Vec<DeviceReceiptsPaneView> = if state.snapshot.cards.is_empty() {
        vec![Box::new(el::<_, DeviceReceiptsPaneState, ()>(
            "div",
            "No cards to show.",
        ))]
    } else {
        state.snapshot.cards.iter().map(card_view).collect()
    };
    Box::new(
        el::<_, DeviceReceiptsPaneState, ()>(
            "div",
            (
                el::<_, DeviceReceiptsPaneState, ()>("div", "Device Receipts")
                    .attr("class", "list-section-title"),
                el::<_, DeviceReceiptsPaneState, ()>("div", status).attr("class", "list-row muted"),
                Box::new(clickable(
                    el::<_, DeviceReceiptsPaneState, ()>("button", text("Refresh"))
                        .attr("class", "setting-apply"),
                    refresh,
                )) as DeviceReceiptsPaneView,
                el::<_, DeviceReceiptsPaneState, ()>("div", cards),
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

/// Retained reader panel over the resident host's cards.
pub struct DeviceReceiptsPane {
    dom: DomHandle,
    runner: DeviceReceiptsPaneRunner,
}

impl DeviceReceiptsPane {
    pub fn new(service: Option<Arc<DeviceReceiptsService>>) -> Self {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = DeviceReceiptsPaneState {
            service,
            snapshot: DeviceReceiptsSnapshot::default(),
            local_status: String::new(),
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        let runner = DeviceReceiptsPaneRunner::new(
            dom.clone(),
            device_receipts_pane_view as fn(&DeviceReceiptsPaneState) -> DeviceReceiptsPaneView,
            state,
        );
        Self { dom, runner }
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

    pub fn scene(&self, w: u32, h: u32) -> netrender::Scene {
        crate::ui::scene_from_dom(&self.dom.borrow(), crate::ui::CAMBIUM_SHEET, w, h)
    }

    pub fn click(&mut self, x: f32, y: f32, w: u32, h: u32) {
        let hit = {
            let dom = self.dom.borrow();
            let layout =
                IncrementalLayout::new(&*dom, &[crate::ui::CAMBIUM_SHEET], w as f32, h as f32);
            layout.hit_test(&*dom, x, y, &ScrollOffsets::<NodeId>::default())
        };
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
