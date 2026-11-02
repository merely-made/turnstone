// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(in super::super) fn request_document_work_redraws(
        &self,
        work: &[pelt_core::PeltDocumentWork<super::super::content_pool::ContentKey>],
    ) {
        if work
            .iter()
            .any(|work| self.content_sessions.pool.window_has_document_work(0, work))
            && let Some(window) = self.window.as_ref()
        {
            window.request_redraw();
        }
        for lens in self.lens_windows.values() {
            if work.iter().any(|work| {
                self.content_sessions
                    .pool
                    .window_has_document_work(lens.ordinal as u64 + 1, work)
            }) {
                lens.window.request_redraw();
            }
        }
    }

    pub(in super::super) fn request_pending_document_redraws(&self) {
        if self.content_sessions.pool.window_needs_frame(0)
            && let Some(window) = self.window.as_ref()
        {
            window.request_redraw();
        }
        for lens in self.lens_windows.values() {
            if self
                .content_sessions
                .pool
                .window_needs_frame(lens.ordinal as u64 + 1)
            {
                lens.window.request_redraw();
            }
        }
    }

    pub(in super::super) fn poll_capture_readback(&mut self) {
        let Some(mut pending) = self.pending_capture_readback.take() else {
            return;
        };
        match pending.frame.poll() {
            Some(Ok(frame)) => {
                if let Err(error) = mesquite::write_png(&pending.path, &frame) {
                    self.main_receipt
                        .errors
                        .push(format!("capture {}: {error}", pending.path.display()));
                } else {
                    self.main_receipt.captures.push((
                        pending.path,
                        frame.digest(),
                        frame.is_blank(),
                    ));
                }
            },
            Some(Err(error)) => self
                .main_receipt
                .errors
                .push(format!("capture {}: {error}", pending.path.display())),
            None if pending.started.elapsed() > std::time::Duration::from_secs(5) => {
                self.main_receipt
                    .errors
                    .push("capture readback exceeded five seconds".into());
            },
            None => {
                self.pending_capture_readback = Some(pending);
                self.request_redraw();
            },
        }
    }

    pub(in super::super) fn request_redraw(&self) {
        self.native_frames.invalidate();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        for lens in self.lens_windows.values() {
            lens.window.request_redraw();
        }
    }
}
