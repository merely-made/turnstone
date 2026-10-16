// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Pelt content inside a Mere projection. The caller supplies admitted held
//! bytes; reading never acquires a remote document or changes graph authority.
use inker::{
    Block, DocumentProvenance, DocumentTrustState, EngineDocument, EngineProfileBinding,
    InlineSpan, SessionSpawnRequest,
};
pub use inker::{
    SessionButtonState, SessionIme, SessionInput, SessionKey, SessionModifiers,
    SessionPointerButton, SessionScrollKey,
};
use netrender::Scene;
use pelt_core::{
    PeltClock, PeltContent, PeltHostEffect, PeltLayer, PeltRegistries, PeltTileRequest,
    WorkspaceRect,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
use uuid::Uuid;

mod view;
pub use view::{content_view, filling_content_view};
mod producer;
pub use producer::ContentProducer;
pub mod fixture;
mod export;
pub use export::ClipExportScope;

/// Admit an explicitly selected package through its Fleece owner, then add the
/// retained reference through the existing Mere owner. No destination fetch.
pub async fn import_clip<B: muniment::Backend + Clone>(
    app: &mut crate::app::GraphshellApp<B>,
    bytes: &[u8],
    limits: mere_document_lanes::retained_clip::ClipLimits,
    context: &fleece::portable_clip::CaptureContext,
) -> Result<Uuid, String> {
    let wire_limit = limits
        .max_bytes
        .checked_mul(16)
        .and_then(|n| n.checked_add(limits.max_record_bytes as u64))
        .ok_or("Clip wire budget overflow")?;
    if bytes.len() as u64 > wire_limit {
        return Err("Clip package exceeds its configured wire budget".into());
    }
    let package: fleece::portable_clip::ClipPackageV1 =
        serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let mut staging = eidetic::MemoryBackend::new();
    let staged =
        fleece::portable_clip::import_clip_package(&mut staging, None, &package, context, limits)
            .await
            .map_err(|e| e.to_string())?;
    mere_document_lanes::retained_clip::read_store(&mut staging, None, staged, limits).await?;
    let mut store = app.host.store();
    let id =
        fleece::portable_clip::import_clip_package(&mut store, None, &package, context, limits)
            .await
            .map_err(|e| e.to_string())?;
    let address = mere_document_lanes::retained_clip::address(id);
    let member = app
        .host
        .create_address(&address, &package.clip.title)
        .map_err(|e| format!("Clip retained at {address}; reference creation needs retry: {e}"))?;
    Ok(member)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeldMedia {
    pub clip: eidetic::ManifestId,
    pub media_type: String,
    pub bytes: Vec<u8>,
}
impl HeldMedia {
    pub fn data_url(&self) -> String {
        use base64::Engine;
        format!(
            "data:{};base64,{}",
            self.media_type,
            base64::engine::general_purpose::STANDARD.encode(&self.bytes)
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeldContent {
    pub address: String,
    pub title: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
    /// Explicit choice among the caller's registered engines, e.g. Reader.
    pub engine: Option<String>,
    pub media: Option<HeldMedia>,
}
impl HeldContent {
    pub fn text(
        address: impl Into<String>,
        title: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            address: address.into(),
            title: title.into(),
            media_type: "text/plain".into(),
            bytes: text.into().into_bytes(),
            engine: None,
            media: None,
        }
    }
    pub fn clip(view: &mere_document_lanes::retained_clip::ClipView) -> Result<Self, String> {
        let media = match &view.body {
            mere_document_lanes::retained_clip::ClipBody::Voice(bytes) => Some(HeldMedia {
                clip: view.id,
                media_type: "audio/wav".into(),
                bytes: bytes.clone(),
            }),
            _ => None,
        };
        Ok(Self {
            address: mere_document_lanes::retained_clip::address(view.id),
            title: view.clip.title.clone(),
            media_type: "application/vnd.mere.clip+json".into(),
            bytes: serde_json::to_vec(&view.document()).map_err(|e| e.to_string())?,
            engine: None,
            media,
        })
    }
    fn request(&self) -> Result<PeltTileRequest, String> {
        let mime = self
            .media_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let (body, content_type) = match mime.as_str() {
            "text/plain" => {
                let text =
                    std::str::from_utf8(&self.bytes).map_err(|_| "Held text is not UTF-8")?;
                let document = EngineDocument {
                    address: self.address.clone(),
                    title: Some(self.title.clone()),
                    content_type: self.media_type.clone(),
                    lang: None,
                    provenance: DocumentProvenance::for_engine(
                        pelt_core::native::DOCUMENT_ENGINE_ID,
                        &self.address,
                    ),
                    trust: DocumentTrustState::Unknown,
                    diagnostics: vec![],
                    navigation: Default::default(),
                    blocks: vec![
                        Block::Heading {
                            level: 1,
                            spans: vec![InlineSpan::Text(self.title.clone())],
                        },
                        Block::Paragraph {
                            spans: vec![InlineSpan::Text(text.into())],
                        },
                    ],
                };
                (
                    serde_json::to_string(&document).map_err(|e| e.to_string())?,
                    "application/vnd.mere.document+json".to_string(),
                )
            },
            "text/html"
            | "application/xhtml+xml"
            | "application/vnd.mere.clip+json"
            | "application/vnd.mere.document+json" => (
                String::from_utf8(self.bytes.clone()).map_err(|_| "Held document is not UTF-8")?,
                mime,
            ),
            "image/png" | "image/jpeg" | "image/gif" | "image/webp" => {
                use base64::Engine;
                // Only the content-addressed held bytes become the image source.
                // A remote-looking document address is not a fetch instruction.
                let encoded = base64::engine::general_purpose::STANDARD.encode(&self.bytes);
                (
                    format!(
                        "<!doctype html><html><body><img alt=\"Retained image\" src=\"data:{mime};base64,{encoded}\"></body></html>"
                    ),
                    "text/html".into(),
                )
            },
            _ if self.engine.is_some() => (
                String::from_utf8(self.bytes.clone())
                    .map_err(|_| "This document engine requires a held UTF-8 body")?,
                mime,
            ),
            _ => {
                return Err(format!(
                    "No registered retained-content adapter for {}",
                    self.media_type
                ));
            },
        };
        let mut request = PeltTileRequest::from_request(
            SessionSpawnRequest::new(&self.address)
                .with_body(body)
                .with_content_type(content_type)
                .with_viewport(640, 480),
        );
        request.engine_override = self.engine.clone();
        Ok(request)
    }
}

struct Clock(Rc<Cell<f64>>);
impl PeltClock for Clock {
    fn now_ms(&self) -> f64 {
        self.0.get()
    }
}

pub struct ContentSlot {
    pub(crate) content: PeltContent<Scene>,
    source: HeldContent,
    pub(crate) clock: Rc<Cell<f64>>,
    pub(crate) revision: u64,
    pub(crate) effects: Vec<PeltHostEffect>,
    pub(crate) visible: bool,
    glyph_runs: usize,
}
impl ContentSlot {
    pub fn glyph_runs(&self) -> usize {
        self.glyph_runs
    }
    pub fn is_visible(&self) -> bool {
        self.visible
    }
    pub fn scroll_key(&mut self, key: SessionScrollKey) -> bool {
        if !self.visible {
            return false;
        }
        let changed = self.content.scroll_for_key(key);
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }
    pub fn source(&self) -> &HeldContent {
        &self.source
    }
    pub fn session_identity(&self) -> Option<pelt_core::PeltSessionIdentity> {
        self.content.session_identity()
    }
    pub fn input(&mut self, input: SessionInput) -> bool {
        if !self.visible {
            return false;
        }
        let effect = self.content.input(input, 1.0);
        let changed = effect.redraw
            || effect.navigation.is_some()
            || effect.submission.is_some()
            || effect.error.is_some();
        let handled = effect.handled || changed;
        if effect.navigation.is_some()
            || effect.submission.is_some()
            || effect.error.is_some()
            || effect.download.is_some()
        {
            self.effects.push(effect);
        }
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        handled
    }
    pub fn scroll(&mut self, dx: f32, dy: f32) -> bool {
        if !self.visible {
            return false;
        }
        let changed = self
            .content
            .document_mut()
            .is_some_and(|d| d.scroll_by(dx, dy));
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }
    pub fn take_effects(&mut self) -> Vec<PeltHostEffect> {
        std::mem::take(&mut self.effects)
    }
    pub fn frame(&mut self, width: f32, height: f32, now_ms: f64) -> Result<Scene, String> {
        if !self.visible {
            return Err("Retained content is hidden".into());
        }
        self.clock.set(now_ms);
        if self.content.pump() {
            self.revision = self.revision.wrapping_add(1);
        }
        match self.content.frame(
            WorkspaceRect {
                x: 0.,
                y: 0.,
                width,
                height,
            },
            1.,
        ) {
            PeltLayer::Document(scene) => {
                self.glyph_runs = scene
                    .ops
                    .iter()
                    .filter(|op| matches!(op, netrender::SceneOp::GlyphRun(_)))
                    .count();
                Ok(scene)
            },
            _ => Err("This content needs the host's external surface compositor".into()),
        }
    }
}

/// Per-access retained controllers. Arrangement remains Forme/Workbench-owned.
pub struct ContentHost {
    registries: PeltRegistries<Scene>,
    slots: BTreeMap<Uuid, Rc<RefCell<ContentSlot>>>,
}
impl Default for ContentHost {
    fn default() -> Self {
        Self::with_native_fonts(Vec::new())
    }
}

impl ContentHost {
    pub fn with_native_fonts(fonts: Vec<Vec<u8>>) -> Self {
        Self::with_native_presentation(fonts, None)
    }
    pub fn with_native_presentation(fonts: Vec<Vec<u8>>, family: Option<String>) -> Self {
        Self::new(
            pelt_core::PeltRegistries::new(
                pelt_core::native::sessions_with_presentation(
                    pelt_core::native::HeldResources::default(),
                    fonts,
                    family,
                ),
                inker::SurfaceEngineRegistry::new(),
                pelt_core::native::route_policy(),
                "graphshell.retained-content",
                inker::routing::ENGINE_GENET_LIVERY,
                EngineProfileBinding {
                    user_data_dir: String::new(),
                },
            )
            .with_host_history()
            .with_host_loading(),
        )
    }
    /// The existing Fleece/Eidetic owner verifies exact held parts without
    /// fetching. Its private sealer and custody backend remain caller supplied.
    pub async fn open_clip(
        &mut self,
        access: Uuid,
        store: &mut dyn eidetic::Store,
        sealer: Option<&dyn eidetic::seal::PayloadSealer>,
        id: eidetic::ManifestId,
        limits: mere_document_lanes::retained_clip::ClipLimits,
    ) -> Result<Rc<RefCell<ContentSlot>>, String> {
        let view =
            mere_document_lanes::retained_clip::read_store(store, sealer, id, limits).await?;
        self.open(access, HeldContent::clip(&view)?)
    }
    pub fn new(registries: PeltRegistries<Scene>) -> Self {
        Self {
            registries,
            slots: BTreeMap::new(),
        }
    }
    pub fn get(&self, access: Uuid) -> Option<Rc<RefCell<ContentSlot>>> {
        self.slots.get(&access).cloned()
    }
    pub fn len(&self) -> usize {
        self.slots.len()
    }
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
    pub fn open(
        &mut self,
        access: Uuid,
        source: HeldContent,
    ) -> Result<Rc<RefCell<ContentSlot>>, String> {
        if let Some(slot) = self.slots.get(&access)
            && slot.borrow().source == source
        {
            return Ok(slot.clone());
        }
        let request = source.request()?;
        if request.request.body.is_none() {
            return Err("Retained content requires held bytes".into());
        }
        let clock = Rc::new(Cell::new(0.));
        let clock_for = clock.clone();
        let content = PeltContent::routed(
            self.registries.clone(),
            request,
            std::sync::Arc::new(move || Box::new(Clock(clock_for.clone()))),
        )?;
        if content.document().is_none() {
            return Err("Retained content requires a registered document session".into());
        }
        let slot = Rc::new(RefCell::new(ContentSlot {
            content,
            source,
            clock,
            revision: 0,
            effects: Vec::new(),
            visible: false,
            glyph_runs: 0,
        }));
        if let Some(old) = self.slots.insert(access, slot.clone()) {
            old.borrow_mut().visible = false;
            old.borrow_mut().content.set_hidden(true);
        }
        Ok(slot)
    }
    /// Hidden tabs retain their session but receive neither frames nor input.
    pub fn show_only(&mut self, visible: &[Uuid]) {
        for (access, slot) in &self.slots {
            let mut slot = slot.borrow_mut();
            let next = visible.contains(access);
            if slot.visible != next {
                slot.visible = next;
                slot.content.set_hidden(!next);
            }
        }
    }
    pub fn close(&mut self, access: Uuid) {
        if let Some(slot) = self.slots.remove(&access) {
            slot.borrow_mut().visible = false;
            slot.borrow_mut().content.set_hidden(true);
        }
    }
}

#[cfg(test)]
mod tests;
