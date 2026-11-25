// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The native graph canvases' repulsion device (physics catalog plan, P5c-T).
//!
//! One `PhysicsDevice` is built from the renderer's own wgpu handles, never a
//! device of its own, and given to every graph runtime through the pool. A
//! lost device clears it, and the canvases fall back to the CPU/rapier path;
//! the next host brings a fresh one. The node count at which a simulation
//! starts using the device is `TURNSTONE_GPU_PHYSICS`: a number (0 is always),
//! `off`, or unset for [`NATIVE_THRESHOLD`].

use cartography::canvas::{PhysicsDevice, physics_device_for};

use super::Shell;

/// The native crossover from the lagged seam's per-tick cost, one step stale.
pub(super) const NATIVE_THRESHOLD: usize = 400;
const MAX_STALE_STEPS: u32 = 1;

/// The node count at which the device takes over, or `None` for CPU only.
pub(super) fn threshold_from(value: Option<&str>) -> Option<usize> {
    match value.map(str::trim) {
        None | Some("") => Some(NATIVE_THRESHOLD),
        Some(text) if text.eq_ignore_ascii_case("off") => None,
        Some(text) => Some(text.parse().unwrap_or(NATIVE_THRESHOLD)),
    }
}

pub(super) fn device_for(
    handles: &netrender::WgpuHandles,
    threshold: usize,
) -> PhysicsDevice {
    physics_device_for(handles)
        .with_threshold(threshold)
        .with_max_stale_steps(MAX_STALE_STEPS)
}

impl Shell {
    /// Build the device from the primary host's renderer and hand it to every
    /// graph runtime. A no-op while a device is bound or the host is absent.
    pub(super) fn bind_physics_device(&mut self) {
        if self.app.graph_runtimes.physics_device().is_some() {
            return;
        }
        let Some(host) = self.host.as_ref() else {
            return;
        };
        let Some(threshold) = threshold_from(std::env::var("TURNSTONE_GPU_PHYSICS").ok().as_deref())
        else {
            return;
        };
        let device = device_for(&host.renderer().wgpu_device.core, threshold);
        tracing::debug!(target: "turnstone::shell::render", threshold, "graph physics device bound");
        self.app.graph_runtimes.set_physics_device(Some(device));
    }

    /// Return every graph runtime to the CPU path, as on device loss.
    pub(super) fn unbind_physics_device(&mut self) {
        if self.app.graph_runtimes.physics_device().is_some() {
            self.app.graph_runtimes.set_physics_device(None);
        }
    }
}

#[cfg(test)]
#[path = "physics_gpu_tests.rs"]
mod tests;
