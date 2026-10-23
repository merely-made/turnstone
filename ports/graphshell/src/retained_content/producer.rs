// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use cambium_rootstock::{
    PortableProducerError, ProducedScene, ProducedTexture, ProducerAction, ProducerContext,
    ProducerFrameInfo, ProducerNode, ProducerRole, ProducerSemantics, SourceAlpha, SourceEncoding,
    TextureProducer,
};
use inker::{
    DocumentA11yAction, DocumentA11yActionRequest, SessionButtonState, SessionModifiers,
    SessionPointerButton,
};

/// A retained Pelt document painted on the host device with private renderer
/// quality state; its input and document ownership remain with Pelt.
pub struct ContentProducer {
    slot: Rc<RefCell<ContentSlot>>,
    texture: Option<wgpu::Texture>,
    generation: u64,
    cue_scope: inker::webcue::Scope,
    cue_report: inker::webcue::CapabilityReport,
    cue_renderer: Option<wgpu_cue_netrender::SceneRasterizer>,
    cue_host: Option<(genet_render_host::CoreGeneration, wgpu_cue::GpuHost)>,
}
impl ContentProducer {
    /// Keys fit the host's 62-bit producer namespace and browser integer range.
    pub fn allocate_key() -> u64 {
        static NEXT: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(0x0000_7065_0000_0000);
        NEXT.fetch_update(
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
            |n| n.checked_add(1).filter(|next| *next < (1_u64 << 53)),
        )
        .expect("content producer keys exhausted")
    }
    pub fn new(slot: Rc<RefCell<ContentSlot>>, _raster_key: u64) -> Self {
        use inker::webcue::Surface;
        let (cue_scope, cue_report) = {
            let owner = slot.borrow();
            let surface = owner
                .content
                .document()
                .expect("retained producer requires a document")
                .cue_surface();
            (surface.scope(), surface.capabilities().clone())
        };
        Self {
            cue_scope,
            cue_report,
            slot,
            texture: None,
            generation: 0,
            cue_renderer: None,
            cue_host: None,
        }
    }
    pub fn same_source(&self, slot: &Rc<RefCell<ContentSlot>>) -> bool {
        Rc::ptr_eq(&self.slot, slot)
    }
    fn scene(&mut self, frame: &ProducerFrameInfo) -> Result<Scene, String> {
        self.slot.borrow_mut().frame(
            frame.logical_size.0.max(1.),
            frame.logical_size.1.max(1.),
            frame
                .timestamp
                .map(|t| t.as_secs_f64() * 1000.)
                .unwrap_or(0.),
        )
    }
}
impl TextureProducer for ContentProducer {
    fn portable_scene(
        &mut self,
        frame: &ProducerFrameInfo,
    ) -> Result<ProducedScene, PortableProducerError> {
        let scene = self
            .scene(frame)
            .map_err(|_| PortableProducerError::InvalidSource)?;
        let slot = self.slot.borrow();
        let identity = slot
            .session_identity()
            .ok_or(PortableProducerError::InvalidSource)?;
        Ok(ProducedScene {
            scene,
            source_identity: identity.instance_id,
            source_revision: slot.revision,
            alpha: SourceAlpha::Straight,
            encoding: SourceEncoding::Srgb,
        })
    }
    fn render(&mut self, cx: &ProducerContext<'_>) -> Option<ProducedTexture> {
        self.render_checked(cx).ok().flatten()
    }
    fn render_checked(
        &mut self,
        cx: &ProducerContext<'_>,
    ) -> Result<Option<ProducedTexture>, genet_render_host::RasterizeFailure> {
        if !self.slot.borrow().visible {
            return Ok(None);
        }
        inker::webcue::Surface::set_viewport(
            self,
            inker::webcue::Viewport {
                width: cx.frame.physical_size[0],
                height: cx.frame.physical_size[1],
                scale: cx.frame.layout_scale,
            },
        )
        .map_err(|e| genet_render_host::RasterizeFailure::SourceError(e.to_string()))?;
        if !self
            .cue_host
            .as_ref()
            .is_some_and(|(generation, _)| *generation == cx.core.generation())
        {
            self.cue_renderer = Some(
                wgpu_cue_netrender::SceneRasterizer::new(core_handles(cx.core)?)
                .map_err(|e| genet_render_host::RasterizeFailure::SourceError(e.to_string()))?,
            );
            self.cue_host = Some((
                cx.core.generation(),
                wgpu_cue::GpuHost::new(cx.device.clone(), cx.queue.clone())
                    .map_err(|e| genet_render_host::RasterizeFailure::SourceError(e.to_string()))?,
            ));
        }
        let viewport = inker::webcue::Viewport {
            width: cx.frame.physical_size[0],
            height: cx.frame.physical_size[1],
            scale: cx.frame.layout_scale,
        };
        let context = self
            .cue_host
            .as_ref()
            .unwrap()
            .1
            .context(viewport)
            .map_err(|e| genet_render_host::RasterizeFailure::SourceError(e.to_string()))?;
        let polled = self
            .cue_renderer
            .as_mut()
            .unwrap()
            .poll(&context)
            .map_err(|e| genet_render_host::RasterizeFailure::SourceError(e.to_string()))?;
        let rendered = if let Some(rendered) = polled {
            rendered
        } else {
            let scene = self
                .scene(cx.frame)
                .map_err(genet_render_host::RasterizeFailure::SourceError)?;
            let mut slot = self.slot.borrow_mut();
            let identity = slot.session_identity().ok_or_else(|| {
                genet_render_host::RasterizeFailure::SourceError("missing retained receiver".into())
            })?;
            let stamp = inker::webcue::FrameStamp {
                scope: inker::webcue::Scope {
                    owner: identity.instance_id.into(),
                    generation: identity.generation,
                },
                allocation_epoch: 0,
                content_revision: slot
                    .cue_surface_mut()
                    .and_then(|surface| surface.next_frame_revision())
                    .map_err(|error| {
                        genet_render_host::RasterizeFailure::SourceError(error.to_string())
                    })?,
            };
            let context = self
                .cue_host
                .as_ref()
                .unwrap()
                .1
                .context(viewport)
                .map_err(|e| genet_render_host::RasterizeFailure::SourceError(e.to_string()))?;
            self.cue_renderer
                .as_mut()
                .unwrap()
                .render(&context, &scene, stamp)
                .map_err(|e| genet_render_host::RasterizeFailure::SourceError(e.to_string()))?
        };
        match rendered {
            wgpu_cue::Rendered::Changed(frame) => {
                self.slot
                    .borrow_mut()
                    .cue_surface_mut()
                    .and_then(|surface| surface.observe_frame())
                    .map_err(|error| {
                        genet_render_host::RasterizeFailure::SourceError(error.to_string())
                    })?;
                self.cue_report.evidence = inker::webcue::Evidence::Observed;
                self.generation = self
                    .generation
                    .checked_add(1)
                    .expect("content producer generation exhausted");
                let view = frame.texture.create_view(&Default::default());
                self.texture = Some(frame.texture);
                Ok(Some(ProducedTexture {
                    view,
                    generation: self.generation,
                    alpha: SourceAlpha::Straight,
                    encoding: SourceEncoding::Srgb,
                }))
            },
            _ => Ok(None),
        }
    }
    fn suspend(&mut self) {
        self.texture = None;
        if let Some(renderer) = &mut self.cue_renderer {
            renderer.suspend();
        }
    }
    fn retire(&mut self) {
        self.texture = None;
        self.cue_renderer = None;
        self.cue_host = None;
    }
    fn semantics(&self) -> Option<ProducerSemantics> {
        let slot = self.slot.borrow();
        if !slot.visible {
            return None;
        }
        let controller = slot.content.document()?;
        let identity = controller.session_identity();
        let mut children = vec![];
        if let Some(projection) = controller.accessibility_projection() {
            for node in projection.nodes() {
                let key = slot
                    .semantic_keys
                    .get(&(identity.instance_id, identity.generation, node.id.get()))?
                    .key;
                let actions = node
                    .actions
                    .iter()
                    .filter_map(|action| match action {
                        DocumentA11yAction::Focus
                            if !node.actions.contains(&DocumentA11yAction::Click) =>
                        {
                            Some(ProducerAction {
                                id: format!("focus/{}", projection.revision()),
                                label: "Focus".into(),
                                description: "Focus this retained document target".into(),
                            })
                        },
                        DocumentA11yAction::Click => Some(ProducerAction {
                            id: format!("activate/{}", projection.revision()),
                            label: "Activate".into(),
                            description: "Activate this retained document target".into(),
                        }),
                        _ => None,
                    })
                    .collect();
                let name = [
                    node.name.as_deref(),
                    node.value.as_deref(),
                    node.description.as_deref(),
                ]
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
                let bounds = node.bounds.unwrap_or(inker::DocumentA11yBounds {
                    x: 0.,
                    y: 0.,
                    width: 0.,
                    height: 0.,
                });
                children.push(ProducerNode {
                    key,
                    role: ProducerRole::Group,
                    name,
                    rect: [bounds.x, bounds.y, bounds.width, bounds.height],
                    actions,
                });
            }
        }
        Some(ProducerSemantics {
            role: Some(ProducerRole::Group),
            name: Some(format!("Retained content: {}", slot.source.title)),
            children,
        })
    }
    fn child_lifetime(&self, key: u64) -> Option<cambium_rootstock::ProducerChildToken> {
        self.slot.borrow().semantic_child_tokens.get(&key).cloned()
    }
    fn act(&mut self, key: u64, action: &str) -> bool {
        let Some((action, observed_revision)) = action.split_once('/') else {
            return false;
        };
        let Ok(revision) = observed_revision.parse::<u64>() else {
            return false;
        };
        let mut slot = self.slot.borrow_mut();
        if !slot.visible {
            return false;
        }
        let Some(identity) = slot.session_identity() else {
            return false;
        };
        let Some(target_id) =
            slot.semantic_keys
                .iter()
                .find_map(|(&(instance, generation, node), value)| {
                    (value.key == key
                        && instance == identity.instance_id
                        && generation == identity.generation)
                        .then_some(node)
                })
        else {
            return false;
        };
        let Some(controller) = slot.content.document_mut() else {
            return false;
        };
        let Some(projection) = controller.accessibility_projection() else {
            return false;
        };
        if projection.revision() != revision {
            return false;
        }
        let Some(target) = projection
            .nodes()
            .iter()
            .find(|node| node.id.get() == target_id)
            .map(|node| node.id)
        else {
            return false;
        };
        let advertised = projection.node(target).map(|n| &n.actions);
        if action == "focus" && advertised.is_some_and(|a| a.contains(&DocumentA11yAction::Focus)) {
            let done = controller.dispatch_accessibility_action(&DocumentA11yActionRequest {
                revision,
                target,
                action: DocumentA11yAction::Focus,
                data: None,
            });
            if done {
                slot.revision = slot.revision.wrapping_add(1);
            }
            return done;
        }
        if action != "activate"
            || !advertised.is_some_and(|a| a.contains(&DocumentA11yAction::Click))
        {
            return false;
        }
        let Some(point) = controller
            .accessibility_click_target(target)
            .filter(|p| p.revision == revision)
        else {
            return false;
        };
        for state in [SessionButtonState::Pressed, SessionButtonState::Released] {
            slot.input(SessionInput::PointerButton {
                x: point.point.x,
                y: point.point.y,
                button: SessionPointerButton::Primary,
                state,
                modifiers: SessionModifiers::default(),
            });
        }
        true
    }
}

