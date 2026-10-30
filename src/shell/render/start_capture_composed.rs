// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn start_capture_composed(
    host: &SurfaceHost,
    layers: &[CompositeLayer],
    w: u32,
    h: u32,
) -> Result<cambium_rootstock::PendingFrame, String> {
    let target = capture_target(host, layers, w, h)?;
    host.shared_core().start_rgba8_readback(&target, w, h)
}
