// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Shared presentation over the real placement pool. Renderer caches own no
//! document session, routing, fetch, history or semantic admission authority.
use super::{CompositeLayer, content_pool::ContentKey};
use crate::surface::{Surface, SurfaceId};
use netrender::{WgpuHandles, external_texture::ExternalTexturePlacement};
use pelt_core::{PeltControllerPool, cue::PeltSceneRenderer};
use std::collections::HashMap;
use wgpu_cue::{
    GpuHost, NativeImporter, Presenter,
    webcue::{self, Surface as _, *},
};

struct Entry {
    content: ContentKey,
    renderer: Option<PeltSceneRenderer>,
    presenter: Presenter,
    presentations: u64,
    last_stamp: Option<FrameStamp>,
    submitted_revision: u64,
}
#[derive(Default)]
pub(super) struct CueFrames {
    host: Option<GpuHost>,
    entries: HashMap<SurfaceId, Entry>,
}
impl CueFrames {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.host = None;
    }
    pub fn diagnostic(&self) -> serde_json::Value {
        serde_json::json!({"entries":self.entries.iter().map(|(id, entry)| serde_json::json!({
            "surface":id.0,"content":format!("{:?}",entry.content),
            "direction":if entry.renderer.is_some() {"host-device"} else {"native-frame"},
            "sampled_layers":entry.presentations,
            "scope":entry.last_stamp.map(|s|format!("{}:{}",s.scope.owner,s.scope.generation)),
            "revision":entry.last_stamp.map(|s|s.content_revision),
            "pending":entry.renderer.as_ref().is_some_and(PeltSceneRenderer::pending),
        })).collect::<Vec<_>>()})
    }
    pub fn ready_count(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.last_stamp.is_some())
            .count()
    }
    pub fn retain(&mut self, pool: &PeltControllerPool<ContentKey, SurfaceId, netrender::Scene>) {
        self.entries.retain(|id, entry| {
            pool.placement_generation(id)
                .is_some_and(|generation| pool.admits(id, generation))
                && pool
                    .placement(id)
                    .is_some_and(|p| p.content == entry.content)
        });
    }
    pub fn present(
        &mut self,
        pool: &mut PeltControllerPool<ContentKey, SurfaceId, netrender::Scene>,
        surface: &Surface,
        scale: f32,
        handles: WgpuHandles,
        importer: &mut dyn NativeImporter,
    ) -> Result<(Option<CompositeLayer>, bool)> {
        if !pool
            .placement_generation(&surface.id)
            .is_some_and(|generation| pool.admits(&surface.id, generation))
        {
            self.entries.remove(&surface.id);
            return Ok((None, false));
        }
        let viewport = Viewport {
            width: surface.rect.w.round().max(1.) as u32,
            height: surface.rect.h.round().max(1.) as u32,
            scale,
        }
        .validate()?;
        // A replacement host never inherits pending frames from the old GPU.
        if self.host.as_ref().is_some_and(|host| {
            host.context(viewport)
                .is_ok_and(|cx| cx.device != &handles.device || cx.queue != &handles.queue)
        }) {
            self.clear();
        }
        if self.host.is_none() {
            self.host = Some(GpuHost::new(handles.device.clone(), handles.queue.clone())?);
        }
        let key = pool.placement(&surface.id).ok_or(Error::Stale)?.content;
        if self
            .entries
            .get(&surface.id)
            .is_some_and(|entry| entry.content != key)
        {
            self.entries.remove(&surface.id);
        }
        let entry = self.entries.entry(surface.id).or_insert_with(|| Entry {
            content: key,
            renderer: None,
            presenter: Presenter::default(),
            presentations: 0,
            last_stamp: None,
            submitted_revision: 0,
        });
        let revision = pool.paint_revision(&key).unwrap_or(0);
        let content = pool.cue_content_mut(&surface.id).map_err(Error::Backend)?;
        let host = self.host.as_ref().unwrap();
        let frame = if let Some(controller) = content.document_mut() {
            if entry.renderer.is_none() {
                entry.renderer = Some(PeltSceneRenderer::new(handles)?);
            }
            if !entry.renderer.as_ref().unwrap().pending() {
                entry.submitted_revision = revision;
            }
            let mut source = entry.renderer.as_mut().unwrap().bind(controller);
            entry.presenter.present(
                host,
                wgpu_cue::Source::HostDevice(&mut source),
                viewport,
                importer,
            )?
        } else {
            entry.renderer = None;
            let source = content
                .cue_browser_mut()
                .ok_or_else(|| Error::Unsupported("browser has no shared frame binding".into()))?;
            source.set_viewport(viewport)?;
            entry
                .presenter
                .present(host, wgpu_cue::Source::Native(source), viewport, importer)?
        };
        let pending = entry
            .renderer
            .as_ref()
            .is_some_and(PeltSceneRenderer::pending)
            || frame.is_none()
            || (entry.renderer.is_some() && revision != entry.submitted_revision);
        if let Some(frame) = &frame {
            if frame.encoding != Encoding::Srgb || frame.texture.format().is_srgb() {
                return Err(Error::Unsupported(
                    "Turnstone composition requires encoded sRGB in a non-sRGB texture view".into(),
                ));
            }
            entry.presentations = entry.presentations.saturating_add(1);
            entry.last_stamp = Some(frame.stamp);
        }
        let layer = frame.map(|frame| {
            let alpha = match frame.alpha {
                Alpha::Premultiplied => netrender::SourceAlpha::Premultiplied,
                _ => netrender::SourceAlpha::Straight,
            };
            CompositeLayer {
                id: surface.id.0,
                kind: surface.kind,
                changed: true,
                dimensions: [viewport.width, viewport.height],
                alpha: Ok(alpha),
                view: frame.view,
                raster: Some(frame.texture.clone()),
                raster_scale: scale,
                placement: ExternalTexturePlacement::new(surface.rect.dest()).with_alpha(alpha),
            }
        });
        Ok((layer, pending))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
struct NoNative;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
impl NativeImporter for NoNative {
    fn try_import(
        &mut self,
        _: &wgpu_cue::RenderContext<'_>,
        _: NativeFrame,
    ) -> Result<wgpu_cue::Imported> {
        Err(Error::Unsupported(
            "no shared native importer on this host".into(),
        ))
    }
}
impl super::Shell {
    pub(super) fn cue_content_layer(
        &mut self,
        surface: &Surface,
        scale: f32,
    ) -> std::result::Result<(Option<CompositeLayer>, bool), String> {
        let handles = self
            .host
            .as_ref()
            .ok_or("render host is not ready")?
            .renderer()
            .wgpu_device
            .core
            .clone();
        #[cfg(target_os = "macos")]
        let mut importer = wgpu_cue::GraftMetalImporter;
        #[cfg(target_os = "linux")]
        let mut importer = wgpu_cue::GraftVulkanImporter;
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let mut importer = NoNative;
        self.cue_frames
            .present(
                &mut self.content_sessions.pool,
                surface,
                scale,
                handles,
                &mut importer,
            )
            .map_err(|e| e.to_string())
    }
}

pub(super) fn cursor_shape(cursor: &str) -> Option<inker::CursorShape> {
    use inker::CursorShape::*;
    Some(match cursor {
        "default" => Default,
        "text" => Text,
        "pointer" => Pointer,
        "grab" => Grab,
        "grabbing" => Grabbing,
        "crosshair" => Crosshair,
        "move" => Move,
        "ns-resize" => ResizeNs,
        "ew-resize" => ResizeEw,
        "nesw-resize" => ResizeNesw,
        "nwse-resize" => ResizeNwse,
        "not-allowed" => NotAllowed,
        "none" => Hidden,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use inker::{DocumentSession, SessionClick, SessionLink, SessionScrollKey};
    struct Clock;
    impl pelt_core::PeltClock for Clock {
        fn now_ms(&self) -> f64 {
            0.
        }
    }
    struct ColorDocument {
        blue: bool,
    }
    impl DocumentSession<netrender::Scene> for ColorDocument {
        fn frame(&mut self, w: u32, h: u32) -> netrender::Scene {
            let mut scene = netrender::Scene::new(w, h);
            scene.push_rect(
                0.,
                0.,
                w as f32,
                h as f32,
                if self.blue {
                    [0., 0., 1., 1.]
                } else {
                    [1., 0., 0., 1.]
                },
            );
            scene
        }
        fn text_input(&mut self, _: &str) -> bool {
            self.blue = true;
            true
        }
        fn scroll_by(&mut self, _: f32, _: f32) -> bool {
            false
        }
        fn scroll_for_key(&mut self, _: SessionScrollKey) -> bool {
            false
        }
        fn click_at(&mut self, _: f32, _: f32) -> SessionClick {
            SessionClick::Miss
        }
        fn links(&self) -> Vec<SessionLink> {
            vec![]
        }
        fn as_any(&mut self) -> &mut dyn std::any::Any {
            self
        }
        fn as_any_ref(&self) -> &dyn std::any::Any {
            self
        }
    }
    struct NoNative;
    impl NativeImporter for NoNative {
        fn try_import(
            &mut self,
            _: &wgpu_cue::RenderContext<'_>,
            _: NativeFrame,
        ) -> Result<wgpu_cue::Imported> {
            panic!("retained fixture must not import native frames")
        }
    }
    #[test]
    #[ignore = "requires a real wgpu adapter; run explicitly"]
    fn input_during_a_pending_frame_gets_a_final_blue_frame_and_unadmitted_geometry_skips() {
        use std::{
            sync::Arc,
            time::{Duration, Instant},
        };
        let core = genet_render_host::RenderCore::boot(netrender::NetrenderOptions {
            enable_vello: true,
            tile_cache_size: Some(256),
            ..Default::default()
        })
        .unwrap();
        let controller = pelt_core::PeltController::from_session_shared(
            Arc::new(inker::SessionRegistry::new()),
            Arc::new(inker::SurfaceEngineRegistry::new()),
            pelt_core::PeltControllerConfig::from_request(
                "fixture.shared",
                inker::SessionSpawnRequest::new("https://fixture.test/"),
            ),
            Clock,
            Box::new(ColorDocument { blue: false }),
        );
        let route = pelt_core::PeltRoute {
            decision: inker::routing::EngineRouteDecision {
                engine_id: "fixture.shared".into(),
                surface_contract: inker::routing::SurfaceContract {
                    target: inker::routing::SurfaceTargetId::new("fixture"),
                    mode: inker::routing::SurfaceContractMode::CompositedTexture,
                },
            },
            source: pelt_core::PeltRouteSource::Automatic,
            state: pelt_core::PeltRouteState::Document,
        };
        let key = ContentKey::Node(uuid::Uuid::nil());
        let id = SurfaceId(7);
        let surface = Surface {
            id,
            kind: crate::surface::SurfaceKind::Content(uuid::Uuid::nil()),
            rect: crate::surface::Rect::new(0., 0., 64., 64.),
        };
        let mut pool = PeltControllerPool::new();
        pool.insert(
            key,
            pelt_core::PeltContent::from_controller(controller, route),
        );
        pool.set_placements([(
            id,
            pelt_core::PeltPlacement {
                content: key,
                window: 0,
                rect: pelt_core::WorkspaceRect::new(0., 0., 64., 64.),
                visible: true,
                focused: true,
                scale_factor: 1.,
            },
        )]);
        let identity = pool.content(&key).unwrap().session_identity();
        let handles = core.renderer().wgpu_device.core.clone();
        let mut frames = CueFrames::default();
        let (first, pending) = frames
            .present(&mut pool, &surface, 1., handles.clone(), &mut NoNative)
            .unwrap();
        assert!(
            first.is_none() && pending,
            "first submission is asynchronous"
        );
        assert!(
            pool.input(
                &id,
                pelt_core::PeltInput::Session(SessionInput::Text("blue".into()))
            )
            .handled
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut saw_old = false;
        loop {
            let (layer, pending) = frames
                .present(&mut pool, &surface, 1., handles.clone(), &mut NoNative)
                .unwrap();
            if let Some(layer) = layer {
                let pixels = core
                    .read_rgba8_texture(layer.raster.as_ref().unwrap(), 64, 64)
                    .unwrap();
                let pixel = &pixels.rgba[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4];
                if pixel == [255, 0, 0, 255] {
                    saw_old = true;
                    assert!(
                        pending,
                        "a completed older frame cannot swallow the newer input redraw"
                    );
                }
                if pixel == [0, 0, 255, 255] && !pending {
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "new input never reached a settled shared frame"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            saw_old,
            "exercise the completion of a frame submitted before input"
        );
        assert_eq!(pool.content(&key).unwrap().session_identity(), identity);
        pool.set_placements([]);
        let (frame, pending) = frames
            .present(&mut pool, &surface, 1., handles, &mut NoNative)
            .unwrap();
        assert!(frame.is_none() && !pending);
        assert_eq!(frames.ready_count(), 0);
    }
}
