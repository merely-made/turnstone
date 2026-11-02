// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl TurnstoneServoSurface {
    pub(super) fn view(&self) -> &WebView {
        self.view.as_ref().expect("live Servo surface has a view")
    }
}
