// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::os::windows::{fs::OpenOptionsExt, io::OwnedHandle};

#[test]
fn rejected_weld_envelopes_release_native_custody_before_import() {
    let identity = FrameIdentity {
        source: FrameSource::WeldOwned,
        epoch: 3,
        width: 16,
        height: 8,
        format: wgpu::TextureFormat::Bgra8Unorm,
    };
    for expected in [
        FrameIdentity {
            width: 17,
            ..identity
        },
        FrameIdentity {
            epoch: 4,
            ..identity
        },
        FrameIdentity {
            format: wgpu::TextureFormat::Rgba8Unorm,
            ..identity
        },
    ] {
        let path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let open_exclusive = || {
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&path)
        };
        let handle: OwnedHandle = open_exclusive().unwrap().into();
        assert!(open_exclusive().is_err());
        let native = welding::native_frame::Dx12SharedTexture::from_owned_handle(
            handle,
            PhysicalSize::new(16, 8),
            wgpu::TextureFormat::Bgra8Unorm,
            3,
        )
        .unwrap();
        assert!(validate_welding_native_frame(native, expected).is_err());
        assert!(
            open_exclusive().is_ok(),
            "native custody must release the exclusive file handle"
        );
    }
    assert!(
        !reuse_allocation(
            Some(identity),
            FrameIdentity {
                source: FrameSource::RawDx12,
                ..identity
            }
        )
        .unwrap()
    );
}
