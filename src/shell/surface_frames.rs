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
#[cfg(feature = "scry")]
use scrying_engine::scrying::{
    Dx12FenceSynchronizer, HostWgpuContext, ImportOptions, NativeFrame, SyncMechanism,
    TextureImporter, WgpuTextureImporter, native_frame::InteropSynchronizer,
};
#[cfg(feature = "weld")]
use winit::dpi::PhysicalSize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameSource {
    #[cfg(feature = "weld")]
    RawDx12,
    #[cfg(feature = "scry")]
    ScryOwned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrameIdentity {
    source: FrameSource,
    epoch: u64,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
}

pub(super) struct ImportedSurfaceFrame {
    pub(super) resource_epoch: u64,
    identity: FrameIdentity,
    // A view does not retain a texture by itself, so this is the cached
    // ownership. Each composition pass asks it for a fresh view.
    texture: wgpu::Texture,
}

#[cfg(feature = "scry")]
pub(super) struct ScryingFrameImporter {
    importer: WgpuTextureImporter,
    synchronizer: std::sync::Arc<Dx12FenceSynchronizer>,
    stats: std::cell::Cell<ScryingFrameStats>,
}

#[cfg(feature = "scry")]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScryingFrameStats {
    pub(super) frames: u64,
    pub(super) imports: u64,
    pub(super) waits: u64,
}

#[cfg(feature = "scry")]
impl ScryingFrameImporter {
    /// The same synchronizer must be installed on the WebView2 producer. Its
    /// shared handle also belongs in the Inker frame envelope. The host factory
    /// supplies it after construction, rather than accepting a raw spawn fence.
    pub(super) fn new(
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

    pub(super) fn stats(&self) -> ScryingFrameStats {
        self.stats.get()
    }

    pub(super) fn fence_handle(&self) -> u64 {
        self.synchronizer.shared_handle().0 as u64
    }
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
    #[cfg(feature = "scry")] scrying_importer: Option<&ScryingFrameImporter>,
) -> Result<(), SurfaceError> {
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
            "Turnstone's Scry importer requires an owned Scry native frame".into(),
        ))
    }
}

fn reuse_allocation(
    cached: Option<FrameIdentity>,
    incoming: FrameIdentity,
) -> Result<bool, SurfaceError> {
    if incoming.width == 0 || incoming.height == 0 {
        return Err(SurfaceError::FrameAcquisitionFailed(
            "surface producer emitted an empty texture".into(),
        ));
    }
    let Some(cached) = cached else {
        return Ok(false);
    };
    if cached.source != incoming.source || cached.epoch != incoming.epoch {
        return Ok(false);
    }
    if cached != incoming {
        return Err(SurfaceError::FrameAcquisitionFailed(
            "surface producer reused a resource epoch with changed texture metadata".into(),
        ));
    }
    Ok(true)
}

#[cfg(feature = "scry")]
fn prepare_scrying_frame(
    frame: SurfaceFrame,
    fence_handle: u64,
) -> Result<(NativeFrame, FrameIdentity), SurfaceError> {
    let expected = FrameIdentity {
        source: FrameSource::ScryOwned,
        epoch: frame.resource_epoch,
        width: frame.width,
        height: frame.height,
        format: map_texture_format(&frame.format)?,
    };
    let sync = match &frame.sync {
        inker::SurfaceSyncHandle::D3d12Fence { handle, value }
            if *handle == fence_handle && *handle != 0 && *value != 0 =>
        {
            *value
        },
        _ => {
            return Err(SurfaceError::FrameAcquisitionFailed(
                "Scry frame is missing its matching nonzero host fence and value".into(),
            ));
        },
    };
    let native = scrying_engine::into_scrying_native_frame(frame).map_err(|_| {
        SurfaceError::Unsupported("Turnstone received an unknown owned surface payload".into())
    })?;
    let NativeFrame::Dx12SharedTexture(payload) = &native else {
        return Err(SurfaceError::Unsupported(
            "Turnstone's Windows Scry importer requires a D3D12 frame".into(),
        ));
    };
    let actual = FrameIdentity {
        source: FrameSource::ScryOwned,
        epoch: payload.generation,
        width: payload.size.width,
        height: payload.size.height,
        format: payload.format,
    };
    if actual != expected
        || payload.producer_sync != SyncMechanism::ExplicitFence
        || payload.fence_value != sync
    {
        return Err(SurfaceError::FrameAcquisitionFailed(
            "Scry surface envelope disagrees with its owned frame or fence".into(),
        ));
    }
    reuse_allocation(None, expected)?;
    Ok((native, expected))
}

