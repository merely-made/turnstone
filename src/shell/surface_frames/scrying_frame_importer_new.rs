// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(feature = "scry")]
impl ScryingFrameImporter {
    /// The same synchronizer must be installed on the WebView2 producer. Its
    /// shared handle also belongs in the Inker frame envelope. The host factory
    /// supplies it after construction, rather than accepting a raw spawn fence.
    pub(in super::super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        synchronizer: std::sync::Arc<Dx12FenceSynchronizer>,
    ) -> Self {
        let host = HostWgpuContext::new(device.clone(), queue.clone());
        Self {
            importer: WgpuTextureImporter::with_synchronizer(host, Box::new(synchronizer.clone())),
            synchronizer,
            stats: std::cell::Cell::new(ScryingFrameStats::default()),
        }
    }

    pub(in super::super) fn stats(&self) -> ScryingFrameStats {
        self.stats.get()
    }

    pub(in super::super) fn fence_handle(&self) -> u64 {
        self.synchronizer.shared_handle().0 as u64
    }
}
