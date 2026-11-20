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
use netrender::{ExternalTexturePlacement, SourceAlpha, external_image_key};
mod pane_raster;
mod producer_image;
use pane_raster::PanePlan;
pub(super) use pane_raster::PaneRaster;
pub(crate) use producer_image::Images;
use producer_image::ProducerImage;
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
        .any(|live| live.key == key)
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
        self.cancel_native_window(window, "native composition refused");
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
        for state in previous.values() {
            state.retire(core);
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
    batch: &genet_render_host::NativeFrameSubmission,
    images: &mut Images,
    masks: &mut Masks,
    slot: usize,
    surfaces: &[Surface],
    layers: Vec<CompositeLayer>,
    textures: &mut Vec<wgpu::Texture>,
    size: (u32, u32),
    pane_cache: Option<(super::native_frames::Identity, Option<&PaneRaster>)>,
    mut diagnostic: Option<(
        &std::path::Path,
        &mut Vec<super::appearance_receipt::PendingCapture>,
        f64,
    )>,
) -> Result<(Vec<CompositeLayer>, Option<PaneRaster>), Failure> {
    let prepare_started = std::time::Instant::now();
    let mut previous = images.get(&slot).cloned().unwrap_or_default();
    let mut current = HashMap::new();
    let mut commands = HashMap::new();
    let mut overlays = Vec::new();
    let mut accepted = Vec::new();
    let reuse_composition = std::env::var_os("TURNSTONE_PROFILE_BYPASS_COMPOSITION").is_none();
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
        let direct = reuse_composition && producer_image::direct_graph_texture(&layer, alpha);
        let old = previous
            .get(&identity)
            .copied()
            .filter(|old| old.direct == direct);
        if old.is_some() {
            previous.remove(&identity);
        }
        let key = match old {
            Some(old) => old.key,
            None => allocate(images, layer.id)?,
        };
        let same_raster = old.filter(|old| !layer.changed && old.size == layer.dimensions);
        let epoch = same_raster.map_or_else(
            || NEXT_SOURCE.fetch_add(1, Ordering::Relaxed),
            |old| old.epoch,
        );
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
        let image = ProducerImage {
            key,
            epoch,
            size: layer.dimensions,
            direct,
        };
        current.insert(identity, image);
        accepted.push((layer, image, alpha, same_raster.is_none()));
    }
    // Preflight appearance and alpha/encoding before replacing staging.
    let arrange_graph = matches!(
        accepted.as_slice(), [(layer, _, _, _)] if matches!(layer.kind, SurfaceKind::Graph(_))
    ) && pane_raster::arrange_visible(app, surfaces);
    let sources_started = std::time::Instant::now();
    let direct_graph_images = accepted
        .iter()
        .filter(|(_, image, _, _)| image.direct)
        .count();
    let mut source_metadata = Vec::new();
    let mut graph_reference = None;
    for (layer, image, alpha, changed) in accepted {
        let ProducerImage { key, epoch, .. } = image;
        if let Some((path, pending, window_scale)) = diagnostic.as_mut() {
            if image.direct
                && arrange_graph
                && std::env::var_os("TURNSTONE_PROFILE_VERIFY_PANE_RASTER").is_some()
            {
                graph_reference = Some(pane_raster::GraphReference {
                    source: layer.view.clone(),
                    direct_key: external_image_key(key),
                    staged_key: external_image_key(allocate(images, layer.id)?),
                    size: layer.dimensions,
                });
            }
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
        if changed || !image.direct {
            image.stage(core, &layer, alpha);
        }
    }
    for state in previous.values() {
        state.retire(core);
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
    let paint_started = std::time::Instant::now();
    let (mut translated, paint_reused) = frame.native_scene_resources().map_err(|error| {
        Failure::Aperture(
            slot as u64,
            format!("frame paint translation refused: {error}"),
        )
    })?;
    let paint_ms = paint_started.elapsed().as_secs_f64() * 1000.0;
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
    let scale = frame.layout.borrow().scale();
    let verify_pane =
        diagnostic.is_some() && std::env::var_os("TURNSTONE_PROFILE_VERIFY_PANE_RASTER").is_some();
    let split_started = std::time::Instant::now();
    let mut split_reused = false;
    let (plan, cached) = match pane_cache {
        Some((window, cached))
            if arrange_graph
                && paint_reused
                && (diagnostic.is_none() || verify_pane)
                && std::env::var_os("TURNSTONE_PROFILE_BYPASS_PANE_RASTER").is_none()
                && core.renderer().vello_output_reuse_allowed() =>
        {
            let plan = frame
                .layout
                .borrow()
                .native_paint_identity()
                .and_then(|paint| {
                    let tile_size = core.renderer().tile_cache()?.try_lock().ok()?.tile_size();
                    if reuse_composition
                        && let Some(plan) = cached.and_then(|raster| {
                            raster.reusable_plan(window, &paint, scale, tile_size)
                        })
                    {
                        split_reused = true;
                        return Some(plan);
                    }
                    PanePlan::new(
                        window,
                        paint,
                        scale,
                        tile_size,
                        &translated,
                        reuse_composition,
                    )
                });
            (plan, cached)
        },
        _ => (None, None),
    };
    let split_ms = split_started.elapsed().as_secs_f64() * 1000.0;
    let scene = translated.scene;
    let id = SurfaceId::frame_window(slot).0;
    if let Some((path, _, _)) = diagnostic.as_ref() {
        std::fs::write(path.with_extension("scene.json"), scene.snapshot_json())
            .map_err(|error| Failure::Aperture(id, format!("native scene diagnostic: {error}")))?;
    }
    let raster_started = std::time::Instant::now();
    let scene_ops = scene.ops.len();
    let pane_raster::RasterizedFrame {
        texture,
        view,
        pane_raster,
        width: frame_raster_width,
        reference,
    } = pane_raster::rasterize(
        core,
        batch,
        id,
        scene,
        size,
        scale,
        (plan, cached),
        verify_pane,
        graph_reference,
    )?;
    let pane_raster_reused = frame_raster_width < size.0;
    tracing::debug!(target: "turnstone::shell::render", slot, batch = batch.batch_id(),
        frame_paint_ms = paint_ms, frame_encode_ms = raster_started.elapsed().as_secs_f64() * 1000.0,
        scene_ops, paint_reused, pane_raster_reused, frame_raster_width, split_ms, split_reused,
        direct_graph_images,
        aperture_ms = (sources_started - prepare_started).as_secs_f64() * 1000.0,
        source_stage_ms = (paint_started - sources_started).as_secs_f64() * 1000.0,
        "native frame profile");
    if let Some((path, pending, _)) = diagnostic {
        for (source, suffix) in [
            (Some(&texture), "raw.png"),
            (reference.as_ref(), "reference.png"),
        ] {
            let Some(source) = source else { continue };
            let readback = core
                .start_rgba8_readback(source, size.0, size.1)
                .map_err(|error| {
                    Failure::Aperture(id, format!("native raster diagnostic {suffix}: {error}"))
                })?;
            pending.push(super::appearance_receipt::PendingCapture {
                path: path.with_extension(suffix),
                frame: readback,
                started: std::time::Instant::now(),
            });
        }
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
        raster_scale: scale,
        placement: ExternalTexturePlacement::new([0.0, 0.0, size.0 as f32, size.1 as f32]),
    }];
    output.extend(overlays);
    Ok((output, pane_raster))
}

#[cfg(test)]
mod tests;
