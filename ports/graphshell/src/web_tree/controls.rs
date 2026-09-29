// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Graph controls use the same canvas operations as the existing product page.
use super::*;
use cambium::{PointerButton, PointerEvent, PointerPhase, button};

pub(super) fn toolbar(page: &TreePage) -> Child {
    let paused = page.shared.canvas.borrow().physics_paused();
    let commands = [
        (
            "Pan left",
            CanvasCommand::Pan {
                dx: -PAN_STEP,
                dy: 0.0,
            },
        ),
        (
            "Pan right",
            CanvasCommand::Pan {
                dx: PAN_STEP,
                dy: 0.0,
            },
        ),
        (
            "Pan up",
            CanvasCommand::Pan {
                dx: 0.0,
                dy: -PAN_STEP,
            },
        ),
        (
            "Pan down",
            CanvasCommand::Pan {
                dx: 0.0,
                dy: PAN_STEP,
            },
        ),
        ("Zoom in", CanvasCommand::Zoom { delta: ZOOM_STEP }),
        ("Zoom out", CanvasCommand::Zoom { delta: -ZOOM_STEP }),
        ("Fit graph", CanvasCommand::Fit),
        ("Restore arrangement", CanvasCommand::RestoreArrangement),
        (
            if paused {
                "Play physics"
            } else {
                "Pause physics"
            },
            CanvasCommand::TogglePhysics,
        ),
    ];
    let buttons: Vec<Child> = commands
        .into_iter()
        .map(|(label, command)| {
            Box::new(button(label, move |page: &mut TreePage, _| {
                command.apply(&mut page.shared.canvas.borrow_mut(), page.shared.size.get());
                page.shared.dirty.set(true);
            })) as Child
        })
        .collect();
    Box::new(
        el("nav", buttons)
            .attr("class", "tree-controls")
            .attr("aria-label", "Graph controls"),
    )
}

impl TreePage {
    pub(super) fn pointer(&mut self, event: PointerEvent) {
        if event.button != PointerButton::Primary {
            return;
        }
        let (x, y) = event.local;
        let mut canvas = self.shared.canvas.borrow_mut();
        match event.phase {
            PointerPhase::Down => {
                canvas.pointer_down(mere::canvas::PointerButton::Left, x, y);
            },
            PointerPhase::Move => {
                canvas.cursor_moved(x, y);
            },
            PointerPhase::Up => {
                canvas.pointer_up(mere::canvas::PointerButton::Left, x, y);
                self.picked = canvas.focused_url().map(str::to_owned);
            },
        }
        self.shared.dirty.set(true);
    }
}
