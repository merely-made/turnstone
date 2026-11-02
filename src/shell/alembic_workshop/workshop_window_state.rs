// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl WorkshopWindow {
    pub(super) fn state(&self) -> Option<&SurfaceState> {
        self.host.s.runner.as_ref().map(|runner| runner.state())
    }

    pub(super) fn update(&mut self, f: impl FnOnce(&mut SurfaceState)) {
        if let Some(runner) = self.host.s.runner.as_mut() {
            runner.update(f);
            // This auxiliary window is outside Shell::request_redraw's main
            // and lens fanout. Runner state mutation alone does not guarantee
            // another native RedrawRequested event while the host is idle.
            if let Some(native) = self.host.native_window() {
                native.request_redraw();
            }
        }
    }

    pub(super) fn status(&mut self, status: impl Into<String>, error: Option<String>) {
        let status = status.into();
        self.update(|state| {
            state.status = status;
            state.error = error;
        });
    }
}
