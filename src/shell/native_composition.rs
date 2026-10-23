// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Stage real same-device producer views at retained CSS body apertures.
//! Source acquisition, session identity, input and accessibility remain owned
//! by their existing producers. Only their presentation joins the window scene.

use super::{CompositeLayer, Shell};
use crate::{
    app::App,
    outer_frame::OuterFrame,
    surface::{Surface, SurfaceId, SurfaceKind},
};
use genet_render_host::{RasterizeFailure, RenderCore};
use layout_dom_api::LayoutDom;
use netrender::{ColorLoad, ExternalTexturePlacement, SourceAlpha, external_image_key};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_SOURCE: AtomicU64 = AtomicU64::new(1);
static NEXT_PRODUCER: AtomicU64 = AtomicU64::new(1);
static NEXT_MASK: AtomicU64 = AtomicU64::new(1);
// Turnstone reserves this named producer range below 1<<61. Rootstock
// physical bindings reserve the upper half of the external producer domain.
// Keys are allocated, never
// hashes truncated into aliases, and never reused for another live appearance.
const PRODUCER_BASE: u64 = 0x1054_5350_0000_0000;
pub(crate) type Images = HashMap<usize, HashMap<(u128, u64), (u64, u64, [u32; 2])>>;
const MASK_BASE: u64 = 0xFFFF_5453_0000_0000;
pub(crate) type Masks = HashMap<usize, HashMap<(u128, u64), MaskState>>;
#[derive(Clone)]
pub(crate) struct MaskState {
    key: u64,
    signature: MaskSignature,
    generation: u64,
}
#[derive(Clone, PartialEq, Eq)]
struct MaskSignature {
    dim: u32,
    bounds: [u32; 4],
    corner: u32,
    blur: u32,
    invert: bool,
}
impl From<&paint_list_render::BoxShadowMaskRequest> for MaskSignature {
    fn from(mask: &paint_list_render::BoxShadowMaskRequest) -> Self {
        Self {
            dim: mask.dim,
            bounds: mask.bounds.map(f32::to_bits),
            corner: mask.corner_radius.to_bits(),
            blur: mask.blur_radius_px.to_bits(),
            invert: mask.invert,
        }
    }
}
fn allocate_mask(masks: &Masks) -> Result<u64, Failure> {
    let sequence = NEXT_MASK
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1).filter(|next| *next < 1 << 32)
        })
        .map_err(|_| Failure::Aperture(0, "shadow mask key range exhausted".into()))?;
    let key = MASK_BASE | sequence;
    if masks
        .values()
        .flat_map(|window| window.values())
        .any(|mask| mask.key == key)
    {
        return Err(Failure::Aperture(
            key,
            "shadow mask custody collision".into(),
        ));
    }
    Ok(key)
}
#[allow(clippy::too_many_arguments)]
fn stage_masks(
    masks: &mut Masks,
    slot: usize,
    session: u128,
    generation: u64,
    translated: &mut paint_list_render::TranslatedDisplayList,
    mut build: impl FnMut(&paint_list_render::BoxShadowMaskRequest),
    mut retire: impl FnMut(u64),
) -> Result<(), Failure> {
    let mut previous_masks = masks.get(&slot).cloned().unwrap_or_default();
    let mut current_masks = HashMap::new();
    for mask in &mut translated.box_shadow_masks {
        let logical_key = mask.key;
        let identity = (session, logical_key);
        let signature = MaskSignature::from(&*mask);
        let previous = previous_masks.remove(&identity);
        let key = match &previous {
            Some(previous) => previous.key,
            None => allocate_mask(masks)?,
        };
        if previous.as_ref().is_none_or(|previous| {
            previous.signature != signature || previous.generation != generation
        }) {
            let mut request = mask.clone();
            request.key = key;
            build(&request);
        }
        for op in &mut translated.scene.ops {
            if let netrender::SceneOp::Image(image) = op
                && image.key == logical_key
            {
                image.key = key;
            }
        }
        mask.key = key;
        current_masks.insert(
            identity,
            MaskState {
                key,
                signature,
                generation,
            },
        );
    }
    for mask in previous_masks.values() {
        retire(mask.key);
    }
    masks.insert(slot, current_masks);
    Ok(())
}
fn allocate(images: &Images, source: u64) -> Result<u64, Failure> {
    let sequence = NEXT_PRODUCER
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1).filter(|next| *next < 1 << 32)
        })
        .map_err(|_| Failure::Aperture(source, "native producer key range exhausted".into()))?;
    let key = PRODUCER_BASE | sequence;
    if images
        .values()
        .flat_map(|images| images.values())
        .any(|(live, _, _)| *live == key)
    {
        return Err(Failure::Aperture(
            source,
            "native producer key custody collision".into(),
        ));
    }
    Ok(key)
}
pub(super) enum Failure {
    Raster(u64, RasterizeFailure),
    Aperture(u64, String),
}
impl Shell {
    pub(super) fn refuse_native_composition(
        &mut self,
        window: u64,
        generation: u64,
        failure: Failure,
    ) {
        match failure {
            Failure::Raster(source, failure) => {
                self.refuse_raster_frame(window, generation, source, failure)
            },
            Failure::Aperture(source, reason) => {
                let error = format!(
                    "window={window} generation={generation} native source={source}: {reason}"
                );
                if !self.main_receipt.errors.contains(&error) {
                    self.main_receipt.errors.push(error.clone());
                }
                // Preserve the producer and its valid staging, but finish a
                // capture bound to this refused presentation explicitly.
                self.refuse_window_capture(
                    window,
                    cambium_rootstock::CaptureRefusal::SurfaceUnavailable { generation, error },
                );
            },
        }
    }
}
pub(super) fn retire(core: &RenderCore, images: &mut Images, masks: &mut Masks, slot: usize) {
    if let Some(previous) = images.remove(&slot) {
        for (key, _, _) in previous.values() {
            core.renderer()
                .unregister_external_image(external_image_key(*key));
        }
    }
    if let Some(previous) = masks.remove(&slot) {
        for mask in previous.values() {
            core.renderer().unregister_image_vello(mask.key);
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn compose(
    frame: &mut OuterFrame,
    app: &App,
    core: &RenderCore,
    images: &mut Images,
    masks: &mut Masks,
    slot: usize,
    surfaces: &[Surface],
    layers: Vec<CompositeLayer>,
    textures: &mut Vec<wgpu::Texture>,
    size: (u32, u32),
    mut diagnostic: Option<(
        &std::path::Path,
        &mut Vec<super::appearance_receipt::PendingCapture>,
        f64,
    )>,
) -> Result<Vec<CompositeLayer>, Failure> {
    let mut previous = images.get(&slot).cloned().unwrap_or_default();
    let mut current = HashMap::new();
    let mut commands = HashMap::new();
    let mut overlays = Vec::new();
    let mut accepted = Vec::new();
    for layer in layers {
        if matches!(layer.kind, SurfaceKind::Chrome | SurfaceKind::Divider(_)) {
            overlays.push(layer);
            continue;
        }
        let alpha = layer
            .alpha
            .clone()
            .map_err(|reason| Failure::Aperture(layer.id, reason))?;
        let surface = surfaces
            .iter()
            .find(|surface| surface.id.0 == layer.id && surface.kind == layer.kind)
            .ok_or_else(|| {
                Failure::Aperture(layer.id, "current source appearance is absent".into())
            })?;
        let node = frame
            .native_source_node(app, surface, slot)
            .ok_or_else(|| {
                Failure::Aperture(
                    layer.id,
                    "retained appearance aperture is absent or ambiguous".into(),
                )
            })?;
        let identity = (app.session_id.as_uuid().as_u128(), layer.id);
        let old = previous.remove(&identity);
        let key = match old {
            Some((key, _, _)) => key,
            None => allocate(images, layer.id)?,
        };
        let epoch = if !layer.changed
            && old.is_some_and(|(_, _, dimensions)| dimensions == layer.dimensions)
        {
            old.unwrap().1
        } else {
            NEXT_SOURCE.fetch_add(1, Ordering::Relaxed)
        };
        let dom = frame.controls.dom.borrow();
        let command = frame
            .layout
            .borrow()
            .native_source_command(&dom, node, key, epoch, surface.rect)
            .ok_or_else(|| {
                Failure::Aperture(layer.id, "retained source placement is unavailable".into())
            })?;
        commands
            .entry(dom.opaque_id(node))
            .or_insert_with(Vec::new)
            .push(command);
        current.insert(identity, (key, epoch, layer.dimensions));
        accepted.push((layer, key, epoch, alpha));
    }
    // Preflight every appearance and alpha/encoding contract before replacing
    // any prior staging. A refused source cannot corrupt another live owner.
    let mut source_metadata = Vec::new();
    for (layer, key, epoch, alpha) in accepted {
        if let Some((path, pending, window_scale)) = diagnostic.as_mut() {
            if let Some(texture) = &layer.raster {
                let readback = core
                    .start_rgba8_readback(texture, layer.dimensions[0], layer.dimensions[1])
                    .map_err(|error| {
                        Failure::Aperture(layer.id, format!("source raster diagnostic: {error}"))
                    })?;
                // Store the same-device producer pixels before staging. This
                // is never a crop or a reconstruction from final frame pixels.
                let adapter = core.renderer().wgpu_device.core.adapter.get_info();
                source_metadata.push(serde_json::json!({
                    "window":slot, "session":app.session_id.as_uuid(),
                    "surface_id":layer.id, "image_key":external_image_key(key),
                    "producer_key":key, "content_generation":epoch,
                    "device_generation":core.generation(), "dimensions":layer.dimensions,
                    "native_window_scale":*window_scale, "raster_scale":layer.raster_scale,
                    "adapter":{
                        "name":adapter.name, "backend":format!("{:?}", adapter.backend),
                        "vendor":adapter.vendor, "device":adapter.device,
                        "device_type":format!("{:?}",adapter.device_type),
                    },
                    "alpha":format!("{alpha:?}"), "format":format!("{:?}", texture.format()),
                    "file":path.with_extension(format!("source-{key}.png")).file_name().unwrap().to_string_lossy(),
                }));
                pending.push(super::appearance_receipt::PendingCapture {
                    path: path.with_extension(format!("source-{key}.png")),
                    frame: readback,
                    started: std::time::Instant::now(),
                });
            }
        }
        core.renderer().stage_external_image(
            external_image_key(key),
            &layer.view,
            layer.dimensions,
            alpha,
            epoch,
        );
    }
    for (key, _, _) in previous.values() {
        core.renderer()
            .unregister_external_image(external_image_key(*key));
    }
    images.insert(slot, current);
    if let Some((path, _, _)) = diagnostic.as_ref() {
        std::fs::write(
            path.with_extension("sources.json"),
            serde_json::to_vec_pretty(&source_metadata)
                .map_err(|error| Failure::Aperture(slot as u64, error.to_string()))?,
        )
        .map_err(|error| Failure::Aperture(slot as u64, error.to_string()))?;
    }
    frame.layout.borrow_mut().set_native_sources(commands);
    let mut translated = frame.scene_resources().map_err(|error| {
        Failure::Aperture(slot as u64, format!("frame paint translation refused: {error}"))
    })?;
    // The scene's blurred-shadow image keys refer to these actual GPU masks.
    // Dropping the side table makes correctly authored shadows disappear.
    stage_masks(
        masks,
        slot,
        app.session_id.as_uuid().as_u128(),
        core.generation(),
        &mut translated,
        |mask| {
            core.renderer().build_box_shadow_mask(
                mask.key,
                mask.dim,
                mask.bounds,
                mask.corner_radius,
                mask.blur_radius_px,
                mask.invert,
            )
        },
        |key| core.renderer().unregister_image_vello(key),
    )?;
    if let Some((path, _, _)) = diagnostic.as_ref() {
        let requests: Vec<_> = translated
            .box_shadow_masks
            .iter()
            .map(|mask| {
                serde_json::json!({
                    "key":mask.key, "dim":mask.dim, "bounds":mask.bounds,
                    "corner_radius":mask.corner_radius, "blur_radius_px":mask.blur_radius_px,
                    "invert":mask.invert, "device_generation":core.generation(),
                })
            })
            .collect();
        std::fs::write(
            path.with_extension("masks.json"),
            serde_json::to_vec_pretty(&requests)
                .map_err(|error| Failure::Aperture(slot as u64, error.to_string()))?,
        )
        .map_err(|error| Failure::Aperture(slot as u64, error.to_string()))?;
    }
    let scene = translated.scene;
    let id = SurfaceId::frame_window(slot).0;
    if let Some((path, _, _)) = diagnostic.as_ref() {
        std::fs::write(path.with_extension("scene.json"), scene.snapshot_json())
            .map_err(|error| Failure::Aperture(id, format!("native scene diagnostic: {error}")))?;
    }
    // The frame is laid out in layout pixels and rasterized at the window's
    // layout scale, onto a texture of the window's physical size.
    let (texture, view) = core
        .try_rasterize_scaled_for(
            id,
            &scene,
            size.0,
            size.1,
            ColorLoad::Clear(wgpu::Color::TRANSPARENT),
            frame.layout.borrow().scale(),
        )
        .map_err(|failure| Failure::Raster(id, failure))?;
    if let Some((path, pending, _)) = diagnostic {
        let readback = core
            .start_rgba8_readback(&texture, size.0, size.1)
            .map_err(|error| Failure::Aperture(id, format!("native raster diagnostic: {error}")))?;
        pending.push(super::appearance_receipt::PendingCapture {
            path: path.with_extension("raw.png"),
            frame: readback,
            started: std::time::Instant::now(),
        });
    }
    let raster = texture.clone();
    textures.push(texture);
    let mut output = vec![CompositeLayer {
        id,
        changed: true,
        dimensions: [size.0, size.1],
        alpha: Ok(SourceAlpha::Straight),
        kind: SurfaceKind::Frame,
        view,
        raster: Some(raster),
        raster_scale: frame.layout.borrow().scale(),
        placement: ExternalTexturePlacement::new([0.0, 0.0, size.0 as f32, size.1 as f32]),
    }];
    output.extend(overlays);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mask_scene(blur: f32) -> paint_list_render::TranslatedDisplayList {
        let logical = 0xFFFF_0000_0000_0000;
        let mut scene = netrender::Scene::default();
        scene
            .ops
            .push(netrender::SceneOp::Image(netrender::SceneImage {
                x0: 0.0,
                y0: 0.0,
                x1: 30.0,
                y1: 40.0,
                uv: [0.0, 0.0, 1.0, 1.0],
                color: [1.0; 4],
                key: logical,
                transform_id: 0,
                clip_rect: netrender::NO_CLIP,
                clip_corner_radii: [0.0; 4],
                clamp_to_uv: false,
                nearest: false,
            }));
        paint_list_render::TranslatedDisplayList {
            scene,
            external_textures: Vec::new(),
            box_shadow_masks: vec![paint_list_render::BoxShadowMaskRequest {
                key: logical,
                dim: 64,
                bounds: [3.0, 4.0, 28.0, 35.0],
                corner_radius: 2.0,
                blur_radius_px: blur,
                invert: false,
            }],
        }
    }
    #[test]
    fn mask_cache_reuses_exact_requests_and_refreshes_changed_blur_or_device() {
        let mut masks = Masks::new();
        let mut builds = Vec::new();
        let mut retired = Vec::new();
        let mut first = mask_scene(4.0);
        stage_masks(
            &mut masks,
            0,
            71,
            1,
            &mut first,
            |mask| builds.push(mask.key),
            |key| retired.push(key),
        )
        .unwrap_or_else(|_| panic!("first mask"));
        let key = first.box_shadow_masks[0].key;
        assert!(matches!(&first.scene.ops[0], netrender::SceneOp::Image(image) if image.key==key));
        for (blur, generation, expected_builds) in [(4.0, 1, 1), (9.0, 1, 2), (9.0, 2, 3)] {
            let mut scene = mask_scene(blur);
            stage_masks(
                &mut masks,
                0,
                71,
                generation,
                &mut scene,
                |mask| builds.push(mask.key),
                |key| retired.push(key),
            )
            .unwrap_or_else(|_| panic!("cached mask"));
            assert_eq!(scene.box_shadow_masks[0].key, key);
            assert_eq!(builds.len(), expected_builds);
        }
        assert!(retired.is_empty());
        let mut absent = mask_scene(9.0);
        absent.box_shadow_masks.clear();
        absent.scene.ops.clear();
        stage_masks(
            &mut masks,
            0,
            71,
            2,
            &mut absent,
            |mask| builds.push(mask.key),
            |key| retired.push(key),
        )
        .unwrap_or_else(|_| panic!("mask withdrawal"));
        assert_eq!(retired, [key]);
        assert!(masks[&0].is_empty());
    }
    #[test]
    fn equal_mask_ordinals_keep_distinct_window_and_session_custody() {
        let mut masks = Masks::new();
        let mut builds = Vec::new();
        let mut retired = Vec::new();
        let mut keys = Vec::new();
        for (window, session) in [(0, 71), (1, 71), (0, 72)] {
            let mut scene = mask_scene(4.0);
            stage_masks(
                &mut masks,
                window,
                session,
                1,
                &mut scene,
                |mask| builds.push(mask.key),
                |key| retired.push(key),
            )
            .unwrap_or_else(|_| panic!("independent mask"));
            keys.push(scene.box_shadow_masks[0].key);
        }
        assert_ne!(keys[0], keys[1]);
        assert_ne!(keys[0], keys[2]);
        assert_ne!(keys[1], keys[2]);
        assert_eq!(retired, [keys[0]], "the replaced session alone retires");
        assert_eq!(masks[&1][&(71, 0xFFFF_0000_0000_0000)].key, keys[1]);
        assert!(
            keys.iter()
                .all(|key| key & 0xFFFF_FFFF_0000_0000 == MASK_BASE)
        );
    }
    #[test]
    fn allocated_producers_preserve_custody_and_rootstock_range() {
        let mut images = Images::new();
        let first = allocate(&images, 11).unwrap_or_else(|_| panic!("first allocation"));
        images
            .entry(0)
            .or_default()
            .insert((7, 11), (first, 1, [20, 30]));
        let second = allocate(&images, 12).unwrap_or_else(|_| panic!("second allocation"));
        assert_ne!(first, second);
        assert!(first > 0 && first < 1 << 61);
        assert!(second > 0 && second < 1 << 61);
        assert_eq!(images[&0][&(7, 11)].0, first);
        // Exercise the actual renderer key conversion's domain assertion.
        let _ = external_image_key(first);
        let _ = external_image_key(second);
    }
}