// The shared control plane is the same retained receiver used by the native
// view callbacks. The slot keeps its held resources and application authority.
impl inker::webcue::Surface for ContentProducer {
    fn scope(&self) -> inker::webcue::Scope {
        self.cue_scope
    }
    fn capabilities(&self) -> &inker::webcue::CapabilityReport {
        &self.cue_report
    }
    fn set_viewport(&mut self, v: inker::webcue::Viewport) -> inker::webcue::Result<()> {
        self.slot.borrow_mut().cue_surface_mut()?.set_viewport(v)
    }
    fn set_visible(&mut self, visible: bool) -> inker::webcue::Result<()> {
        let mut slot = self.slot.borrow_mut();
        slot.cue_surface_mut()?.set_visible(visible)?;
        slot.visible = visible;
        Ok(())
    }
    fn input(&mut self, input: inker::webcue::Input) -> inker::webcue::Result<()> {
        let mut slot = self.slot.borrow_mut();
        if !slot.visible {
            return Err(inker::webcue::Error::Invalid(
                "retained content is hidden".into(),
            ));
        }
        slot.cue_surface_mut()?.input(input)?;
        slot.revision = slot
            .revision
            .checked_add(1)
            .ok_or(inker::webcue::Error::Exhausted)?;
        Ok(())
    }
    fn request(
        &mut self,
        id: inker::webcue::RequestId,
        command: inker::webcue::Command,
    ) -> inker::webcue::Result<inker::webcue::RequestKey> {
        self.slot
            .borrow_mut()
            .cue_surface_mut()?
            .request(id, command)
    }
    fn semantic_action(
        &mut self,
        action: inker::webcue::SemanticAction,
    ) -> inker::webcue::Result<()> {
        let mut slot = self.slot.borrow_mut();
        if !slot.visible {
            return Err(inker::webcue::Error::Stale);
        }
        slot.cue_surface_mut()?.semantic_action(action)?;
        slot.revision = slot
            .revision
            .checked_add(1)
            .ok_or(inker::webcue::Error::Exhausted)?;
        Ok(())
    }
    fn advance(&mut self, now: std::time::Duration) -> inker::webcue::Result<()> {
        let mut slot = self.slot.borrow_mut();
        slot.clock.set(now.as_secs_f64() * 1000.);
        slot.cue_surface_mut()?.advance(now)
    }
    fn poll_event(&mut self) -> Option<inker::webcue::Envelope> {
        self.slot
            .borrow_mut()
            .content
            .document_mut()?
            .poll_cue_event()
    }
    fn retire(&mut self) -> inker::webcue::Result<()> {
        self.texture = None;
        if let Some(renderer) = &mut self.cue_renderer {
            renderer.suspend();
        }
        self.slot.borrow_mut().cue_surface_mut()?.retire()
    }
}
impl wgpu_cue::TextureSource for ContentProducer {
    fn render(
        &mut self,
        cx: &wgpu_cue::RenderContext<'_>,
        _needs_frame: bool,
    ) -> inker::webcue::Result<wgpu_cue::Rendered> {
        use inker::webcue::Surface;
        self.set_viewport(cx.viewport)?;
        if !self.slot.borrow().visible {
            return Ok(wgpu_cue::Rendered::Pending);
        }
        let renderer = self.cue_renderer.as_mut().ok_or_else(|| {
            inker::webcue::Error::Invalid("attach the host core before shared rendering".into())
        })?;
        if let Some(rendered) = renderer.poll(cx)? {
            if matches!(rendered, wgpu_cue::Rendered::Changed(_)) {
                self.slot.borrow_mut().cue_surface_mut()?.observe_frame()?;
                self.cue_report.evidence = inker::webcue::Evidence::Observed;
                self.generation = self
                    .generation
                    .checked_add(1)
                    .ok_or(inker::webcue::Error::Exhausted)?;
            }
            return Ok(rendered);
        }
        let mut slot = self.slot.borrow_mut();
        let now = slot.clock.get();
        let scene = slot
            .frame(
                cx.viewport.width as f32 / cx.viewport.scale,
                cx.viewport.height as f32 / cx.viewport.scale,
                now,
            )
            .map_err(inker::webcue::Error::Backend)?;
        let stamp = inker::webcue::FrameStamp {
            scope: self.cue_scope,
            allocation_epoch: 0,
            content_revision: slot.cue_surface_mut()?.next_frame_revision()?,
        };
        let result = renderer.render(cx, &scene, stamp)?;
        Ok(result)
    }
}
/// The Core's device handles; the browser reaches its renderer only through
/// the checked accessor, which refuses while a browser frame is in flight.
fn core_handles(
    core: &genet_render_host::RenderCore,
) -> Result<wgpu_cue_netrender::WgpuHandles, genet_render_host::RasterizeFailure> {
    #[cfg(target_arch = "wasm32")]
    let handles = core.try_renderer()?.wgpu_device.core.clone();
    #[cfg(not(target_arch = "wasm32"))]
    let handles = core.renderer().wgpu_device.core.clone();
    Ok(handles)
}
impl ContentProducer {
    /// Attach a private retained renderer on the caller's existing device.
    pub fn with_core(
        slot: Rc<RefCell<ContentSlot>>,
        core: &genet_render_host::RenderCore,
    ) -> inker::webcue::Result<Self> {
        let mut producer = Self::new(slot, Self::allocate_key());
        let handles =
            core_handles(core).map_err(|e| inker::webcue::Error::Backend(e.to_string()))?;
        producer.cue_renderer = Some(wgpu_cue_netrender::SceneRasterizer::new(handles)?);
        Ok(producer)
    }
}
