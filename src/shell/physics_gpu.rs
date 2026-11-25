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

use cartography::canvas::{PhysicsDevice, gpu_node_device_for, physics_device_for};

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

/// Whether the GPU node tier runs: `TURNSTONE_GPU_NODES=off` turns it off;
/// `on` or unset leaves it on wherever a device exists.
pub(super) fn nodes_enabled_from(value: Option<&str>) -> bool {
    !value.is_some_and(|text| text.trim().eq_ignore_ascii_case("off"))
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
        self.bind_node_device();
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
        self.unbind_node_device();
        if self.app.graph_runtimes.physics_device().is_some() {
            self.app.graph_runtimes.set_physics_device(None);
        }
    }
}

impl Shell {
    /// Give every graph runtime the node tier's device, built from the
    /// renderer's own handles. A no-op while bound, switched off, or without a
    /// host.
    fn bind_node_device(&mut self) {
        if self.app.graph_runtimes.node_device().is_some() {
            return;
        }
        if !nodes_enabled_from(std::env::var("TURNSTONE_GPU_NODES").ok().as_deref()) {
            return;
        }
        let Some(host) = self.host.as_ref() else {
            return;
        };
        let device = gpu_node_device_for(&host.renderer().wgpu_device.core);
        tracing::debug!(target: "turnstone::shell::render", "graph node tier device bound");
        self.app.graph_runtimes.set_node_device(Some(device));
    }

    /// Return every graph runtime to the DOM node path, as on device loss. The
    /// textures staged in the lost renderer went with it.
    fn unbind_node_device(&mut self) {
        self.staged_node_textures.clear();
        if self.app.graph_runtimes.node_device().is_some() {
            self.app.graph_runtimes.set_node_device(None);
        }
    }

    /// Stage a graph pane's node texture in the renderer under its key, so the
    /// scene's external-texture item resolves it. The canvas submitted its pass
    /// inside `frame`, so this staging pass, on the same queue, reads the
    /// finished texture. A pane that stopped producing one releases its key.
    pub(super) fn stage_graph_node_texture(&mut self, pane: crate::panes::PaneId) {
        let Some(host) = self.host.as_ref() else {
            return;
        };
        let renderer = host.renderer();
        match self.app.graph_pane_node_target(pane) {
            Some(target) => {
                renderer.stage_external_image(
                    netrender::external_image_key(target.key),
                    &target.view,
                    target.size,
                    netrender::SourceAlpha::Premultiplied,
                    target.generation,
                );
                self.staged_node_textures.insert(pane, target.key);
            },
            None => {
                if let Some(key) = self.staged_node_textures.remove(&pane) {
                    renderer.unregister_external_image(netrender::external_image_key(key));
                }
            },
        }
    }
}

#[cfg(test)]
#[path = "physics_gpu_tests.rs"]
mod tests;
