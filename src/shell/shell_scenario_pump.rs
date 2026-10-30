// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn scenario_pump(&mut self, event_loop: &ActiveEventLoop) {
        if !self.main_receipt.errors.is_empty()
            && let Some(shared) = self.shared_scenario.take()
        {
            let mut outcome = shared.finish();
            outcome.ok = false;
            self.write_shared_done(&outcome);
            event_loop.exit();
            return;
        }
        if self.pending_capture.is_some()
            || self.native_frames.capture_pending()
            || self.pending_lens_capture.is_some()
            || self.pending_capture_readback.is_some()
            || !self.pending_native_diagnostics.is_empty()
        {
            self.request_redraw();
            return;
        }
        if let Some(wait) = self.pending_scenario_wait.as_mut() {
            match wait.poll(&self.app.place.status_lines()) {
                Ok(true) => self.pending_scenario_wait = None,
                Ok(false) => {
                    self.request_redraw();
                    return;
                },
                Err(error) => {
                    self.pending_scenario_wait = None;
                    if let Some(shared) = self.shared_scenario.take() {
                        let mut outcome = shared.finish();
                        outcome.ok = false;
                        outcome.log.push(error);
                        self.write_shared_done(&outcome);
                        event_loop.exit();
                    }
                    return;
                },
            }
        }
        // The shared taproot driver, when active, takes the frame: take the
        // scenario out (so `tick(self)` can borrow the Shell mutably), tick it,
        // put it back — or, on Done, write the `scenario.done` sentinel in
        // turnstone's format and exit. Mutually exclusive with the turnstone driver.
        if let Some(mut shared) = self.shared_scenario.take() {
            match self.finish_contributed_click() {
                Ok(true) => {
                    self.shared_scenario = Some(shared);
                    self.request_redraw();
                    return;
                },
                Err(error) => {
                    let mut outcome = shared.finish();
                    outcome.ok = false;
                    outcome.log.push(format!("scoped click failed: {error}"));
                    self.write_shared_done(&outcome);
                    event_loop.exit();
                    return;
                },
                Ok(false) => {},
            }
            use taproot::Progress;
            match shared.tick(self) {
                Progress::Done => {
                    let outcome = shared.finish();
                    self.write_shared_done(&outcome);
                    event_loop.exit();
                },
                Progress::Running => {
                    self.request_redraw();
                    self.shared_scenario = Some(shared);
                },
            }
            return;
        }
    }
}
