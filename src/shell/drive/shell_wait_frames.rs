// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Re-check `condition` once per (simulated) frame for up to `frames`
/// attempts, sleeping briefly between attempts. Legacy row/document waits
/// are "sticky" over the SHARED taproot scenario loop's own
/// frame pump (`Scenario::tick` advances its step index unconditionally once
/// per call — see `taproot::scenario`), so there is no hook to hold a
/// single step across several of ITS frames. This polls within the one call
/// turnstone's `app_step` gets instead: background work (file writes, network
/// I/O) runs on other threads/tasks, so a short sleep between checks still
/// lets it progress. Returns true the moment `condition` holds.
impl Shell {
    /// A wait that holds the frame loop still needs the place worker's answers,
    /// which the loop would otherwise drain; drain them here between checks.
    pub(super) fn wait_frames(
        &mut self,
        frames: u32,
        mut condition: impl FnMut(&Shell) -> bool,
    ) -> bool {
        let attempts = frames.max(1);
        for attempt in 0..attempts {
            // A resolved citation is an App intent, independent of a surface's
            // next viewport sync. Poll the same native drain during waits.
            self.pump_research();
            while let Ok(update) = self.trail_rx.try_recv() {
                let effects = self.app.apply_update(update);
                self.run_effects(effects);
            }
            while let Ok(update) = self.place_rx.try_recv() {
                let effects = self.app.apply_update(update);
                self.run_effects(effects);
            }
            // Retained work must advance even when no window is rendering.
            // Pump folds completion events as well as document clocks; hidden
            // work can settle without requesting a paint.
            let _ = self.pump_visible_documents();
            self.refresh_knot_documents();
            if condition(self) {
                return true;
            }
            if attempt + 1 < attempts {
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        }
        false
    }
}
