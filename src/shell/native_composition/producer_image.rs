// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{CompositeLayer, RenderCore, SurfaceKind};
use netrender::{SourceAlpha, external_image_key};
use std::{collections::HashMap, sync::Arc};

pub(crate) type Images = HashMap<usize, HashMap<(u128, u64), ProducerImage>>;

#[derive(Clone, Copy)]
pub(crate) struct ProducerImage {
    pub(super) key: u64,
    pub(super) epoch: u64,
    pub(super) size: [u32; 2],
    pub(super) direct: bool,
}

/// A graph layer comes from the native batch's straight RGBA8 scene raster
/// and uses the full, unorm view of that texture. The staging pass would
/// sample the same mip at the same size with nearest filtering. Imported
/// sources still go through staging for format and alpha normalization.
pub(super) fn direct_graph_texture(layer: &CompositeLayer, alpha: SourceAlpha) -> bool {
    matches!(layer.kind, SurfaceKind::Graph(_))
        && alpha == SourceAlpha::Straight
        && layer.raster.as_ref().is_some_and(|texture| {
            texture == layer.view.texture()
                && [texture.width(), texture.height()] == layer.dimensions
                && texture.format() == wgpu::TextureFormat::Rgba8Unorm
                && texture.usage().contains(wgpu::TextureUsages::COPY_SRC)
        })
}

impl ProducerImage {
    pub(super) fn stage(self, core: &RenderCore, layer: &CompositeLayer, alpha: SourceAlpha) {
        let key = external_image_key(self.key);
        if self.direct {
            core.renderer()
                .insert_image_vello(key, Arc::new(layer.view.texture().clone()));
        } else {
            core.renderer()
                .stage_external_image(key, &layer.view, self.size, alpha, self.epoch);
        }
    }

    pub(super) fn retire(self, core: &RenderCore) {
        let key = external_image_key(self.key);
        if self.direct {
            core.renderer().unregister_image_vello(key);
        } else {
            core.renderer().unregister_external_image(key);
        }
    }
}
