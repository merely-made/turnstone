// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Drop for Shell {
    fn drop(&mut self) {
        self.clip_owner.close();
        // Driver drop cancels and joins its worker. Keep App's session writer
        // leases alive until that join completes, including unwind paths;
        // normal window close persists shutdown before removing the window.
        drop(self.alembic_workshop.take());
        if let Some(binding) = self.custody_binding.take() {
            binding.bound.retire();
            binding.watcher.request_stop();
            drop(binding);
        }
        if let Some((setup, _)) = self.custody_setup.take() {
            setup.request_stop();
            drop(setup);
        }
        for watcher in &self.retired_watchers {
            watcher.request_stop();
        }
        self.retired_watchers.clear();
        for setup in &self.retired_setups {
            setup.request_stop();
        }
        self.retired_setups.clear();
    }
}
