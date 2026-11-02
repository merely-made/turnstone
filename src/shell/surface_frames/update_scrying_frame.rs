// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(feature = "scry")]
pub(super) fn update_scrying_frame(
    cached: &mut Option<ImportedSurfaceFrame>,
    frame: SurfaceFrame,
    importer: Option<&ScryingFrameImporter>,
) -> Result<(), SurfaceError> {
    let importer = importer.ok_or_else(|| {
        SurfaceError::Unsupported("Scry owned-frame importer is not installed".into())
    })?;
    let fence_handle = importer.synchronizer.shared_handle().0 as u64;
    let (native, expected) = prepare_scrying_frame(frame, fence_handle)?;
    let mut stats = importer.stats.get();
    stats.frames = stats.frames.saturating_add(1);
    importer.stats.set(stats);
    if reuse_allocation(cached.as_ref().map(|existing| existing.identity), expected)? {
        // Reimporting is unnecessary for a reused allocation, but its new
        // paint has a new producer fence. Wait on every acquired frame.
        synchronize_reused_scrying_frame(importer.synchronizer.as_ref(), &native)?;
        stats.waits = stats.waits.saturating_add(1);
        importer.stats.set(stats);
        return Ok(());
    }
    // The importer waits on this same synchronizer before consuming custody.
    let imported = importer
        .importer
        .import_frame(native, &ImportOptions::default())
        .map_err(|error| {
            SurfaceError::FrameAcquisitionFailed(format!("Scry frame import failed: {error}"))
        })?;
    let actual = FrameIdentity {
        source: FrameSource::ScryOwned,
        epoch: imported.generation,
        width: imported.size.width,
        height: imported.size.height,
        format: imported.format,
    };
    if actual != expected {
        return Err(SurfaceError::FrameAcquisitionFailed(
            "Scry frame metadata changed across import".into(),
        ));
    }
    *cached = Some(ImportedSurfaceFrame {
        resource_epoch: expected.epoch,
        identity: expected,
        texture: imported.texture,
    });
    stats.imports = stats.imports.saturating_add(1);
    stats.waits = stats.waits.saturating_add(1);
    importer.stats.set(stats);
    Ok(())
}
