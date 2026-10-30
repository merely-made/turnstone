// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    pub(super) fn tick(&mut self) -> R<()> {
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
                wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => {
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
            4 if self
                .cards
                .iter()
                .all(|c| c.navigation_observed && c.completions.len() == c.requests as usize) =>
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
                use wgpu_cue::webcue::grafting_frame::{FrameSync, MetalTexture, OwnedNativeImage};
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
                self.records.push(
                    serde_json::json!({"foreign_host_refused":true,"diagnostic_device_only":true}),
                );
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

    pub(super) fn cleanup(&mut self) {
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
