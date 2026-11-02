// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ImportedSurfaceFrame {
    pub(in super::super) fn presentation_alpha(&self) -> Result<netrender::SourceAlpha, String> {
        if self.identity.format.is_srgb() {
            return Err("staged native source requires encoded RGBA in a non-sRGB view; acquired sRGB view is unsupported".into());
        }
        match self.identity.source {
            #[cfg(feature = "weld")]
            FrameSource::RawDx12 | FrameSource::WeldOwned => {
                Ok(netrender::SourceAlpha::Premultiplied)
            },
            #[cfg(feature = "scry")]
            FrameSource::ScryOwned => Ok(netrender::SourceAlpha::Premultiplied),
            #[cfg(feature = "servo")]
            // Servo aac43a3/WebRender 0.70 final composition uses
            // ONE/ONE_MINUS_SRC_ALPHA. Grafting bb48281 only normalizes
            // channel order/Y orientation with blend disabled; it preserves
            // that premultiplied output. Transparent native pixels still
            // require a headed producer acceptance receipt.
            FrameSource::ServoOwned => Ok(netrender::SourceAlpha::Premultiplied),
        }
    }

    pub(in super::super) fn dimensions(&self) -> [u32; 2] {
        [self.identity.width, self.identity.height]
    }

    pub(in super::super) fn view(&self) -> wgpu::TextureView {
        self.texture
            .create_view(&wgpu::TextureViewDescriptor::default())
    }
}
