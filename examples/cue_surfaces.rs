// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Real same-host receiving: Pelt/Genet and Graft/Servo through Webcue and
//! WgpuCue. `cargo run --features servo --example cue_surfaces -- OUTPUT_DIR [--offscreen]`
//! The finite, isolated fixture writes capability reports, events and pixels.
//! Pixel readback is queued and polled on later turns, without GPU waits.
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The concrete native receiver currently requires macOS/Metal");
    std::process::exit(2);
}
#[cfg(target_os = "macos")]
fn main() {
    if let Err(e) = native::run() {
        eprintln!("CUE RECEIVING FAILED: {e}");
        std::process::exit(1);
    }
}
#[cfg(target_os = "macos")]
mod native {
    use genet_render_host::RenderCore;
    use pelt_core::{PeltContent, PeltTileRequest, cue::PeltSharedContent};
    use std::{
        path::PathBuf,
        sync::Arc,
        time::{Duration, Instant},
    };
    use turnstone::servo_host::cue::{ServoCueHost, ServoCueImporter};
    use wgpu_cue::webcue::*;
    use wgpu_cue::{GpuHost, Imported, NativeImporter, Presenter, Source};
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        window::{Window, WindowId},
    };
    type R<T> = Result<T>;
    struct Clock(Instant);
    impl pelt_core::PeltClock for Clock {
        fn now_ms(&self) -> f64 {
            self.0.elapsed().as_secs_f64() * 1000.
        }
    }
    fn html(color: &str, name: &str) -> String {
        format!(
            "<!doctype html><html><head><meta charset='utf-8'><title>Cue {name}</title><style>html,body{{margin:0;background:{color};font:20px sans-serif}}input{{position:absolute;left:16px;top:16px;width:160px;height:40px;font:20px sans-serif}}a{{position:absolute;left:16px;top:90px;color:white}}</style></head><body><input value='hello' oninput=\"document.title='Cue value='+this.value\"><a href='page-b.html'>next</a></body></html>"
        )
    }
    fn report(s: &dyn Surface) -> serde_json::Value {
        let c = s.capabilities();
        serde_json::json!({"scope":format!("{:?}",s.scope()),"engine":c.identity.engine,"version":c.identity.version,"platform":c.identity.platform,"build_features":c.identity.build_features,"presentation":format!("{:?}",c.presentation),"evidence":format!("{:?}",c.evidence),"semantics":format!("{:?}",c.semantics),"features":c.features().map(|(f,s)|(format!("{f:?}"),serde_json::json!({"permits":s.permits(),"status":format!("{s:?}")}))).collect::<serde_json::Map<_,_>>()})
    }
    struct Card {
        content: PeltSharedContent,
        presenter: Presenter,
        events: Vec<serde_json::Value>,
        last_sequence: u64,
        requests: u64,
        completions: Vec<RequestKey>,
        hash: Option<blake3::Hash>,
        stamp: Option<FrameStamp>,
        size: Option<[u32; 2]>,
        background: Option<[u8; 4]>,
        typed_observed: bool,
        navigation_observed: bool,
        navigation_observations: usize,
        retiring: bool,
        pending_readback: Option<(FrameStamp, u8, genet_render_host::PendingRgbaReadback)>,
    }
    impl Card {
        fn new(content: PeltSharedContent) -> Self {
            Self {
                content,
                presenter: Presenter::default(),
                events: vec![],
                last_sequence: 0,
                requests: 0,
                completions: vec![],
                hash: None,
                stamp: None,
                size: None,
                background: None,
                typed_observed: false,
                navigation_observed: false,
                navigation_observations: 0,
                retiring: false,
                pending_readback: None,
            }
        }
        fn command(&mut self, command: Command) -> R<RequestKey> {
            self.requests += 1;
            self.content.request(RequestId(self.requests), command)
        }
        fn drain(&mut self) -> R<()> {
            while let Some(e) = self.content.poll_event() {
                if e.scope != self.content.scope() || e.sequence <= self.last_sequence {
                    return Err(Error::Invalid("event scope/order changed".into()));
                }
                self.last_sequence = e.sequence;
                if let Event::Completed {
                    result: Err(error), ..
                } = &e.event
                {
                    if !self.retiring || !matches!(error, Error::Retired) {
                        return Err(Error::Backend(format!("accepted command failed: {error}")));
                    }
                }
                if matches!(
                    &e.event,
                    Event::NavigationRequested { .. }
                        | Event::NavigationFinished { success: true, .. }
                ) {
                    self.navigation_observations += 1;
                }
                match &e.event {
                    Event::NavigationRequested { address }
                    | Event::NavigationStarted { address }
                    | Event::AddressChanged(address)
                        if address.ends_with("page-b.html") =>
                    {
                        self.navigation_observed = true
                    },
                    Event::TitleChanged(title)
                        if title.starts_with("Cue value=") && title.contains('X') =>
                    {
                        self.typed_observed = true
                    },
                    Event::Semantics {
                        payload: SemanticPayload::Snapshot(projection),
                        ..
                    } if projection
                        .nodes()
                        .iter()
                        .any(|node| node.value.as_deref().is_some_and(|v| v.contains('X'))) =>
                    {
                        self.typed_observed = true
                    },
                    _ => {},
                }
                if let Event::Completed { request, .. } = &e.event {
                    if self.completions.contains(request) {
                        return Err(Error::Invalid("duplicate terminal result".into()));
                    }
                    self.completions.push(*request);
                }
                self.events.push(serde_json::json!({"sequence":e.sequence,"scope":format!("{:?}",e.scope),"event":format!("{:?}",e.event)}));
            }
            Ok(())
        }
        fn input(&mut self, input: SessionInput) -> R<()> {
            self.content.input(Input::Document(input))
        }
        fn edit(&mut self) -> R<()> {
            self.input(SessionInput::Focus(true))?;
            self.input(SessionInput::PointerMoved {
                x: 40.,
                y: 35.,
                modifiers: Default::default(),
            })?;
            for state in [SessionButtonState::Pressed, SessionButtonState::Released] {
                self.input(SessionInput::PointerButton {
                    x: 40.,
                    y: 35.,
                    button: SessionPointerButton::Primary,
                    state,
                    modifiers: Default::default(),
                })?;
            }
            self.input(SessionInput::Text("X".into()))?;
            self.command(Command::Zoom(1.25))?;
            if self
                .content
                .capabilities()
                .feature(Feature::Semantics)
                .permits()
            {
                self.command(Command::SetSemantics(true))?;
            }
            Ok(())
        }
        fn retire(&mut self) -> R<()> {
            let key = self.command(Command::Zoom(1.))?;
            self.content.retire()?;
            self.retiring = true;
            self.drain()?;
            if self.completions.iter().filter(|x| **x == key).count() != 1 {
                return Err(Error::Invalid("retirement lost an accepted request".into()));
            }
            if !matches!(self.input(SessionInput::Focus(true)), Err(Error::Retired)) {
                return Err(Error::Invalid("retired receiver accepted input".into()));
            }
            self.presenter.retire();
            Ok(())
        }
    }
    struct Blitter {
        layout: wgpu::BindGroupLayout,
        pipeline: wgpu::RenderPipeline,
        sampler: wgpu::Sampler,
    }
    impl Blitter {
        fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Cue receiver texture"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
            let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor {label:Some("Cue receiver blit"),source:wgpu::ShaderSource::Wgsl(r#"
struct V { @builtin(position) p:vec4<f32>, @location(0) uv:vec2<f32> }
@vertex fn vs(@builtin(vertex_index) i:u32)->V { var p=array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.)); var v:V; v.p=vec4(p[i],0.,1.);v.uv=vec2((p[i].x+1.)*.5,(1.-p[i].y)*.5);return v; }
@group(0) @binding(0) var t:texture_2d<f32>; @group(0) @binding(1) var s:sampler;
@fragment fn fs(v:V)->@location(0) vec4<f32> { return textureSample(t,s,v.uv); }
"#.into())});
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Cue receiver"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
            Self {
                layout,
                pipeline,
                sampler: device.create_sampler(&Default::default()),
            }
        }
        fn group(&self, device: &wgpu::Device, view: &wgpu::TextureView) -> wgpu::BindGroup {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        }
    }
    struct App {
        output: PathBuf,
        require_window: bool,
        window: Option<Arc<Window>>,
        core: Option<RenderCore>,
        host: Option<GpuHost>,
        servo: Option<ServoCueHost>,
        importer: Option<ServoCueImporter>,
        cards: Vec<Card>,
        surface: Option<wgpu::Surface<'static>>,
        config: Option<wgpu::SurfaceConfiguration>,
        blitter: Option<Blitter>,
        viewport: Viewport,
        start: Instant,
        phase: u8,
        phase_start: Instant,
        baseline: Vec<blake3::Hash>,
        navigation_baseline: Vec<usize>,
        presented_frames: u64,
        records: Vec<serde_json::Value>,
        held: Option<NativeFrame>,
        custody_readback: Option<genet_render_host::PendingRgbaReadback>,
        custody_checked: bool,
        old_scopes: Vec<Scope>,
        error: Option<String>,
        passed: bool,
    }
    impl App {
        fn new(output: PathBuf, require_window: bool) -> Self {
            Self {
                output,
                require_window,
                window: None,
                core: None,
                host: None,
                servo: None,
                importer: None,
                cards: vec![],
                surface: None,
                config: None,
                blitter: None,
                viewport: Viewport {
                    width: 320,
                    height: 240,
                    scale: 1.,
                },
                start: Instant::now(),
                phase: 0,
                phase_start: Instant::now(),
                baseline: vec![],
                navigation_baseline: vec![],
                presented_frames: 0,
                records: vec![],
                held: None,
                custody_readback: None,
                custody_checked: false,
                old_scopes: vec![],
                error: None,
                passed: false,
            }
        }
        fn address(&self, page: &str) -> String {
            url::Url::from_file_path(self.output.join(page))
                .unwrap()
                .to_string()
        }
        fn open(&self, page: &str) -> R<Vec<Card>> {
            let core = self.core.as_ref().unwrap();
            let address = self.address(page);
            let html = std::fs::read_to_string(self.output.join(page))
                .map_err(|e| Error::Backend(e.to_string()))?;
            let registries = pelt_core::native::registries(
                "cue-receiver",
                pelt_core::native::route_policy(),
                inker::EngineProfileBinding {
                    user_data_dir: String::new(),
                },
            );
            let v = self.viewport;
            let content = PeltContent::routed(
                registries,
                PeltTileRequest::from_request(
                    inker::SessionSpawnRequest::new(&address)
                        .with_body(&html)
                        .with_content_type("text/html")
                        .with_viewport(v.width, v.height),
                ),
                Arc::new(|| Box::new(Clock(Instant::now()))),
            )
            .map_err(Error::Backend)?;
            let mut genet = PeltSharedContent::from_content(
                content,
                core.renderer().wgpu_device.core.clone(),
                None,
            )?;
            genet.set_viewport(v)?;
            let servo = self.servo.as_ref().unwrap().open(
                &address,
                Scope {
                    owner: 0x534552564f,
                    generation: if page == "page-a.html" { 1 } else { 2 },
                },
                v,
            )?;
            Ok(vec![Card::new(genet), Card::new(servo)])
        }
        fn init(&mut self, el: &ActiveEventLoop) -> R<()> {
            std::fs::create_dir_all(&self.output).map_err(|e| Error::Backend(e.to_string()))?;
            for (page, color, name) in [
                ("page-a.html", "#d02020", "A"),
                ("page-b.html", "#2050d0", "B"),
            ] {
                std::fs::write(self.output.join(page), html(color, name))
                    .map_err(|e| Error::Backend(e.to_string()))?;
            }
            let window = Arc::new(
                el.create_window(
                    Window::default_attributes()
                        .with_title("Cue: Genet (left) / Servo (right)")
                        .with_visible(self.require_window)
                        .with_inner_size(winit::dpi::PhysicalSize::new(640, 240)),
                )
                .map_err(|e| Error::Backend(e.to_string()))?,
            );
            let core = RenderCore::boot(netrender::NetrenderOptions {
                enable_vello: true,
                tile_cache_size: Some(256),
                ..Default::default()
            })
            .map_err(Error::Backend)?;
            let handles = core.renderer().wgpu_device.core.clone();
            eprintln!("Cue shared receiver GPU: {:?}", handles.adapter.get_info());
            let surface = handles
                .instance
                .create_surface(window.clone())
                .map_err(|e| Error::Backend(e.to_string()))?;
            let size = window.inner_size();
            let config = surface
                .get_default_config(&handles.adapter, size.width, size.height)
                .ok_or_else(|| Error::Unsupported("no window surface configuration".into()))?;
            surface.configure(&handles.device, &config);
            self.blitter = Some(Blitter::new(&handles.device, config.format));
            self.host = Some(GpuHost::new(handles.device.clone(), handles.queue.clone())?);
            self.servo = Some(ServoCueHost::new(
                handles,
                self.output.join("servo-profile"),
                Arc::new(|| {}),
            )?);
            self.importer = Some(self.servo.as_ref().unwrap().importer());
            if self.require_window {
                window.focus_window();
            }
            eprintln!(
                "Cue window: visible={:?} focused={}",
                window.is_visible(),
                window.has_focus()
            );
            self.window = Some(window);
            self.core = Some(core);
            self.surface = Some(surface);
            self.config = Some(config);
            self.cards = self.open("page-a.html")?;
            self.record("constructed")?;
            Ok(())
        }
        fn record(&mut self, label: &str) -> R<()> {
            eprintln!("Cue receiving phase: {label}");
            let reports = self
                .cards
                .iter()
                .map(|c| report(&c.content))
                .collect::<Vec<_>>();
            self.records.push(serde_json::json!({"phase":label,"capabilities":reports,"frames":self.cards.iter().map(|c|serde_json::json!({"stamp":format!("{:?}",c.stamp),"size":c.size,"background":c.background,"hash":c.hash.map(|x|x.to_hex().to_string())})).collect::<Vec<_>>()}));
            Ok(())
        }
        fn tick(&mut self) -> R<()> {
            if self.start.elapsed() > Duration::from_secs(90) {
                return Err(Error::Backend(format!("phase {} timed out", self.phase)));
            }
            if let Some(result) = self.custody_readback.as_mut().and_then(|r| r.poll()) {
                self.custody_readback = None;
                let bytes = result.map_err(|e| Error::Backend(e.to_string()))?;
                let p = ((bytes.height - 12) * bytes.width + bytes.width - 12) as usize * 4;
                if bytes.rgba[p..p + 4] != [208, 32, 32, 255] {
                    return Err(Error::Invalid(
                        "native custody lost pixels after producer destruction".into(),
                    ));
                }
                self.records.push(serde_json::json!({"native_import_after_source_drop":true,"hash":blake3::hash(&bytes.rgba).to_hex().to_string()}));
                self.custody_checked = true;
            }
            let core = self.core.as_ref().unwrap();
            let host = self.host.as_ref().unwrap();
            let importer = self.importer.as_mut().unwrap();
            let mut groups = vec![];
            for (i, card) in self.cards.iter_mut().enumerate() {
                if let Some(result) = card
                    .pending_readback
                    .as_mut()
                    .and_then(|(_, _, r)| r.poll())
                {
                    let (stamp, phase, _) = card.pending_readback.take().unwrap();
                    let bytes = result.map_err(|e| Error::Backend(e.to_string()))?;
                    let first_frame = card.hash.is_none();
                    card.hash = Some(blake3::hash(&bytes.rgba));
                    card.size = Some([bytes.width, bytes.height]);
                    card.stamp = Some(stamp);
                    let p = ((bytes.height - 12) * bytes.width + bytes.width - 12) as usize * 4;
                    card.background = Some(bytes.rgba[p..p + 4].try_into().unwrap());
                    if first_frame {
                        eprintln!(
                            "Cue first pixels: {} {:?} {:?}",
                            card.content.capabilities().identity.engine,
                            card.size,
                            card.background
                        );
                    }

                    image::save_buffer(
                        self.output.join(format!("phase-{phase}-{i}.png")),
                        &bytes.rgba,
                        bytes.width,
                        bytes.height,
                        image::ColorType::Rgba8,
                    )
                    .map_err(|e| Error::Backend(e.to_string()))?;
                }
                card.content.advance(self.start.elapsed())?;
                card.drain()?;
                if self.phase == 2 {
                    continue;
                }
                // Hold the native allocation across producer destruction in the
                // resize phase. The normal presenter handles every other frame.
                if self.phase == 3 && i == 1 && self.held.is_none() {
                    if let Source::Native(source) = card.content.source() {
                        if let Some(Frame::Native(frame)) = source.try_acquire_frame()? {
                            if frame.frame.metadata().size
                                == FrameSize::new(self.viewport.width, self.viewport.height)
                            {
                                self.held = Some(frame);
                            }
                        }
                    }
                }
                if let Some(frame) =
                    card.presenter
                        .present(host, card.content.source(), self.viewport, importer)?
                {
                    if card.pending_readback.is_none() && card.stamp != Some(frame.stamp) {
                        card.pending_readback = Some((
                            frame.stamp,
                            self.phase,
                            core.start_rgba8_readback(
                                frame.texture,
                                self.viewport.width,
                                self.viewport.height,
                            )
                            .map_err(Error::Backend)?,
                        ));
                    }
                    groups.push((
                        i,
                        self.blitter
                            .as_ref()
                            .unwrap()
                            .group(core.device(), &frame.view),
                    ));
                }
            }
            if self.require_window {
                let output = match self.surface.as_ref().unwrap().get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(x)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(x) => x,
                    wgpu::CurrentSurfaceTexture::Occluded
                    | wgpu::CurrentSurfaceTexture::Timeout => {
                        return Ok(());
                    },
                    wgpu::CurrentSurfaceTexture::Outdated => {
                        self.surface
                            .as_ref()
                            .unwrap()
                            .configure(core.device(), self.config.as_ref().unwrap());
                        return Ok(());
                    },
                    other => return Err(Error::Backend(format!("window acquire: {other:?}"))),
                };
                let view = output.texture.create_view(&Default::default());
                let mut encoder = core.device().create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("shared Cue cards"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        ..Default::default()
                    });
                    pass.set_pipeline(&self.blitter.as_ref().unwrap().pipeline);
                    let c = self.config.as_ref().unwrap();
                    for (i, group) in &groups {
                        pass.set_viewport(
                            *i as f32 * c.width as f32 / 2.,
                            0.,
                            c.width as f32 / 2.,
                            c.height as f32,
                            0.,
                            1.,
                        );
                        pass.set_bind_group(0, group, &[]);
                        pass.draw(0..3, 0..1);
                    }
                }
                core.queue().submit([encoder.finish()]);
                core.queue().present(output);
                self.presented_frames += 1;
            }
            let ready = self.cards.iter().all(|c| {
                c.stamp.is_some() && c.size == Some([self.viewport.width, self.viewport.height])
            });
            match self.phase {
                0 if ready
                    && self
                        .cards
                        .iter()
                        .all(|c| c.background == Some([208, 32, 32, 255]))
                    && self.cards.iter().all(|c| {
                        c.content.capabilities().presentation != PresentationMode::NativeFrame
                            || c.navigation_observations > 0
                    }) =>
                {
                    self.record("initial-pixels")?;
                    self.baseline = self.cards.iter().map(|c| c.hash.unwrap()).collect();
                    for c in &mut self.cards {
                        c.edit()?;
                    }
                    self.phase = 1;
                    self.phase_start = Instant::now();
                },
                1 if ready
                    && self.cards.iter().enumerate().all(|(i, c)| {
                        c.hash != Some(self.baseline[i])
                            && c.completions.len() == c.requests as usize
                            && c.typed_observed
                    })
                    && self.phase_start.elapsed() > Duration::from_millis(150) =>
                {
                    self.record("input-and-command-results")?;
                    self.navigation_baseline = self
                        .cards
                        .iter()
                        .map(|c| c.navigation_observations)
                        .collect();
                    for c in &mut self.cards {
                        c.content.set_visible(false)?;
                        if c.input(SessionInput::Focus(true)).is_ok() {
                            return Err(Error::Invalid("hidden card accepted input".into()));
                        }
                        c.command(Command::Reload)?;
                    }
                    self.phase = 2;
                    self.phase_start = Instant::now();
                },
                2 if self.cards.iter().enumerate().all(|(i, c)| {
                    c.navigation_observations > self.navigation_baseline[i]
                        && c.completions.len() == c.requests as usize
                }) =>
                {
                    self.record("hidden-navigation-progress-without-frame-acquisition")?;
                    self.viewport = Viewport {
                        width: 300,
                        height: 200,
                        scale: 1.,
                    };
                    for c in &mut self.cards {
                        c.content.set_visible(true)?;
                        c.content.set_viewport(self.viewport)?;
                    }
                    self.phase = 3;
                    self.phase_start = Instant::now();
                },
                3 if self.cards[0].size == Some([300, 200]) && self.held.is_some() => {
                    self.record("resized-and-native-held")?;
                    let address = self.address("page-b.html");
                    for c in &mut self.cards {
                        c.command(Command::Navigate {
                            address: address.clone(),
                        })?;
                    }
                    self.phase = 4;
                    self.phase_start = Instant::now();
                },
                4 if self.cards.iter().all(|c| {
                    c.navigation_observed && c.completions.len() == c.requests as usize
                }) =>
                {
                    self.record("shared-navigation-dispatch-and-observation")?;
                    self.old_scopes = self.cards.iter().map(|c| c.content.scope()).collect();
                    for c in &mut self.cards {
                        c.retire()?;
                    }
                    self.records.push(serde_json::json!({"retired_events":self.cards.iter().map(|c|&c.events).collect::<Vec<_>>()}));
                    self.cards.clear();
                    if self.servo.as_ref().unwrap().active_views() != 0 {
                        return Err(Error::Invalid("closed Servo producer remained live".into()));
                    }
                    let frame = self.held.take().unwrap();
                    // A separate retained reference tests foreign-host refusal;
                    // the original move-owned frame is still available to import.
                    let webcue_image = match frame.frame.image() {
                        grafting_frame_image @ wgpu_cue::webcue::grafting_frame::OwnedNativeImage::MetalTexture(_) => grafting_frame_image,
                        _ => return Err(Error::Invalid("expected retained Metal texture".into())),
                    };
                    let wgpu_cue::webcue::grafting_frame::OwnedNativeImage::MetalTexture(native) =
                        webcue_image
                    else {
                        unreachable!()
                    };
                    use objc2::Message;
                    use wgpu_cue::webcue::grafting_frame::{
                        FrameSync, MetalTexture, OwnedNativeImage,
                    };
                    let foreign_frame = NativeFrame {
                        stamp: frame.stamp,
                        alpha: frame.alpha,
                        encoding: frame.encoding,
                        frame: OwnedNativeFrame::new(
                            frame.frame.metadata(),
                            OwnedNativeImage::MetalTexture(MetalTexture::new(
                                native.texture().retain(),
                            )),
                            FrameSync::None,
                        ),
                    };
                    let adapter = &self
                        .core
                        .as_ref()
                        .unwrap()
                        .renderer()
                        .wgpu_device
                        .core
                        .adapter;
                    let (device, queue) =
                        pollster::block_on(adapter.request_device(&Default::default()))
                            .map_err(|e| Error::Backend(e.to_string()))?;
                    let foreign = GpuHost::new(device, queue)?;
                    if !matches!(
                        self.importer
                            .as_mut()
                            .unwrap()
                            .try_import(&foreign.context(self.viewport)?, foreign_frame),
                        Err(Error::Invalid(_))
                    ) {
                        return Err(Error::Invalid(
                            "native frame accepted on a foreign host".into(),
                        ));
                    }
                    self.records.push(serde_json::json!({"foreign_host_refused":true,"diagnostic_device_only":true}));
                    let cx = self.host.as_ref().unwrap().context(self.viewport)?;
                    let Imported::Ready(texture) =
                        self.importer.as_mut().unwrap().try_import(&cx, frame)?
                    else {
                        return Err(Error::Invalid(
                            "same-queue native frame unexpectedly pending".into(),
                        ));
                    };
                    self.custody_readback = Some(
                        self.core
                            .as_ref()
                            .unwrap()
                            .start_rgba8_readback(&texture.texture, 300, 200)
                            .map_err(Error::Backend)?,
                    );
                    self.cards = self.open("page-b.html")?;
                    for (i, c) in self.cards.iter().enumerate() {
                        if c.content.scope() == self.old_scopes[i] {
                            return Err(Error::Invalid("replacement reused scope".into()));
                        }
                    }
                    self.phase = 5;
                    self.phase_start = Instant::now();
                },
                5 if ready
                    && self.custody_checked
                    && self
                        .cards
                        .iter()
                        .all(|c| c.background == Some([32, 80, 208, 255])) =>
                {
                    self.record("replacement-pixels")?;
                    for c in &mut self.cards {
                        c.retire()?;
                    }
                    self.cards.clear();
                    let servo = self.servo.take().unwrap();
                    if servo.active_views() != 0 {
                        return Err(Error::Invalid("Servo views leaked".into()));
                    }
                    servo.finish()?;
                    self.passed = true;
                    self.phase = 6;
                    std::fs::write(self.output.join("qualification.json"),serde_json::to_vec_pretty(&serde_json::json!({"passed":true,"platform":"macos/Metal","qualification_kind":if self.require_window {"headed-native-receiving"} else {"offscreen-native-receiving"},"producer_device_count":1,"window_presentations":self.presented_frames,"records":self.records,"limits":["finite receiving host, not full Turnstone UI migration","synthetic shared input; no physical input/IME or human AT","Linux/Vulkan and physical device reset unqualified"]})).unwrap()).map_err(|e|Error::Backend(e.to_string()))?;
                    eprintln!("CUE RECEIVING PASSED");
                },
                _ => {},
            }
            Ok(())
        }
        fn cleanup(&mut self) {
            if let Some(error) = &self.error {
                let _ = std::fs::write(self.output.join("failure.json"), serde_json::to_vec_pretty(&serde_json::json!({"passed":false,"error":error,"phase":self.phase,"records":self.records,"cards":self.cards.iter().map(|c|serde_json::json!({"report":report(&c.content),"background":c.background,"size":c.size,"typed_observed":c.typed_observed,"requests":c.requests,"completions":c.completions.len(),"navigation_observations":c.navigation_observations,"events":c.events})).collect::<Vec<_>>()})).unwrap());
            }
            self.cards.clear();
            self.held = None;
            if let Some(servo) = self.servo.take() {
                let _ = servo.finish();
            }
        }
    }
    impl ApplicationHandler for App {
        fn resumed(&mut self, el: &ActiveEventLoop) {
            if self.window.is_none() {
                if let Err(e) = self.init(el) {
                    self.error = Some(e.to_string());
                    self.cleanup();
                    el.exit();
                }
            }
        }
        fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
            match event {
                WindowEvent::CloseRequested => {
                    self.cleanup();
                    el.exit();
                },
                WindowEvent::RedrawRequested => {
                    if let Err(e) = self.tick() {
                        self.error = Some(e.to_string());
                        self.cleanup();
                        el.exit();
                    }
                    if self.passed {
                        el.exit();
                    }
                },
                _ => {},
            }
        }
        fn about_to_wait(&mut self, el: &ActiveEventLoop) {
            el.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(16),
            ));
            if !self.require_window && self.core.is_some() && !self.passed {
                if let Err(e) = self.tick() {
                    self.error = Some(e.to_string());
                    self.cleanup();
                    el.exit();
                }
                if self.passed {
                    el.exit();
                }
            } else if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }
    pub fn run() -> std::result::Result<(), String> {
        let output = std::env::args_os()
            .nth(1)
            .ok_or("supply a fresh output directory")?;
        let output = PathBuf::from(output);
        if output.exists() {
            return Err("output directory must be fresh".into());
        }
        let output = std::path::absolute(output).map_err(|e| e.to_string())?;
        let require_window = match std::env::args_os().nth(2) {
            None => true,
            Some(flag) if flag == "--offscreen" => false,
            Some(_) => return Err("only --offscreen is accepted after the output directory".into()),
        };
        let mut app = App::new(output, require_window);
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        let mut builder = EventLoop::builder();
        builder
            .with_activation_policy(ActivationPolicy::Regular)
            .with_activate_ignoring_other_apps(require_window);
        let el = builder.build().map_err(|e| e.to_string())?;
        el.run_app(&mut app).map_err(|e| e.to_string())?;
        if let Some(e) = app.error {
            return Err(e);
        }
        if !app.passed {
            return Err("receiver closed before qualification completed".into());
        }
        Ok(())
    }
}
