// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Drop for InferenceService {
    fn drop(&mut self) {
        // Close the sender before joining. Keep ownership of shutdown and the
        // join result; worker retirement and UI responsiveness remain bounded
        // by the worker's existing request deadlines.
        let (closed, _) = mpsc::sync_channel(1);
        self.commands = closed;
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
    }
}
