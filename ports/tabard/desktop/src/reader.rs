// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Portable preview scenes on the host's existing renderer and device.

use std::{cell::RefCell, rc::Rc};

use cambium_genet_winit_host::{
    ProducedTexture, ProducerContext, SourceAlpha, SourceEncoding, TextureProducer,
};
use cambium_rootstock::{ProducerRole, ProducerSemantics};
use netrender::ColorLoad;
use tabard_workshop::{PreviewScene, ReaderSpecimen};

pub const READER_RASTER_KEY: u64 = 0x7461_6261_7264_7264;

pub type ReaderProducer = ScenePreviewProducer<ReaderSpecimen>;

pub struct ScenePreviewProducer<T: PreviewScene> {
    preview: Rc<RefCell<T>>,
    raster_key: u64,
    texture: Option<wgpu::Texture>,
    physical_size: [u32; 2],
    logical_size: (u32, u32),
    layout_scale: f32,
    revision: u64,
    generation: u64,
}

impl<T: PreviewScene> ScenePreviewProducer<T> {
    pub fn new(preview: Rc<RefCell<T>>, raster_key: u64) -> Self {
        Self {
            preview,
            raster_key,
            texture: None,
            physical_size: [0, 0],
            logical_size: (0, 0),
            layout_scale: 0.0,
            revision: 0,
            generation: 0,
        }
    }

    /// Reopening the library replaces the workshop state. Bind its new
    /// appearance without retaining pixels from the old preview instance.
    pub fn set_preview(&mut self, preview: Rc<RefCell<T>>) -> bool {
        if Rc::ptr_eq(&self.preview, &preview) {
            return false;
        }
        self.preview = preview;
        self.revision = 0;
        self.texture = None;
        true
    }
}

impl<T: PreviewScene> TextureProducer for ScenePreviewProducer<T> {
    fn render(&mut self, cx: &ProducerContext<'_>) -> Option<ProducedTexture> {
        let logical_size = (
            cx.frame.logical_size.0.round().max(1.0) as u32,
            cx.frame.logical_size.1.round().max(1.0) as u32,
        );
        let mut preview = self.preview.borrow_mut();
        if !cx.frame.needs_frame
            && self.texture.is_some()
            && self.revision == preview.revision()
            && self.physical_size == cx.frame.physical_size
            && self.logical_size == logical_size
            && self.layout_scale == cx.frame.layout_scale
        {
            return None;
        }
        let scene = preview.frame(logical_size.0, logical_size.1);
        let [width, height] = cx.frame.physical_size;
        let (texture, view) = cx.core.rasterize_scaled_for(
            self.raster_key,
            &scene,
            width,
            height,
            ColorLoad::Clear(wgpu::Color::TRANSPARENT),
            cx.frame.layout_scale,
        );
        self.texture = Some(texture);
        self.physical_size = cx.frame.physical_size;
        self.logical_size = logical_size;
        self.layout_scale = cx.frame.layout_scale;
        self.revision = preview.revision();
        self.generation = self
            .generation
            .checked_add(1)
            .expect("preview generation exhausted");
        Some(ProducedTexture {
            view,
            generation: self.generation,
            alpha: SourceAlpha::Straight,
            encoding: SourceEncoding::Srgb,
        })
    }

    fn suspend(&mut self) {
        self.texture = None;
    }

    fn semantics(&mut self) -> Option<ProducerSemantics> {
        Some(ProducerSemantics {
            role: Some(ProducerRole::Image),
            name: Some(self.preview.borrow().accessible_name().to_owned()),
            children: Vec::new(),
        })
    }
}

impl ScenePreviewProducer<ReaderSpecimen> {
    pub fn set_reader(&mut self, reader: Rc<RefCell<ReaderSpecimen>>) -> bool {
        self.set_preview(reader)
    }
}
