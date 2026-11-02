// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Drop for LiveLanes {
    fn drop(&mut self) {
        // Explicit because the watcher outlives nothing else usefully: it
        // holds an Emitter, and a tick landing after the place closed would
        // ask the app to resync a place that is gone.
        let runtime = self._runtime.take().expect("lane runtime is taken once");
        // Before the transport closes: the accept loop holds a clone of it,
        // and a session already served releases its own slot when its stream
        // ends, exactly as it would have anyway.
        if let Some(accept) = self.projection.as_mut().and_then(ProjectionServing::abort) {
            runtime.block_on(async {
                let _ = accept.await;
            });
        }
        self.projection = None;
        if let Some(watcher) = self.watcher.take() {
            watcher.abort();
            runtime.block_on(async {
                let _ = watcher.await;
            });
        }
        self.joined = None;
        // The transport owns an iroh endpoint whose Drop path aborts
        // ungracefully. The place worker is synchronous, but retains the
        // lane runtime precisely so this owner can await endpoint shutdown
        // before the runtime and transport are dropped.
        if let Err(error) = runtime.block_on(self._transport.close()) {
            tracing::warn!(%error, "place transport did not close cleanly");
        }
        // Dropping the runtime only detaches its tasks; each still holds a
        // store clone, and redb's file lock with it. Shut down and wait.
        runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    }
}
