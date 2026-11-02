// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl LensPanePointerHost for Shell {
    fn lens_pane_admission(&self) -> Option<super::super::input::ContributedPointer> {
        self.contributed_pointer
    }
    fn lens_pane_admission_current(
        &self,
        admitted: super::super::input::ContributedPointer,
    ) -> bool {
        self.contributed_pointer_is_current(admitted)
    }
    fn lens_cancel_stale_admission(&mut self) {
        self.cancel_content_pointer();
    }
    fn lens_release_admission(&mut self, window: u64, x: f32, y: f32) {
        self.release_contributed_pointer(window, x, y);
    }
}
