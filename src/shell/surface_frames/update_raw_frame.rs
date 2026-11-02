// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(feature = "weld")]
pub(super) fn update_raw_frame(
    cached: &mut Option<ImportedSurfaceFrame>,
    frame: SurfaceFrame,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<(), SurfaceError> {
    let NativeTextureHandle::D3d12Shared { handle, .. } = &frame.texture else {
        return Err(SurfaceError::Unsupported(
            "Turnstone's Weld importer supports Windows D3D12 shared textures only".into(),
        ));
    };
    if *handle == 0 || !matches!(&frame.sync, inker::SurfaceSyncHandle::None) {
        close_if_transferred(&frame.texture);
        return Err(SurfaceError::FrameAcquisitionFailed(
            "Weld's raw callback frame requires a valid handle and completed synchronization"
                .into(),
        ));
    }
    let format = match map_texture_format(&frame.format) {
        Ok(format) => format,
        Err(error) => {
            close_if_transferred(&frame.texture);
            return Err(error);
        },
    };
    let identity = FrameIdentity {
        source: FrameSource::RawDx12,
        epoch: frame.resource_epoch,
        width: frame.width,
        height: frame.height,
        format,
    };
    let reuse = match reuse_allocation(cached.as_ref().map(|existing| existing.identity), identity)
    {
        Ok(reuse) => reuse,
        Err(error) => {
            close_if_transferred(&frame.texture);
            return Err(error);
        },
    };
    if reuse {
        // A reusable producer may emit several paints for one allocation. Its
        // existing imported texture sees those writes directly. A transferred
        // handle is never reusable; close a malformed duplicate rather than
        // turning it into a per-frame handle leak.
        close_if_transferred(&frame.texture);
        return Ok(());
    }

    let NativeTextureHandle::D3d12Shared { handle, ownership } = frame.texture else {
        return Err(SurfaceError::Unsupported(
            "Turnstone's first surface importer supports Windows D3D12 shared textures only".into(),
        ));
    };
    if handle == 0 || frame.width == 0 || frame.height == 0 {
        close_transferred_handle(handle, ownership);
        return Err(SurfaceError::FrameAcquisitionFailed(
            "surface producer emitted an invalid D3D12 frame".into(),
        ));
    }

    let texture = if ownership == FrameHandleOwnership::Transferred {
        // Weld's callback copier has a D3D11-to-D3D12 cache-visible import
        // finish that is specific to CEF's shared texture path. Consume the
        // transferred handle through that helper; its frame Drop closes the
        // Win32 handle after OpenSharedHandle takes a resource reference.
        //
        // wgpu-weld 881f340/f34f95b made `Dx12SharedTexture`'s fields private
        // in favour of a constructor: `from_owned_raw_handle` takes ownership
        // of the raw handle the same way the old struct literal did, so the
        // Drop-closes-handle behaviour this comment describes is unchanged.
        //
        // SAFETY: `handle` is non-null (checked above) and, per
        // `FrameHandleOwnership::Transferred`, is an owned Win32 shared-texture
        // handle this host is responsible for closing.
        let native = unsafe {
            welding::native_frame::Dx12SharedTexture::from_owned_raw_handle(
                handle as *mut std::ffi::c_void,
                PhysicalSize::new(frame.width, frame.height),
                format,
                frame.resource_epoch,
            )
        }
        .map_err(|error| {
            SurfaceError::FrameAcquisitionFailed(format!(
                "D3D12 shared-texture import failed: {error}"
            ))
        })?;
        let host = welding::HostWgpuContext::new(device.clone(), queue.clone());
        welding::WgpuTextureImporter::import_owned_dx12_callback_frame(native, &host)
            .map(|imported| imported.texture)
            .map_err(|error| {
                SurfaceError::FrameAcquisitionFailed(format!(
                    "D3D12 shared-texture import failed: {error}"
                ))
            })?
    } else {
        // wgpu-graft 8bb4d5e ("Make native frame ownership explicit") made
        // `Dx12SharedTexture`'s fields private too, but its safe constructor
        // takes an *owned* `Dx12SharedResource` — wrong here, since
        // `FrameHandleOwnership::Borrowed` means the producer keeps custody of
        // this handle for the resource epoch and Turnstone must not close it.
        // Import through grafting's documented borrowed escape hatch instead
        // of taking RAII custody Turnstone does not have.
        let host = grafting::HostWgpuContext::new(device.clone(), queue.clone());
        let metadata = grafting::FrameMetadata {
            size: PhysicalSize::new(frame.width, frame.height),
            format,
            generation: frame.resource_epoch,
            producer_sync: grafting::SyncMechanism::ImplicitGlFlush,
        };
        // SAFETY: `handle` is non-null (checked above) and, per
        // `FrameHandleOwnership::Borrowed`, remains valid and owned by the
        // producer for the duration of this import.
        unsafe {
            let borrowed =
                std::os::windows::io::BorrowedHandle::borrow_raw(handle as *mut std::ffi::c_void);
            grafting::import_dx12_shared_handle_borrowed(borrowed, metadata, &host)
        }
        .map_err(|error| {
            SurfaceError::FrameAcquisitionFailed(format!(
                "D3D12 shared-texture import failed: {error}"
            ))
        })?
    };
    *cached = Some(ImportedSurfaceFrame {
        resource_epoch: frame.resource_epoch,
        identity,
        texture,
    });
    Ok(())
}
