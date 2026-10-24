// SPDX-License-Identifier: MPL-2.0
//! Completed Servo GL snapshots enter Cue as move-owned DMA-BUF frames.
use inker::cue_browser::FrameReader;
use inker::{NativeTextureHandle, SurfaceSyncHandle, SurfaceTextureFormat};
use wgpu_cue::webcue::{self, *};

pub(crate) fn capabilities() -> Result<CapabilityReport> {
    super::cue_capabilities::report()
}

pub(crate) fn frame_reader(_device: wgpu::Device, _queue: wgpu::Queue) -> FrameReader {
    Box::new(move |producer, mut stamp| {
        let Some(frame) = producer
            .acquire_frame()
            .map_err(|e| Error::Backend(e.to_string()))?
        else {
            return Ok(None);
        };
        if !matches!(frame.sync, SurfaceSyncHandle::None)
            || frame.format != SurfaceTextureFormat::Rgba8Unorm
        {
            return Err(Error::Invalid(
                "Servo DMA-BUF frame has unexpected synchronization or format".into(),
            ));
        }
        let NativeTextureHandle::OwnedPayload(payload) = frame.texture else {
            return Err(Error::Invalid(
                "Servo DMA-BUF requires move-owned custody".into(),
            ));
        };
        if payload.payload_kind() != super::FRAME_KIND {
            return Err(Error::Invalid("unexpected Servo native payload".into()));
        }
        let payload = payload
            .into_any()
            .downcast::<super::ServoDmaBufFrame>()
            .map_err(|_| {
                Error::Invalid("Servo native custody differs from its declared payload kind".into())
            })?;
        let metadata = payload.frame.metadata();
        if metadata.size != FrameSize::new(frame.width, frame.height)
            || metadata.format != PixelFormat::Rgba8Unorm
            || metadata.generation != frame.resource_epoch
            || frame.resource_epoch == 0
            || !matches!(
                payload.frame.sync(),
                webcue::grafting_frame::FrameSync::None
            )
        {
            return Err(Error::Invalid(
                "Servo native metadata differs from its completed snapshot".into(),
            ));
        }
        stamp.allocation_epoch = frame.resource_epoch;
        Ok(Some(Frame::Native(NativeFrame {
            stamp,
            frame: payload.frame,
            alpha: Alpha::Premultiplied,
            encoding: Encoding::Srgb,
        })))
    })
}

/// Provision the primary/lens/producer device once, with native Vulkan import
/// extensions enabled before any renderer is constructed. This is boot-time
/// device creation; ordinary import and presentation never wait for completion.
pub(crate) fn boot_render_core(
    options: netrender::NetrenderOptions,
) -> std::result::Result<genet_render_host::RenderCore, String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        flags: wgpu::InstanceFlags::default(),
        memory_budget_thresholds: Default::default(),
        backend_options: Default::default(),
        display: None,
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        force_fallback_adapter: false,
        apply_limit_buckets: options.apply_limit_buckets,
        ..Default::default()
    }))
    .map_err(|e| format!("native Vulkan adapter: {e}"))?;
    let features = netrender::REQUIRED_FEATURES
        | (options.optional_features
            & adapter.features()
            & !wgpu::Features::all_experimental_mask());
    let host = grafting::vulkan_dmabuf::create_dmabuf_host_context(
        &adapter,
        &wgpu::DeviceDescriptor {
            label: Some("Turnstone shared native Vulkan device"),
            required_features: features,
            required_limits: wgpu::Limits {
                max_inter_stage_shader_variables: netrender::REQUIRED_INTER_STAGE_VARIABLES,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .map_err(|e| format!("native Vulkan device: {e}"))?;
    genet_render_host::RenderCore::from_wgpu_handles(
        netrender::WgpuHandles {
            instance,
            adapter,
            device: host.device,
            queue: host.queue,
        },
        options,
    )
}
