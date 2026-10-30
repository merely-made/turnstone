// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl super::super::super::input::PrimaryPressAdmissionHost for PrimaryPaneCaptureFixture {
    fn pointer_press_action(&mut self, action: crate::action::Action) {
        let events = self.content.pointers[0].lock().unwrap();
        if self.content.window == 0 {
            assert_eq!(
                events
                    .iter()
                    .filter(|e| e.phase == PointerPhase::Cancel)
                    .count(),
                1,
                "primary interception must retire the old receiver BEFORE executing its App action"
            );
            assert_eq!(
                self.cancellation_capture,
                vec![Some(SurfaceKind::Content(self.content.nodes[0]))],
                "retirement must observe the old shell capture before it is cleared"
            );
        } else {
            assert!(
                events.iter().all(|e| e.phase != PointerPhase::Cancel),
                "a primary action must preserve foreign Lens custody"
            );
        }
        drop(events);
        self.content.app.update(action);
    }
    fn pointer_omnibar_open(&self) -> bool {
        self.content.app.omnibar.open
    }
    fn pointer_retire_primary_pointers(&mut self) {
        self.cancellation_capture.push(self.capture);
        TouchInput::default().cancel_window(&mut self.content.contents.pool, 0);
    }
}
