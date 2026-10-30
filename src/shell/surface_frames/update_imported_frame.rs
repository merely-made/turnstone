// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(in super::super) fn update_imported_frame(
    cached: &mut Option<ImportedSurfaceFrame>,
    frame: SurfaceFrame,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    #[cfg(feature = "scry")] scrying_importer: Option<&ScryingFrameImporter>,
) -> Result<(), SurfaceError> {
    #[cfg(feature = "servo")]
    if matches!(&frame.texture, NativeTextureHandle::OwnedPayload(payload)
        if payload.payload_kind() == super::super::servo::FRAME_KIND)
    {
        let result = update_servo_frame(cached, frame, device, queue);
        if result.is_err() {
            *cached = None;
        }
        return result;
    }
    #[cfg(feature = "weld")]
    if matches!(&frame.texture, NativeTextureHandle::OwnedPayload(payload)
        if payload.payload_kind() == "welding-0.15.native-frame")
    {
        let result = update_welding_frame(cached, frame, device, queue);
        if result.is_err() {
            *cached = None;
        }
        return result;
    }

    #[cfg(feature = "scry")]
    if matches!(&frame.texture, NativeTextureHandle::OwnedPayload(_)) {
        let result = update_scrying_frame(cached, frame, scrying_importer);
        if result.is_err() {
            // A reused allocation may already contain the producer's next
            // write. After a failed wait it cannot be sampled as an old frame.
            *cached = None;
        }
        return result;
    }

    #[cfg(feature = "weld")]
    return update_raw_frame(cached, frame, device, queue);

    #[cfg(not(feature = "weld"))]
    {
        let _ = (device, queue);
        close_if_transferred(&frame.texture);
        Err(SurfaceError::Unsupported(
            "Turnstone requires an owned frame from a registered browser importer".into(),
        ))
    }
}
