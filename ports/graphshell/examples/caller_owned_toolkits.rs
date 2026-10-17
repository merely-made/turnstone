// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Real retained Genet/Fleece content and a Mere graph, encoded by two toolkit
//! adapters on one caller-owned GPU. Run with an evidence output directory:
//! cargo run -p graphshell --no-default-features --features caller-owned-toolkit-bench,image-decode --example caller_owned_toolkits -- <receipt-directory>
//! The toolkit callbacks are native/headless; physical IME/AT remain separate.
use cambium_rootstock::{
    TextureProducer,
    embedding::{self, Frame, Placement, TargetConfig, View},
};
use embedding::{egui, iced};
use graphshell::{
    app::GraphshellApp,
    mere_host::{FIXTURE_PERSONA_ADDRESS, SelectedPersonaRef},
    retained_content::{ContentHost, ContentProducer, HeldContent},
};
use iced::backend::primitive::{Pipeline as _, Primitive as _};
use netrender::SceneOp;
use std::{path::Path, sync::mpsc};

fn texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Caller toolkit target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
fn read(device: &wgpu::Device, queue: &wgpu::Queue, target: &wgpu::Texture) -> Vec<u8> {
    let row = (target.width() * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Caller evidence readback"),
        size: u64::from(row) * u64::from(target.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(target.height()),
            },
        },
        target.size(),
    );
    queue.submit([encoder.finish()]);
    let (send, receive) = mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |answer| {
            send.send(answer).unwrap();
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    receive
        .recv_timeout(std::time::Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .expect("caller readback mapped range");
    let mut pixels = vec![];
    for scanline in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&scanline[..target.width() as usize * 4]);
    }
    drop(mapped);
    buffer.unmap();
    pixels
}
fn clear(encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Caller clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.2,
                    g: 0.3,
                    b: 0.4,
                    a: 1.,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
}
fn render_egui(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frame: Frame,
    adapter: &egui::Adapter,
) -> Vec<u8> {
    let scale = frame.placement.scale;
    let target = texture(device, 720, 520);
    let view = target.create_view(&Default::default());
    let clip = frame.placement.clip;
    let jobs = vec![egui::paint::ClippedPrimitive {
        clip_rect: egui::paint::emath::Rect::from_min_size(
            egui::paint::emath::pos2(clip[0], clip[1]),
            egui::paint::emath::vec2(clip[2], clip[3]),
        ),
        primitive: egui::paint::Primitive::Callback(adapter.callback(frame)),
    }];
    let screen = egui::backend::ScreenDescriptor {
        size_in_pixels: [720, 520],
        pixels_per_point: scale,
    };
    let mut renderer = egui::backend::Renderer::new(
        device,
        wgpu::TextureFormat::Rgba8Unorm,
        egui::backend::RendererOptions::PREDICTABLE,
    );
    let mut encoder = device.create_command_encoder(&Default::default());
    clear(&mut encoder, &view);
    let commands = renderer.update_buffers(device, queue, &mut encoder, &jobs, &screen);
    {
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Caller egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime();
        renderer.render(&mut pass, &jobs, &screen);
    }
    queue.submit(commands.into_iter().chain([encoder.finish()]));
    let error = adapter.take_error();
    assert!(error.is_none(), "egui rendering refused: {error:?}");
    read(device, queue, &target)
}
fn render_iced(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frame: Frame,
    adapter: &iced::Adapter,
) -> Vec<u8> {
    let target = texture(device, 720, 520);
    let view = target.create_view(&Default::default());
    let b = frame.placement.bounds;
    let c = frame.placement.clip;
    let s = frame.placement.scale;
    let bounds = iced::backend::core::Rectangle {
        x: b[0],
        y: b[1],
        width: b[2],
        height: b[3],
    };
    let viewport = iced::backend::graphics::Viewport::with_physical_size(
        iced::backend::core::Size::new(720, 520),
        iced::backend::core::renderer::Scale {
            window: s,
            application: 1.,
        },
    );
    let primitive = adapter.primitive(frame);
    let mut pipeline = iced::Pipeline::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
    primitive.prepare(&mut pipeline, device, queue, &bounds, &viewport);
    let mut encoder = device.create_command_encoder(&Default::default());
    clear(&mut encoder, &view);
    primitive.render(
        &pipeline,
        &mut encoder,
        &view,
        &iced::backend::core::Rectangle {
            x: (c[0] * s) as u32,
            y: (c[1] * s) as u32,
            width: (c[2] * s) as u32,
            height: (c[3] * s) as u32,
        },
    );
    queue.submit([encoder.finish()]);
    let error = adapter.take_error();
    assert!(error.is_none(), "Iced rendering refused: {error:?}");
    read(device, queue, &target)
}
fn assert_clipped(pixels: &[u8], placement: Placement) {
    let x = (placement.clip[0] * placement.scale) as u32;
    let y = (placement.clip[1] * placement.scale) as u32;
    let w = (placement.clip[2] * placement.scale) as u32;
    let h = (placement.clip[3] * placement.scale) as u32;
    let untouched = &pixels[..4];
    let mut changed = 0;
    for py in 0..520 {
        for px in 0..720 {
            let value = &pixels[((py * 720 + px) * 4) as usize..((py * 720 + px) * 4 + 4) as usize];
            if px < x || py < y || px >= x + w || py >= y + h {
                assert_eq!(value, untouched, "pixels escaped caller clip");
            } else if value != untouched {
                changed += 1;
            }
        }
    }
    assert!(changed > 100, "native scene produced no meaningful pixels");
}
fn evidence(output: &Path, name: &str, pixels: &[u8], placement: Placement) {
    assert_clipped(pixels, placement);
    image::save_buffer(
        output.join(format!("{name}.png")),
        pixels,
        720,
        520,
        image::ColorType::Rgba8,
    )
    .unwrap();
}
struct GraphPane {
    canvas: mere::canvas::Canvas,
    source: u64,
    revision: u64,
}
impl TextureProducer for GraphPane {
    fn render(
        &mut self,
        _: &cambium_rootstock::ProducerContext<'_>,
    ) -> Option<cambium_rootstock::ProducedTexture> {
        None
    }
    fn portable_scene(
        &mut self,
        info: &cambium_rootstock::ProducerFrameInfo,
    ) -> Result<cambium_rootstock::ProducedScene, cambium_rootstock::PortableProducerError> {
        let (scene, _) = self
            .canvas
            .frame(info.logical_size.0 as u32, info.logical_size.1 as u32);
        Ok(cambium_rootstock::ProducedScene {
            scene,
            source_identity: self.source,
            source_revision: self.revision,
            alpha: netrender::SourceAlpha::Straight,
            encoding: cambium_rootstock::SourceEncoding::Srgb,
        })
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args().nth(1).ok_or("Pass a receipt directory")?;
    let output = Path::new(&output);
    std::fs::create_dir_all(output)?;
    let limits = mere_document_lanes::retained_clip::ClipLimits {
        max_parts: 8,
        max_bytes: 1024 * 1024,
        max_record_bytes: 128 * 1024,
    };
    let mut app = GraphshellApp::fixture(
        muniment::MemoryBackend::new(),
        SelectedPersonaRef {
            persona: FIXTURE_PERSONA_ADDRESS.into(),
            profile: "caller-toolkit-acceptance".into(),
        },
    )?;
    pollster::block_on(graphshell::retained_content::fixture::seed(
        &mut app, limits,
    ))?;
    let mut content = ContentHost::with_native_presentation(
        vec![include_bytes!("../web/IBMPlexSans-Regular.ttf").to_vec()],
        Some("IBM Plex Sans".into()),
    );
    let mut views = vec![];
    let mut accesses = vec![];
    let mut media_bytes = 0;
    let mut slots = vec![];
    for (_, node) in app.host.graph().nodes() {
        let Some(request) = mere_document_lanes::retained_clip::request(node.url())? else {
            continue;
        };
        let mere_document_lanes::retained_clip::ClipRequest::Read(id) = request else {
            continue;
        };
        let access = uuid::Uuid::new_v4();
        let mut store = app.host.store();
        let slot = pollster::block_on(content.open_clip(access, &mut store, None, id, limits))?;
        media_bytes += slot
            .borrow()
            .source()
            .media
            .as_ref()
            .map_or(0, |m| m.bytes.len());
        accesses.push(access);
        slots.push(slot.clone());
        let placement = Placement {
            bounds: [24., 32., 360., 200.],
            clip: [24., 32., 300., 180.],
            scale: 1.,
        };
        views.push(View::new(
            views.len() as u64 + 1,
            ContentProducer::new(slot, ContentProducer::allocate_key()),
            placement,
        )?);
    }
    assert_eq!(views.len(), 2);
    assert!(media_bytes > 0);
    let access = uuid::Uuid::new_v4();
    accesses.push(access);
    let slot=content.open(access,HeldContent{address:"mere://held/toolkit-document".into(),title:"Retained native document".into(),media_type:"text/html".into(),
        bytes:b"<html><body><h1>Retained Genet document</h1><p>The toolkit keeps its GPU.</p><a href='mere://held/toolkit-document'>Retained link</a></body></html>".to_vec(),engine:None,media:None})?;
    views.push(View::new(
        3,
        ContentProducer::new(slot, ContentProducer::allocate_key()),
        views[0].placement(),
    )?);
    let access = uuid::Uuid::new_v4();
    accesses.push(access);
    let slot = content.open(
        access,
        HeldContent {
            address: "mere://held/toolkit-image".into(),
            title: "Retained image".into(),
            media_type: "image/png".into(),
            bytes: include_bytes!("../docs/receipts/co_op_member.png").to_vec(),
            engine: None,
            media: None,
        },
    )?;
    views.push(View::new(
        4,
        ContentProducer::new(slot, ContentProducer::allocate_key()),
        views[0].placement(),
    )?);
    let access = uuid::Uuid::new_v4();
    accesses.push(access);
    let same_source = slots[0].borrow().source().clone();
    let duplicate = content.open(access, same_source)?;
    assert_ne!(
        slots[0].borrow().session_identity(),
        duplicate.borrow().session_identity()
    );
    views.push(View::new(
        6,
        ContentProducer::new(duplicate.clone(), ContentProducer::allocate_key()),
        views[0].placement(),
    )?);
    content.show_only(&accesses);
    // These are actual retained document owners, not a mock input sink. One
    // access receives focus; its separately opened peer refuses keyboard input.
    assert!(
        views[0]
            .input(
                graphshell::retained_content::SessionInput::Focus(true),
                |input| slots[0].borrow_mut().input(input)
            )
            .is_some()
    );
    assert!(
        views[4]
            .input(
                graphshell::retained_content::SessionInput::Text("unfocused peer".into()),
                |input| duplicate.borrow_mut().input(input)
            )
            .is_none()
    );
    let mut canvas = mere::canvas::Canvas::with_graph(app.host.graph().clone());
    canvas.set_physics_paused(true);
    let positions: Vec<_> = app
        .host
        .graph()
        .nodes()
        .enumerate()
        .map(|(n, (key, _))| {
            (
                key,
                mere::kernel::geometry::PortablePoint::new(
                    (n % 4) as f32 * 90.,
                    (n / 4) as f32 * 60.,
                ),
            )
        })
        .collect();
    canvas.land_paused_positions(&positions);
    canvas.resize(
        views[0].placement().bounds[2] as u32,
        views[0].placement().bounds[3] as u32,
    );
    canvas.fit_visible();
    let mut graph = View::new(
        5,
        GraphPane {
            canvas,
            source: ContentProducer::allocate_key(),
            revision: app.host.projection_revision().0,
        },
        views[0].placement(),
    )?;
    let handles = netrender::boot()?;
    let adapter_info = handles.adapter.get_info();
    let config = TargetConfig {
        format: wgpu::TextureFormat::Rgba8Unorm,
        sample_count: 1,
    };
    let resource_limits = netrender::vello_backends::SparseResourceLimits::default();
    let mut records = vec![];
    for (n, view) in views.iter_mut().enumerate() {
        let frame = view.frame(None).map_err(|e| format!("{e:?}"))?;
        assert!(
            frame
                .produced
                .scene
                .ops
                .iter()
                .any(|op| matches!(op, SceneOp::GlyphRun(_) | SceneOp::Image(_)))
        );
        if n == 3 {
            assert!(
                frame
                    .produced
                    .scene
                    .ops
                    .iter()
                    .any(|op| matches!(op, SceneOp::Image(_))),
                "held PNG decoded only as alternate text"
            );
            assert!(
                !frame.produced.scene.image_sources.is_empty(),
                "held PNG has no actual pixel source"
            );
        }
        let egui = egui::Adapter::new(config, resource_limits, 1);
        let iced = iced::Adapter::new(resource_limits, 1);
        evidence(
            output,
            &format!("egui-content-{n}"),
            &render_egui(&handles.device, &handles.queue, frame.clone(), &egui),
            frame.placement,
        );
        evidence(
            output,
            &format!("iced-content-{n}"),
            &render_iced(&handles.device, &handles.queue, frame.clone(), &iced),
            frame.placement,
        );
        view.resize(Placement {
            scale: 1.5,
            ..view.placement()
        })?;
        let resized = view.frame(None).map_err(|e| format!("{e:?}"))?;
        evidence(
            output,
            &format!("egui-content-{n}-scaled"),
            &render_egui(&handles.device, &handles.queue, resized.clone(), &egui),
            resized.placement,
        );
        evidence(
            output,
            &format!("iced-content-{n}-scaled"),
            &render_iced(&handles.device, &handles.queue, resized.clone(), &iced),
            resized.placement,
        );
        egui.device_recovered(2);
        iced.device_recovered(2);
        evidence(
            output,
            &format!("egui-content-{n}-rehydrated"),
            &render_egui(&handles.device, &handles.queue, resized.clone(), &egui),
            resized.placement,
        );
        evidence(
            output,
            &format!("iced-content-{n}-rehydrated"),
            &render_iced(&handles.device, &handles.queue, resized.clone(), &iced),
            resized.placement,
        );
        records.push(serde_json::json!({"appearance":frame.appearance,"source":frame.produced.source_identity,"revision":frame.produced.source_revision,"operations":frame.produced.scene.ops.len()}));
        egui.dispose();
        iced.dispose();
    }
    let frame = graph.frame(None).map_err(|e| format!("{e:?}"))?;
    assert!(
        !frame
            .produced
            .scene
            .ops
            .iter()
            .any(|op| matches!(op, SceneOp::Fragment(_))),
        "graph requires an admitted retained fragment source ledger"
    );
    evidence(
        output,
        "egui-graph",
        &render_egui(
            &handles.device,
            &handles.queue,
            frame.clone(),
            &egui::Adapter::new(config, resource_limits, 1),
        ),
        frame.placement,
    );
    evidence(
        output,
        "iced-graph",
        &render_iced(
            &handles.device,
            &handles.queue,
            frame.clone(),
            &iced::Adapter::new(resource_limits, 1),
        ),
        frame.placement,
    );
    records.push(serde_json::json!({"appearance":5,"graph_session":app.host.graph_session().id().0,"graph_nodes":app.host.graph().nodes().count(),"source_instance":frame.produced.source_identity,"revision":frame.produced.source_revision,"operations":frame.produced.scene.ops.len()}));
    let original_source = slots[0].borrow().source().clone();
    views[0].dispose();
    content.close(accesses[0]);
    assert_eq!(*duplicate.borrow().source(), original_source);
    assert!(
        views[4].frame(None).is_ok(),
        "closing one appearance retired its peer"
    );
    std::fs::write(
        output.join("qualification.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"kind":"native-headless-toolkit-callbacks","backend":"Netrender sparse GPU","wgpu":"30","caller_submission":true,"adapter":{"name":adapter_info.name,"backend":format!("{:?}",adapter_info.backend),"vendor":adapter_info.vendor,"device":adapter_info.device,"device_type":format!("{:?}",adapter_info.device_type),"driver":adapter_info.driver,"driver_info":adapter_info.driver_info},"caller_gpu":{"device_count":1,"queue_count":1,"same_handles_for_both_toolkits":true,"initial_generation":1,"rehydrated_generation":2,"physical_device_replaced":false},"media_bytes_verified":media_bytes,"views":records,"open":["Classic encoder","physical window IME and accessibility","actual device loss","audio output"]}),
        )?,
    )?;
    println!(
        "Caller-owned egui and Iced native scenes passed; receipts {}",
        output.display()
    );
    Ok(())
}
