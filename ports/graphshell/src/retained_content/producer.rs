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
    DocumentA11yAction, DocumentA11yActionRequest, DocumentA11yNodeId, SessionButtonState,
    SessionModifiers, SessionPointerButton,
};

/// A retained Pelt document painted on the embedding host's own device/core.
pub struct ContentProducer {
    slot: Rc<RefCell<ContentSlot>>,
    raster_key: u64,
    texture: Option<wgpu::Texture>,
    generation: u64,
    keys: BTreeMap<(u64, u64, u64), u64>,
    actions: BTreeMap<u64, (pelt_core::PeltSessionIdentity, u64, DocumentA11yNodeId)>,
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
    pub fn new(slot: Rc<RefCell<ContentSlot>>, raster_key: u64) -> Self {
        Self {
            slot,
            raster_key,
            texture: None,
            generation: 0,
            keys: BTreeMap::new(),
            actions: BTreeMap::new(),
        }
    }
    pub fn same_source(&self, slot: &Rc<RefCell<ContentSlot>>) -> bool {
        Rc::ptr_eq(&self.slot, slot)
    }
    fn scene(&mut self, frame: &ProducerFrameInfo) -> Result<Scene, PortableProducerError> {
        self.slot
            .borrow_mut()
            .frame(
                frame.logical_size.0.max(1.),
                frame.logical_size.1.max(1.),
                frame
                    .timestamp
                    .map(|t| t.as_secs_f64() * 1000.)
                    .unwrap_or(0.),
            )
            .map_err(|_| PortableProducerError::InvalidSource)
    }
}
impl TextureProducer for ContentProducer {
    fn portable_scene(
        &mut self,
        frame: &ProducerFrameInfo,
    ) -> Result<ProducedScene, PortableProducerError> {
        let scene = self.scene(frame)?;
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
        let Ok(scene) = self.scene(cx.frame) else {
            return Ok(None);
        };
        let [width, height] = cx.frame.physical_size;
        let (texture, view) = cx.core.try_rasterize_scaled_for(
            self.raster_key,
            &scene,
            width,
            height,
            netrender::ColorLoad::Clear(wgpu::Color::TRANSPARENT),
            cx.frame.layout_scale,
        )?;
        self.texture = Some(texture);
        self.generation = self
            .generation
            .checked_add(1)
            .expect("content producer generation exhausted");
        Ok(Some(ProducedTexture {
            view,
            generation: self.generation,
            alpha: SourceAlpha::Straight,
            encoding: SourceEncoding::Srgb,
        }))
    }
    fn suspend(&mut self) {
        self.texture = None;
        self.actions.clear();
    }
    fn semantics(&mut self) -> Option<ProducerSemantics> {
        self.actions.clear();
        let slot = self.slot.borrow();
        if !slot.visible {
            return None;
        }
        let controller = slot.content.document()?;
        let identity = controller.session_identity();
        let mut children = vec![];
        if let Some(projection) = controller.accessibility_projection() {
            for node in projection.nodes() {
                let key = *self
                    .keys
                    .entry((identity.instance_id, identity.generation, node.id.get()))
                    .or_insert_with(|| {
                        static NEXT: std::sync::atomic::AtomicU64 =
                            std::sync::atomic::AtomicU64::new(1);
                        NEXT.fetch_update(
                            std::sync::atomic::Ordering::Relaxed,
                            std::sync::atomic::Ordering::Relaxed,
                            |n| n.checked_add(1),
                        )
                        .expect("content action keys exhausted")
                    });
                self.actions
                    .insert(key, (identity, projection.revision(), node.id));
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
    fn act(&mut self, key: u64, action: &str) -> bool {
        let Some(&(identity, revision, target)) = self.actions.get(&key) else {
            return false;
        };
        let Some((action, observed_revision)) = action.split_once('/') else {
            return false;
        };
        if observed_revision.parse::<u64>().ok() != Some(revision) {
            return false;
        }
        let mut slot = self.slot.borrow_mut();
        if !slot.visible || slot.session_identity() != Some(identity) {
            return false;
        }
        let Some(controller) = slot.content.document_mut() else {
            return false;
        };
        let Some(projection) = controller.accessibility_projection() else {
            return false;
        };
        if projection.revision() != revision {
            return false;
        }
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
