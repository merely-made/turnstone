// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PrimaryPressAdmissionHost for Shell {
    fn pointer_press_action(&mut self, action: Action) {
        self.act(action);
    }
    fn pointer_omnibar_open(&self) -> bool {
        self.app.omnibar.open
    }
    fn pointer_retire_primary_pointers(&mut self) {
        self.pointer_cancel_primary_content();
    }
}