#[cfg(feature = "scry")]
fn synchronize_reused_scrying_frame(
    synchronizer: &dyn InteropSynchronizer,
    native: &NativeFrame,
) -> Result<(), SurfaceError> {
    synchronizer
        .producer_complete(native, native.producer_sync())
        .map_err(|error| {
            SurfaceError::FrameAcquisitionFailed(format!("Scry frame wait failed: {error}"))
        })
}

#[cfg(feature = "scry")]
fn update_scrying_frame(
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

#[cfg(feature = "weld")]
fn update_raw_frame(
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

#[cfg(all(test, feature = "scry"))]
mod tests {
    use std::cell::Cell;
    use std::os::windows::io::{AsRawHandle, OwnedHandle};
    use std::rc::Rc;

    use super::*;
    use inker::{OwnedSurfaceFrame, SurfaceSyncHandle};
    use scrying_engine::scrying::{WebSurfaceFrame, native_frame::Dx12SharedTexture};

    fn file_backed_frame() -> (SurfaceFrame, u64) {
        // A real owned Win32 file handle proves custody without requiring a
        // GPU. Rejected metadata must close it before any texture import.
        let file = std::fs::File::open(std::env::current_exe().unwrap()).unwrap();
        let handle: OwnedHandle = file.into();
        let raw = handle.as_raw_handle() as u64;
        let native = NativeFrame::Dx12SharedTexture(Dx12SharedTexture::from_owned_handle(
            winit::dpi::PhysicalSize::new(16, 8),
            wgpu::TextureFormat::Bgra8Unorm,
            3,
            SyncMechanism::ExplicitFence,
            9,
            handle,
        ));
        (
            scrying_engine::translation::map_frame(WebSurfaceFrame::Native(native), Some(77))
                .unwrap(),
            raw,
        )
    }

    fn handle_is_open(raw: u64) -> bool {
        let mut flags = 0;
        unsafe {
            windows::Win32::Foundation::GetHandleInformation(
                windows::Win32::Foundation::HANDLE(raw as *mut std::ffi::c_void),
                &mut flags,
            )
            .is_ok()
        }
    }

    #[test]
    fn owned_scry_frame_rejection_closes_custody_before_import() {
        let (mut frame, raw) = file_backed_frame();
        assert!(handle_is_open(raw));
        frame.width += 1;
        assert!(matches!(
            prepare_scrying_frame(frame, 77),
            Err(SurfaceError::FrameAcquisitionFailed(_))
        ));
        assert!(!handle_is_open(raw));

        let (frame, raw) = file_backed_frame();
        assert!(prepare_scrying_frame(frame, 78).is_err());
        assert!(!handle_is_open(raw));
    }

    #[test]
    fn owned_scry_frame_teardown_keeps_custody_until_consumer_drop() {
        let (frame, raw) = file_backed_frame();
        let (native, identity) = prepare_scrying_frame(frame, 77).unwrap();
        assert_eq!(identity.epoch, 3);
        assert!(handle_is_open(raw));
        drop(native);
        assert!(!handle_is_open(raw));
    }

    struct DropWitness(Rc<Cell<u32>>);

    impl std::fmt::Debug for DropWitness {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("DropWitness")
        }
    }

    impl OwnedSurfaceFrame for DropWitness {
        fn payload_kind(&self) -> &'static str {
            "unknown.fixture"
        }

        fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
            self
        }
    }

    impl Drop for DropWitness {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn missing_scry_importer_releases_unknown_payload_once() {
        let drops = Rc::new(Cell::new(0));
        let frame = SurfaceFrame {
            texture: NativeTextureHandle::OwnedPayload(Box::new(DropWitness(drops.clone()))),
            sync: SurfaceSyncHandle::D3d12Fence {
                handle: 77,
                value: 9,
            },
            width: 16,
            height: 8,
            format: SurfaceTextureFormat::Bgra8Unorm,
            resource_epoch: 3,
        };
        assert!(matches!(
            update_scrying_frame(&mut None, frame, None),
            Err(SurfaceError::Unsupported(_))
        ));
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn reused_scry_epoch_requires_unchanged_metadata() {
        let identity = FrameIdentity {
            source: FrameSource::ScryOwned,
            epoch: 3,
            width: 16,
            height: 8,
            format: wgpu::TextureFormat::Bgra8Unorm,
        };
        assert!(reuse_allocation(Some(identity), identity).unwrap());
        assert!(
            reuse_allocation(
                Some(identity),
                FrameIdentity {
                    width: 17,
                    ..identity
                }
            )
            .is_err()
        );
        assert!(
            !reuse_allocation(
                Some(identity),
                FrameIdentity {
                    epoch: 4,
                    ..identity
                }
            )
            .unwrap()
        );
        #[cfg(feature = "weld")]
        assert!(
            !reuse_allocation(
                Some(identity),
                FrameIdentity {
                    source: FrameSource::RawDx12,
                    ..identity
                },
            )
            .unwrap()
        );
    }

    struct RecordingSynchronizer {
        values: std::cell::RefCell<Vec<u64>>,
        fail: Cell<bool>,
    }

    impl InteropSynchronizer for RecordingSynchronizer {
        fn producer_complete(
            &self,
            frame: &NativeFrame,
            mechanism: SyncMechanism,
        ) -> Result<(), scrying_engine::scrying::native_frame::InteropError> {
            assert_eq!(mechanism, SyncMechanism::ExplicitFence);
            let NativeFrame::Dx12SharedTexture(payload) = frame else {
                panic!("expected D3D12 fixture")
            };
            self.values.borrow_mut().push(payload.fence_value);
            if self.fail.get() {
                return Err(scrying_engine::scrying::native_frame::InteropError::Dx12(
                    "fixture queue refused wait".into(),
                ));
            }
            Ok(())
        }

        fn consumer_ready(
            &self,
            _: &scrying_engine::scrying::ImportedTexture,
            _: SyncMechanism,
        ) -> Result<(), scrying_engine::scrying::native_frame::InteropError> {
            panic!("reused allocation must not be imported again")
        }
    }

    #[test]
    fn reused_scry_paints_each_wait_and_propagate_wait_failure() {
        let (frame, raw) = file_backed_frame();
        let (mut native, identity) = prepare_scrying_frame(frame, 77).unwrap();
        let synchronizer = RecordingSynchronizer {
            values: std::cell::RefCell::new(Vec::new()),
            fail: Cell::new(false),
        };
        assert!(reuse_allocation(Some(identity), identity).unwrap());
        synchronize_reused_scrying_frame(&synchronizer, &native).unwrap();
        let NativeFrame::Dx12SharedTexture(payload) = &mut native else {
            panic!("expected D3D12 fixture")
        };
        payload.fence_value = 10;
        assert!(reuse_allocation(Some(identity), identity).unwrap());
        synchronize_reused_scrying_frame(&synchronizer, &native).unwrap();
        assert_eq!(*synchronizer.values.borrow(), vec![9, 10]);

        synchronizer.fail.set(true);
        assert!(matches!(
            synchronize_reused_scrying_frame(&synchronizer, &native),
            Err(SurfaceError::FrameAcquisitionFailed(reason)) if reason.contains("fixture queue refused wait")
        ));
        assert!(handle_is_open(raw));
        drop(native);
        assert!(!handle_is_open(raw));
    }
}
