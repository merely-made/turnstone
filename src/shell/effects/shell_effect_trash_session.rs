// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn effect_trash_session(&mut self, effect: Effect) {
        match effect {
            Effect::TrashSession { closing, next } => {
                self.withdraw_all_user_agent_requests("session-closed");
                self.release_place_worker();
                let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
                self.bin_handle
                    .command(crate::recycle::BinCommand::Release(ack_tx));
                if ack_rx
                    .recv_timeout(std::time::Duration::from_millis(1500))
                    .is_err()
                {
                    tracing::warn!("bin release ack timed out; attempting the trash move anyway");
                }
                // The trail-memory store lives in the same session dir,
                // so it releases (and flushes) under the same handshake.
                let (trail_ack_tx, trail_ack_rx) = std::sync::mpsc::sync_channel(1);
                self.trail_handle
                    .command(crate::trail_memory::TrailCommand::Release(trail_ack_tx));
                if trail_ack_rx
                    .recv_timeout(std::time::Duration::from_millis(1500))
                    .is_err()
                {
                    tracing::warn!(
                        "trail memory release ack timed out; attempting the trash move anyway"
                    );
                }
                self.app.apply_trash(closing);
                self.content_sessions.clear();
                self.clear_surface_content();
                self.pending_surface_spawns.clear();
                self.refuse_lens_captures_before_clear("session trashed");
                self.lens_windows.clear();
                self.lens_divider_drag = None;
                self.pending_windows.clear();
                let fx = self.app.adopt_session(next);
                if !self.app.session_load_refused() {
                    self.bin_handle.command(crate::recycle::BinCommand::Reopen(
                        crate::recycle::bin_dir(&self.app.session_dir()),
                    ));
                    self.trail_handle
                        .command(crate::trail_memory::TrailCommand::Reopen {
                            directory: crate::trail_memory::memory_dir(&self.app.session_dir()),
                            capture_generation: self.app.source_capture_generation,
                        });
                } else {
                    let (bin_ack, _) = std::sync::mpsc::sync_channel(1);
                    let (trail_ack, _) = std::sync::mpsc::sync_channel(1);
                    self.bin_handle
                        .command(crate::recycle::BinCommand::Release(bin_ack));
                    self.trail_handle
                        .command(crate::trail_memory::TrailCommand::Release(trail_ack));
                }
                self.run_effects(fx);
                self.request_redraw();
            },
            Effect::SwitchSession { id } => {
                self.withdraw_all_user_agent_requests("session-switched");
                self.save_session();
                self.release_place_worker();
                self.content_sessions.clear();
                self.clear_surface_content();
                self.pending_surface_spawns.clear();
                self.refuse_lens_captures_before_clear("session switched");
                self.lens_windows.clear();
                self.lens_divider_drag = None;
                self.pending_windows.clear();
                let fx = self.app.adopt_session(id);
                // Re-point the bin actor at the adopted session's store;
                // it answers with THAT bin's list (the app cleared its
                // mirror in adopt_session). The trail memory re-points
                // with it (flushing the departing session's segments).
                if !self.app.session_load_refused() {
                    self.bin_handle.command(crate::recycle::BinCommand::Reopen(
                        crate::recycle::bin_dir(&self.app.session_dir()),
                    ));
                    self.trail_handle
                        .command(crate::trail_memory::TrailCommand::Reopen {
                            directory: crate::trail_memory::memory_dir(&self.app.session_dir()),
                            capture_generation: self.app.source_capture_generation,
                        });
                } else {
                    let (bin_ack, _) = std::sync::mpsc::sync_channel(1);
                    let (trail_ack, _) = std::sync::mpsc::sync_channel(1);
                    self.bin_handle
                        .command(crate::recycle::BinCommand::Release(bin_ack));
                    self.trail_handle
                        .command(crate::trail_memory::TrailCommand::Release(trail_ack));
                }
                self.run_effects(fx);
                self.request_redraw();
            },
            _ => unreachable!("dispatch group disagrees with exhaustive routing"),
        }
    }
}
