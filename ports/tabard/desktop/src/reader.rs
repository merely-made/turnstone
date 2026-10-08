// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The real reader scene on the host's existing renderer and device.

use std::{cell::RefCell, rc::Rc};

use cambium_genet_winit_host::{
    ProducedTexture, ProducerContext, SourceAlpha, SourceEncoding, TextureProducer,
};
use cambium_rootstock::{ProducerRole, ProducerSemantics};
use netrender::ColorLoad;
use tabard_workshop::ReaderSpecimen;

const READER_RASTER_KEY: u64 = 0x7461_6261_7264_7264;

pub struct ReaderProducer {
    reader: Rc<RefCell<ReaderSpecimen>>,
    texture: Option<wgpu::Texture>,
    physical_size: [u32; 2],
    logical_size: (u32, u32),
    layout_scale: f32,
    revision: u64,
    generation: u64,
}

impl ReaderProducer {
    pub fn new(reader: Rc<RefCell<ReaderSpecimen>>) -> Self {
        Self {
            reader,
            texture: None,
            physical_size: [0, 0],
            logical_size: (0, 0),
            layout_scale: 0.0,
            revision: 0,
            generation: 0,
        }
    }

    /// Reopening the library replaces the workshop state. Bind its new
    /// appearance without retaining pixels from the old reader instance.
    pub fn set_reader(&mut self, reader: Rc<RefCell<ReaderSpecimen>>) -> bool {
        if Rc::ptr_eq(&self.reader, &reader) {
            return false;
        }
        self.reader = reader;
        self.revision = 0;
        self.texture = None;
        true
    }
}

impl TextureProducer for ReaderProducer {
    fn render(&mut self, cx: &ProducerContext<'_>) -> Option<ProducedTexture> {
        let logical_size = (
            cx.frame.logical_size.0.round().max(1.0) as u32,
            cx.frame.logical_size.1.round().max(1.0) as u32,
        );
        let mut reader = self.reader.borrow_mut();
        if !cx.frame.needs_frame
            && self.texture.is_some()
            && self.revision == reader.revision()
            && self.physical_size == cx.frame.physical_size
            && self.logical_size == logical_size
            && self.layout_scale == cx.frame.layout_scale
        {
            return None;
        }
        let scene = reader.frame(logical_size.0, logical_size.1);
        let [width, height] = cx.frame.physical_size;
        let (texture, view) = cx.core.rasterize_scaled_for(
            READER_RASTER_KEY,
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
        self.revision = reader.revision();
        self.generation = self
            .generation
            .checked_add(1)
            .expect("reader generation exhausted");
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
            name: Some(self.reader.borrow().accessible_name().to_owned()),
            children: Vec::new(),
        })
    }
}
