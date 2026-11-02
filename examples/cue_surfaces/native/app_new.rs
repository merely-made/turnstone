// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    pub(super) fn new(output: PathBuf, require_window: bool) -> Self {
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

    pub(super) fn address(&self, page: &str) -> String {
        url::Url::from_file_path(self.output.join(page))
            .unwrap()
            .to_string()
    }

    pub(super) fn open(&self, page: &str) -> R<Vec<Card>> {
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

    pub(super) fn init(&mut self, el: &ActiveEventLoop) -> R<()> {
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

    pub(super) fn record(&mut self, label: &str) -> R<()> {
        eprintln!("Cue receiving phase: {label}");
        let reports = self
            .cards
            .iter()
            .map(|c| report(&c.content))
            .collect::<Vec<_>>();
        self.records.push(serde_json::json!({"phase":label,"capabilities":reports,"frames":self.cards.iter().map(|c|serde_json::json!({"stamp":format!("{:?}",c.stamp),"size":c.size,"background":c.background,"hash":c.hash.map(|x|x.to_hex().to_string())})).collect::<Vec<_>>()}));
        Ok(())
    }
}
