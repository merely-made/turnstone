// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Bind the portable preview scene API to the existing shared host adapter.

use std::{cell::RefCell, rc::Rc};

use cambium_genet_winit_host::SceneProducer;
use cambium_rootstock::{ProducerRole, ProducerSemantics};
use tabard_workshop::{PreviewScene, ReaderSpecimen};

pub const READER_RASTER_KEY: u64 = 0x7461_6261_7264_7264;
pub const STYLESHEET_RASTER_KEY: u64 = 0x7461_6261_7264_6373;

pub type ReaderProducer = SceneProducer<ReaderSpecimen>;
pub type ScenePreviewProducer<T> = SceneProducer<T>;

pub fn scene_producer<T: PreviewScene>(
    preview: Rc<RefCell<T>>,
    raster_key: u64,
) -> SceneProducer<T> {
    SceneProducer::new(preview, raster_key, T::frame, T::revision, |source| {
        Some(ProducerSemantics {
            role: Some(ProducerRole::Image),
            name: Some(source.accessible_name().to_owned()),
            children: Vec::new(),
        })
    })
}
