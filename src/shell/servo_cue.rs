// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Concrete Servo/Metal receiving through the published shared contract.
//!
//! Graft normalizes the pre-present GL image onto this host's queue. Retaining
//! that Metal allocation adds ownership, without a second copy or a CPU wait.
//! The paired importer enforces the exact device/queue ordering relationship.
use super::{ServoHostOptions, ServoImportedFrame, TurnstoneServoFactory};
use graft_engine::{GraftProducer, GraftProducerFactory};
use inker::cue_browser::{BrowserSurface, FrameReader};
use inker::{
    EngineProfileBinding, NativeTextureHandle, SurfaceSpawnRequest, SurfaceSyncHandle,
    SurfaceTextureFormat,
};
use objc2::Message;
use pelt_core::cue::PeltSharedContent;
use std::{path::PathBuf, sync::Arc};
use webcue::grafting_frame::{FrameMetadata, FrameSync, MetalTexture, OwnedNativeImage};
use wgpu_cue::webcue::{self, *};
use wgpu_cue::{Imported, NativeImporter, RenderContext};

fn backend(error: impl std::fmt::Display) -> Error {
    Error::Backend(error.to_string())
}

/// A process-owned Servo host on an existing Metal device. Construct, open,
/// drive and finish on the same UI thread. Drop every surface before `finish`.
pub struct ServoCueHost {
    factory: TurnstoneServoFactory,
    handles: netrender::WgpuHandles,
}
impl ServoCueHost {
    pub fn new(
        handles: netrender::WgpuHandles,
        profile_dir: PathBuf,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self> {
        // Refuse a non-Metal host before process-global Servo initialization.
        // SAFETY: the guard is only inspected and dropped; no HAL object is mutated.
        if unsafe { handles.device.as_hal::<wgpu::wgc::api::Metal>() }.is_none() {
            return Err(Error::Unsupported(
                "Servo Cue binding requires Metal".into(),
            ));
        }
        Ok(Self {
            factory: TurnstoneServoFactory::new(
                ServoHostOptions {
                    profile_name: "cue".into(),
                    profile_dir,
                },
                handles.device.clone(),
                handles.queue.clone(),
                wake,
            )
            .map_err(backend)?,
            handles,
        })
    }
    /// The importer is paired to the producing queue. A frame with no fence
    /// cannot be transferred to a different queue merely by retaining it.
    pub fn importer(&self) -> ServoCueImporter {
        ServoCueImporter {
            device: self.factory.device.clone(),
            queue: self.factory.queue.clone(),
            inner: wgpu_cue::GraftMetalImporter,
        }
    }
    /// The host supplies a scope that is unique for this receiver lifetime.
    pub fn open(
        &self,
        address: &str,
        scope: Scope,
        viewport: Viewport,
    ) -> Result<PeltSharedContent> {
        let viewport = viewport.validate()?;
        let request = SurfaceSpawnRequest {
            url: address.into(),
            width: viewport.width,
            height: viewport.height,
            profile: EngineProfileBinding {
                user_data_dir: self
                    .factory
                    .options
                    .profile_dir
                    .to_string_lossy()
                    .into_owned(),
            },
            fence_handle: None,
        };
        let producer = Box::new(GraftProducer::new(
            self.factory.build(&request).map_err(backend)?,
        ));
        let route = pelt_core::PeltRoute {
            decision: inker::routing::EngineRouteDecision {
                engine_id: graft_engine::GRAFT_SERVO_ENGINE_ID.into(),
                surface_contract: inker::routing::SurfaceContract {
                    target: inker::routing::SurfaceTargetId::new(address),
                    mode: inker::routing::SurfaceContractMode::CompositedTexture,
                },
            },
            source: pelt_core::PeltRouteSource::UserOverride,
            state: pelt_core::PeltRouteState::Surface,
        };
        let surface = BrowserSurface::new_with_report(
            producer,
            scope,
            viewport,
            capabilities()?,
            frame_reader(self.factory.device.clone(), self.factory.queue.clone()),
        )?
        .with_engine_turn(|_| super::pump().map_err(backend));
        Ok(PeltSharedContent::from_browser(route, surface))
    }

    pub fn active_views(&self) -> usize {
        super::active_views()
    }
    pub fn finish(self) -> Result<()> {
        super::shutdown().map_err(backend)
    }
}

/// Only this paired importer may rely on the source's no-fence queue ordering.
pub struct ServoCueImporter {
    device: wgpu::Device,
    queue: wgpu::Queue,
    inner: wgpu_cue::GraftMetalImporter,
}
impl NativeImporter for ServoCueImporter {
    fn try_import(&mut self, cx: &RenderContext<'_>, frame: NativeFrame) -> Result<Imported> {
        if cx.device != &self.device || cx.queue != &self.queue {
            return Err(Error::Invalid(
                "Servo frame requires its producing host device and queue".into(),
            ));
        }
        self.inner.try_import(cx, frame)
    }
}

/// Preserve the concrete producer's import custody; never reinterpret a raw
/// borrowed descriptor as a transferred native allocation.
fn frame_reader(device: wgpu::Device, queue: wgpu::Queue) -> FrameReader {
    Box::new(move |producer, mut stamp| {
        let Some(frame) = producer.acquire_frame().map_err(backend)? else {
            return Ok(None);
        };
        if !matches!(frame.sync, SurfaceSyncHandle::None)
            || frame.format != SurfaceTextureFormat::Rgba8Unorm
        {
            return Err(Error::Invalid(
                "Servo frame has unexpected synchronization or format".into(),
            ));
        }
        let NativeTextureHandle::OwnedPayload(payload) = frame.texture else {
            return Err(Error::Unsupported(
                "Servo requires owned pre-present import custody".into(),
            ));
        };
        if payload.payload_kind() != super::FRAME_KIND {
            return Err(Error::Unsupported("unknown Servo payload".into()));
        }
        let payload = payload
            .into_any()
            .downcast::<ServoImportedFrame>()
            .map_err(|_| {
                Error::Invalid("Servo payload custody differs from its declared kind".into())
            })?;
        let imported = &payload.imported;
        let texture = &imported.texture;
        if payload.device != device
            || payload.queue != queue
            || payload.resource_epoch != frame.resource_epoch
            || imported.origin != grafting::TextureOrigin::TopLeft
            || imported.format != wgpu::TextureFormat::Rgba8Unorm
            || texture.format() != imported.format
            || (imported.size.width, imported.size.height) != (frame.width, frame.height)
            || texture.size()
                != (wgpu::Extent3d {
                    width: frame.width,
                    height: frame.height,
                    depth_or_array_layers: 1,
                })
            || texture.dimension() != wgpu::TextureDimension::D2
            || texture.sample_count() != 1
            || !texture
                .usage()
                .contains(wgpu::TextureUsages::TEXTURE_BINDING)
            || frame.width == 0
            || frame.height == 0
            || frame.resource_epoch == 0
        {
            return Err(Error::Invalid("Servo frame disagrees with device, queue, allocation or normalized texture metadata".into()));
        }
        // SAFETY: we retain the native object while the HAL destruction guard
        // is held. We neither destroy it nor mutate its state. Graft's source
        // normalization and the paired importer use the exact same queue.
        let retained = unsafe {
            let hal = texture
                .as_hal::<wgpu::wgc::api::Metal>()
                .ok_or_else(|| Error::Unsupported("Servo frame is not Metal".into()))?;
            hal.raw_handle().retain()
        };
        stamp.allocation_epoch = frame.resource_epoch;
        let native = OwnedNativeFrame::new(
            FrameMetadata {
                size: FrameSize::new(frame.width, frame.height),
                format: PixelFormat::Rgba8Unorm,
                generation: frame.resource_epoch,
            },
            OwnedNativeImage::MetalTexture(MetalTexture::new(retained)),
            FrameSync::None,
        );
        Ok(Some(Frame::Native(NativeFrame {
            stamp,
            frame: native,
            alpha: Alpha::Premultiplied,
            encoding: Encoding::Srgb,
        })))
    })
}

/// Effective shared operations of this installed adapter, including exact
/// refusals that the older navigation vocabulary groups together.
fn capabilities() -> Result<CapabilityReport> {
    CapabilityReport::new(
        BackendIdentity { engine: "graft.servo".into(), version: Some("Servo 0.7.0 / aac43a3f".into()), platform: "macos".into(), build_features: vec!["wgpu-30".into(), "metal".into(), "pre-present-graft".into()] },
        PresentationMode::NativeFrame, Evidence::Constructed, SemanticProtocol::Unavailable,
        Feature::ALL.map(|feature| (feature, match feature {
            Feature::Navigation | Feature::Reload | Feature::History => Support::Partial("asynchronous native navigation; completion acknowledges dispatch, with ordered navigation observations".into()),
            Feature::Zoom => Support::Partial("native page zoom with effective-value readback; content settles asynchronously".into()),
            Feature::Mouse => Support::Partial("move and primary/secondary/auxiliary buttons without modifiers".into()),
            Feature::Scroll => Support::Partial("unmodified pixel wheel input".into()),
            Feature::Focus => Support::Supported,
            Feature::Keyboard => Support::Partial("canonical logical keys; physical codes and repeat are not projected".into()),
            Feature::Text => Support::Partial("logical character-key input; explicit IME composition is not exposed".into()),
            Feature::Stop => Support::Unsupported("pinned Servo has no stop-loading API".into()),
            Feature::InlineHtml => Support::Unsupported("this host loads addresses; inline HTML/base-address construction is not implemented".into()),
            Feature::NativeKeyboard => Support::Unsupported("physical key codes and independent native character fields are not projected".into()),
            _ => Support::Unsupported("not exposed through this concrete shared binding; platform extensions retain their original owner".into()),
        })),
    )
}
