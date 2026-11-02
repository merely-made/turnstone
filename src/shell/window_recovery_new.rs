// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl WindowRecovery {
    pub(super) fn new(
        owner: &genet_render_host::SharedRenderCore,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            subscription: owner.subscribe(wake),
            detached_generation: None,
            surface_failure: None,
        }
    }

    pub(super) fn detach(
        &mut self,
        owner: &genet_render_host::SharedRenderCore,
        generation: u64,
        can_replace: bool,
        release: impl FnOnce(),
    ) {
        if self.detached_generation != Some(generation) {
            release();
            self.detached_generation = Some(generation);
            self.surface_failure = None;
        }
        // A callback may still be cancelling readbacks on another thread.
        // Retry acknowledgement after it enters Waiting, without releasing twice.
        if can_replace {
            owner.acknowledge_detached(&self.subscription, generation);
        }
    }
}
