// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Host-side import for the neutral inker surface-frame vocabulary.
//!
//! Inker stays wgpu-free. Turnstone owns the one place that turns an acquired
//! native resource into a texture view on this window's device, and retains the
//! imported texture for the producer's resource epoch.

use inker::{
    FrameHandleOwnership, NativeTextureHandle, SurfaceError, SurfaceFrame, SurfaceTextureFormat,
};
use winit::dpi::PhysicalSize;

pub(super) struct ImportedSurfaceFrame {
    pub(super) resource_epoch: u64,
    // A view does not retain a texture by itself, so this is the cached
    // ownership. Each composition pass asks it for a fresh view.
    texture: wgpu::Texture,
}

impl ImportedSurfaceFrame {
    pub(super) fn view(&self) -> wgpu::TextureView {
        self.texture
            .create_view(&wgpu::TextureViewDescriptor::default())
    }
}

pub(super) fn update_imported_frame(
    cached: &mut Option<ImportedSurfaceFrame>,
    frame: SurfaceFrame,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<(), SurfaceError> {
    if cached
        .as_ref()
        .is_some_and(|existing| existing.resource_epoch == frame.resource_epoch)
    {
        // A reusable producer may emit several paints for one allocation. Its
        // existing imported texture sees those writes directly. A transferred
        // handle is never reusable; close a malformed duplicate rather than
        // turning it into a per-frame handle leak.
        close_if_transferred(&frame.texture);
        return Ok(());
    }

    let format = map_texture_format(&frame.format)?;
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
        let native = welding::native_frame::Dx12SharedTexture {
            handle: handle as *mut std::ffi::c_void,
            size: PhysicalSize::new(frame.width, frame.height),
            format,
            generation: frame.resource_epoch,
        };
        let host = welding::HostWgpuContext::new(device.clone(), queue.clone());
        welding::WgpuTextureImporter::import_owned_dx12_callback_frame(native, &host)
            .map(|imported| imported.texture)
            .map_err(|error| {
                SurfaceError::FrameAcquisitionFailed(format!(
                    "D3D12 shared-texture import failed: {error}"
                ))
            })?
    } else {
        let host = grafting::HostWgpuContext::new(device.clone(), queue.clone());
        let native = grafting::Dx12SharedTexture {
            size: PhysicalSize::new(frame.width, frame.height),
            format,
            generation: frame.resource_epoch,
            producer_sync: grafting::SyncMechanism::ImplicitGlFlush,
            fence_value: 0,
            handle: handle as *mut std::ffi::c_void,
        };
        grafting::import_dx12_shared_texture(&native, &host).map_err(|error| {
            SurfaceError::FrameAcquisitionFailed(format!(
                "D3D12 shared-texture import failed: {error}"
            ))
        })?
    };
    *cached = Some(ImportedSurfaceFrame {
        resource_epoch: frame.resource_epoch,
        texture,
    });
    Ok(())
}

fn map_texture_format(format: &SurfaceTextureFormat) -> Result<wgpu::TextureFormat, SurfaceError> {
    match format {
        SurfaceTextureFormat::Rgba8Unorm => Ok(wgpu::TextureFormat::Rgba8Unorm),
        SurfaceTextureFormat::Rgba8UnormSrgb => Ok(wgpu::TextureFormat::Rgba8UnormSrgb),
        SurfaceTextureFormat::Bgra8Unorm => Ok(wgpu::TextureFormat::Bgra8Unorm),
        SurfaceTextureFormat::Bgra8UnormSrgb => Ok(wgpu::TextureFormat::Bgra8UnormSrgb),
        SurfaceTextureFormat::Other(format) => Err(SurfaceError::Unsupported(format!(
            "Turnstone has no explicit import mapping for surface texture format {format}"
        ))),
    }
}

fn close_if_transferred(texture: &NativeTextureHandle) {
    if let NativeTextureHandle::D3d12Shared { handle, ownership } = texture {
        close_transferred_handle(*handle, *ownership);
    }
}

fn close_transferred_handle(handle: u64, ownership: FrameHandleOwnership) {
    if ownership != FrameHandleOwnership::Transferred || handle == 0 {
        return;
    }
    unsafe {
        let _ = windows::Win32::Foundation::CloseHandle(windows::Win32::Foundation::HANDLE(
            handle as *mut std::ffi::c_void,
        ));
    }
}
