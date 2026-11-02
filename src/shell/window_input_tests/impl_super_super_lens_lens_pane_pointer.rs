// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl super::super::super::lens::LensPanePointerHost for LensPaneCustodyFixture {
    fn lens_pane_admission(&self) -> Option<super::super::super::input::ContributedPointer> {
        self.admitted
    }
    fn lens_pane_admission_current(
        &self,
        admitted: super::super::super::input::ContributedPointer,
    ) -> bool {
        admitted.is_current(
            self.actual_owner_plan(admitted.window)
                .into_iter()
                .find(|surface| surface.id == admitted.surface.id),
            Some(self.generation),
            admitted.window,
        )
    }
    fn lens_cancel_stale_admission(&mut self) {
        panic!("these controls qualify valid custody, not existing global stale cleanup");
    }
    fn lens_release_admission(&mut self, window: u64, x: f32, y: f32) {
        let Some(admitted) = self.admitted else {
            return;
        };
        if admitted.window != window {
            return;
        }
        assert!(
            super::super::super::lens::LensPanePointerHost::lens_pane_admission_current(
                self, admitted
            )
        );
        self.admitted = None;
        self.capture = None;
        self.pane_events.push((
            PointerPhase::Up,
            window,
            (x - admitted.surface.rect.x, y - admitted.surface.rect.y),
        ));
    }
}
