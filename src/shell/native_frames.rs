// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Window scheduling only. Genet owns completion, attribution and loss checks.
use super::*;
use genet_render_host::PendingNativeFrame;
use std::{
    cell::Cell,
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub(super) struct Identity {
    pub window: WindowId,
    pub ordinal: usize,
    pub generation: u64,
    pub dimensions: (u32, u32),
    pub session: crate::panes::SessionId,
    pub custody: u64,
    pub scale: f32,
}
pub(super) struct StagedWindowFrame {
    pub identity: Identity,
    pub revision: u64,
    pub layers: Vec<CompositeLayer>,
    pub raster_textures: Vec<wgpu::Texture>,
    pub started: Instant,
    pub timings: crate::frame_timing::FrameTimings,
    pub raster_elapsed: Duration,
    pub surface_raster: crate::frame_timing::SurfaceRasterSplit,
    pub needs_redraw: bool,
    pub palette: bool,
    pub nodes: usize,
    pub capture: Option<std::path::PathBuf>,
    pub diagnostics: Vec<super::appearance_receipt::PendingCapture>,
}
pub(super) struct QueuedFrame {
    pub identity: Identity,
    pub pending: PendingNativeFrame<StagedWindowFrame>,
    pub capture: Option<std::path::PathBuf>,
}
#[derive(Default)]
pub(super) struct NativeFrameQueue {
    pending: HashMap<WindowId, QueuedFrame>,
    waiting: VecDeque<WindowId>,
    revision: Cell<u64>,
}
impl NativeFrameQueue {
    pub fn invalidate(&self) {
        self.revision.set(self.revision.get().wrapping_add(1));
    }
    pub fn revision(&self) -> u64 {
        self.revision.get()
    }
    pub fn has(&self, id: WindowId) -> bool {
        self.pending.contains_key(&id)
    }
    pub fn capture_pending(&self) -> bool {
        self.pending.values().any(|frame| frame.capture.is_some())
    }
    pub fn active(&self) -> bool {
        !self.pending.is_empty() || !self.waiting.is_empty()
    }
    pub fn wants(&self, id: WindowId) -> bool {
        self.has(id) || self.waiting.front() == Some(&id)
    }
    pub fn prepare(&mut self, id: WindowId) -> bool {
        if !self.waiting.contains(&id) {
            self.waiting.push_back(id);
        }
        self.pending.is_empty() && self.waiting.front() == Some(&id)
    }
    pub fn submit(
        &mut self,
        identity: Identity,
        pending: PendingNativeFrame<StagedWindowFrame>,
        capture: Option<std::path::PathBuf>,
    ) {
        self.waiting.retain(|id| *id != identity.window);
        self.pending.insert(
            identity.window,
            QueuedFrame {
                identity,
                pending,
                capture,
            },
        );
    }
    pub fn take(&mut self, id: WindowId) -> Option<QueuedFrame> {
        self.pending.remove(&id)
    }
    pub fn put(&mut self, queued: QueuedFrame) {
        self.pending.insert(queued.identity.window, queued);
    }
    pub fn cancel(&mut self, id: WindowId) {
        self.waiting.retain(|window| *window != id);
        self.pending.remove(&id);
    }
    pub fn clear(&mut self) {
        self.waiting.clear();
        self.pending.clear();
    }
}
impl Shell {
    pub(super) fn cancel_native_window(&mut self, ordinal: u64, reason: &str) {
        let id = if ordinal == 0 {
            self.window.as_ref().map(|window| window.id())
        } else {
            self.lens_windows
                .iter()
                .find(|(_, lens)| lens.ordinal as u64 + 1 == ordinal)
                .map(|(id, _)| *id)
        };
        if let Some(id) = id {
            if let Some(frame) = self.native_frames.take(id)
                && let Some(path) = &frame.capture
            {
                self.main_receipt.errors.push(format!(
                    "native capture {} refused: {reason}",
                    path.display()
                ));
            }
            self.native_frames.cancel(id);
        }
    }
    pub(super) fn cancel_native_frames(&mut self, reason: &str) {
        let captures = self
            .native_frames
            .pending
            .values()
            .filter_map(|frame| frame.capture.as_ref())
            .map(|path| format!("native capture {} refused: {reason}", path.display()))
            .collect::<Vec<_>>();
        self.main_receipt.errors.extend(captures);
        self.native_frames.clear();
    }
    // Child hosts retain the checked raster API. Their redraws participate in
    // this same host-owned queue, while ordinary input is delivered at once.
    fn native_child_windows(&self) -> Vec<Arc<winit::window::Window>> {
        let mut windows = Vec::new();
        if let Some(window) = self
            .theme_editor
            .as_ref()
            .and_then(|editor| editor.host.native_window())
        {
            windows.push(window.clone());
        }
        #[cfg(feature = "workshop")]
        if let Some(window) = self
            .alembic_workshop
            .as_ref()
            .and_then(|editor| editor.host.native_window())
        {
            windows.push(window.clone());
        }
        windows
    }
    pub(super) fn native_child_open_pending(&self) -> bool {
        self.host
            .as_ref()
            .is_some_and(|host| host.core().native_frame_busy())
    }
    pub(super) fn defer_native_child_open(&self) {
        // A canceled batch may still be retiring after its staging is gone.
        // Keep an event clock until the Core's actual admission interval ends.
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        self.request_native_frame_redraws();
    }
    pub(super) fn native_child_requested(&self) -> bool {
        self.theme_editor_requested || self.alembic_workshop_requested
    }
    pub(super) fn admit_native_child_redraw(&mut self, id: WindowId, event: &WindowEvent) -> bool {
        if !matches!(event, WindowEvent::RedrawRequested) {
            return true;
        }
        if self.native_frames.prepare(id) && !self.native_child_open_pending() {
            return true;
        }
        self.defer_native_child_open();
        false
    }
    pub(super) fn finish_native_child_redraw(&mut self, id: WindowId, event: &WindowEvent) {
        if matches!(event, WindowEvent::RedrawRequested) {
            self.native_frames.cancel(id);
            self.request_native_frame_redraws();
        }
    }
    pub(super) fn prune_native_frames(&mut self) {
        let obsolete = self
            .native_frames
            .pending
            .iter()
            .filter_map(|(id, frame)| (!self.native_frame_current(frame.identity)).then_some(*id))
            .collect::<Vec<_>>();
        for id in obsolete {
            if let Some(frame) = self.native_frames.take(id)
                && let Some(path) = &frame.capture
            {
                self.main_receipt.errors.push(format!(
                    "native capture {} refused: window, session, custody or generation changed",
                    path.display()
                ));
            }
            self.native_frames.cancel(id);
        }
        let children = self.native_child_windows();
        let absent = self
            .native_frames
            .waiting
            .iter()
            .copied()
            .filter(|id| {
                !self
                    .window
                    .as_ref()
                    .is_some_and(|window| window.id() == *id && self.host.is_some())
                    && !self
                        .lens_windows
                        .get(id)
                        .is_some_and(|lens| lens.host.is_some())
                    && !children.iter().any(|window| window.id() == *id)
            })
            .collect::<Vec<_>>();
        for id in absent {
            self.native_frames.cancel(id);
        }
    }

    pub(super) fn native_frame_identity(
        &self,
        id: WindowId,
        ordinal: usize,
        generation: u64,
        dimensions: (u32, u32),
    ) -> Identity {
        Identity {
            window: id,
            ordinal,
            generation,
            dimensions,
            session: self.app.session_id,
            scale: self.app.shell_chrome_config().layout_scale(ordinal),
            custody: self
                .custody_generation
                .load(std::sync::atomic::Ordering::Acquire),
        }
    }
    fn native_frame_current(&self, identity: Identity) -> bool {
        if identity.scale
            != self
                .app
                .shell_chrome_config()
                .layout_scale(identity.ordinal)
            || identity.session != self.app.session_id
            || identity.custody
                != self
                    .custody_generation
                    .load(std::sync::atomic::Ordering::Acquire)
        {
            return false;
        }
        if identity.ordinal == 0 {
            self.window
                .as_ref()
                .is_some_and(|window| window.id() == identity.window)
                && (self.width, self.height) == identity.dimensions
                && self
                    .host
                    .as_ref()
                    .is_some_and(|host| host.core().generation() == identity.generation)
        } else {
            self.lens_windows.get(&identity.window).is_some_and(|lens| {
                (lens.width, lens.height) == identity.dimensions
                    && lens.ordinal + 1 == identity.ordinal
                    && lens
                        .host
                        .as_ref()
                        .is_some_and(|host| host.core().generation() == identity.generation)
            })
        }
    }
    /// Returns (presented, continue rendering). Stage the next needed batch in
    /// this redraw, avoiding another semantic/event turn per presented frame.
    pub(super) fn poll_native_frame(&mut self, id: WindowId) -> (bool, bool) {
        let Some(mut queued) = self.native_frames.take(id) else {
            return (false, false);
        };
        let identity = queued.identity;
        if !self.native_frame_current(identity) {
            if let Some(path) = &queued.capture {
                self.main_receipt.errors.push(format!(
                    "native capture {} refused: obsolete staging",
                    path.display()
                ));
            }
            drop(queued);
            self.request_redraw();
            return (false, false);
        }
        let result = match queued.pending.poll() {
            None => {
                self.native_frames.put(queued);
                return (false, false);
            },
            Some(result) => result,
        };
        if result.is_err()
            && let Some(path) = &queued.capture
        {
            self.main_receipt.errors.push(format!(
                "native capture {} refused: GPU qualification failed",
                path.display()
            ));
        }
        let outcome = match result {
            Ok(accepted) => match accepted.publish(|staged| {
                let needs_redraw = staged.needs_redraw;
                let revision = staged.revision;
                let timings = staged.timings.clone();
                let capture = staged.capture.clone();
                let presented = if staged.identity.ordinal == 0 {
                    self.present_native_primary(staged)
                } else {
                    self.present_native_lens(staged)
                };
                if !presented && self.native_frame_current(identity) {
                    self.app.frame_timings.absorb(timings);
                    if let Some(path) = capture {
                        let vacant = if identity.ordinal == 0 {
                            self.pending_capture.is_none()
                        } else {
                            self.pending_lens_capture.is_none()
                        };
                        if vacant {
                            if identity.ordinal == 0 {
                                self.pending_capture = Some(path);
                            } else {
                                self.pending_lens_capture = Some(path);
                                self.pending_lens_capture_window = Some(identity.window);
                            }
                        } else {
                            self.main_receipt.errors.push(format!(
                                "native capture {} refused: a later request superseded the skipped presentation",
                                path.display()
                            ));
                        }
                    }
                    self.request_redraw();
                }
                let continue_render = presented && (
                    needs_redraw || self.native_frames.revision() != revision
                    || self.shared_scenario.is_some()
                );
                (presented, continue_render)
            }) {
                Ok(presented) => presented,
                Err(failure) => {
                    self.refuse_raster_frame(
                        identity.ordinal as u64,
                        identity.generation,
                        0,
                        failure,
                    );
                    (false, false)
                },
            },
            Err(failure) => {
                self.refuse_raster_frame(identity.ordinal as u64, identity.generation, 0, failure);
                (false, false)
            },
        };
        self.request_native_frame_redraws();
        outcome
    }
    pub(super) fn request_native_frame_redraws(&self) {
        for window in self.native_child_windows() {
            if self.native_frames.wants(window.id()) {
                window.request_redraw();
            }
        }
        if let Some(window) = &self.window
            && self.native_frames.wants(window.id())
        {
            window.request_redraw();
        }
        for (id, lens) in &self.lens_windows {
            if self.native_frames.wants(*id) {
                lens.window.request_redraw();
            }
        }
    }
}

#[cfg(test)]
#[path = "native_frame_queue_tests.rs"]
mod tests;
