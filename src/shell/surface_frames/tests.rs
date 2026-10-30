// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

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
        scrying_engine::translation::map_frame(WebSurfaceFrame::Native(native), Some(77)).unwrap(),
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
