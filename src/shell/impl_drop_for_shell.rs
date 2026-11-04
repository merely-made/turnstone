// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Drop for Shell {
    fn drop(&mut self) {
        self.close_native_frame_workers();
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

impl Shell {
    /// Cancel host publication, then retire all completion workers while their
    /// devices and windows are still owned. Also used when the shell unwinds.
    pub(super) fn close_native_frame_workers(&mut self) {
        self.cancel_native_frames("host shutting down");
        let mut owners = Vec::new();
        if let Some(owner) = &self.render_recovery {
            owners.push(owner.clone());
        }
        if let Some(host) = &self.host {
            owners.push(host.shared_core().shared_recovery());
        }
        for lens in self.lens_windows.values() {
            if let Some(host) = &lens.host {
                owners.push(host.shared_core().shared_recovery());
            }
        }
        let mut closed = Vec::new();
        for owner in owners {
            let identity = Arc::as_ptr(&owner);
            if closed.contains(&identity) {
                continue;
            }
            closed.push(identity);
            if let Err(error) = owner.close_native_frames() {
                tracing::error!(%error, "native completion workers did not retire cleanly");
            }
        }
    }
}
