// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use crate::shell::native_frames::Identity;
use genet_render_host::{NativeFrameSubmission, RenderCore};
use netrender::{
    ColorLoad, ImageKey, Scene, SceneBlendMode, SceneClip, SceneCompose, SceneOp, SourceAlpha,
};
use paint_list_render::TranslatedDisplayList;
use std::rc::Rc;

pub(super) fn arrange_visible(app: &crate::app::App, surfaces: &[crate::surface::Surface]) -> bool {
    surfaces.iter().any(|surface| {
        let crate::surface::SurfaceKind::Pane(id) = surface.kind else {
            return false;
        };
        matches!(app.pane_content(id), Some(crate::panes::PaneContent::Registered(kind))
            if kind.as_str() == crate::panes::kind::ARRANGE)
    })
}

pub(super) struct RasterizedFrame {
    pub(super) texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    pub(super) pane_raster: Option<PaneRaster>,
    pub(super) width: u32,
    pub(super) reference: Option<wgpu::Texture>,
}

pub(super) struct GraphReference {
    pub(super) source: wgpu::TextureView,
    pub(super) direct_key: ImageKey,
    pub(super) staged_key: ImageKey,
    pub(super) size: [u32; 2],
}

impl GraphReference {
    fn draw(
        self,
        core: &RenderCore,
        scene: &Scene,
        draw: impl FnOnce(&Scene) -> Result<wgpu::Texture, super::Failure>,
    ) -> Result<wgpu::Texture, super::Failure> {
        let mut staged_scene = scene.clone();
        for op in &mut staged_scene.ops {
            if let SceneOp::Image(image) = op
                && image.key == self.direct_key
            {
                image.key = self.staged_key;
            }
        }
        let renderer = core.renderer();
        // The full-frame oracle samples the same producer through the
        // original normalization pass under a separate temporary image key.
        renderer.stage_external_image(
            self.staged_key,
            &self.source,
            self.size,
            SourceAlpha::Straight,
            0,
        );
        let reference = draw(&staged_scene);
        renderer.unregister_external_image(self.staged_key);
        reference
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn rasterize(
    core: &RenderCore,
    batch: &NativeFrameSubmission,
    id: u64,
    mut scene: Scene,
    size: (u32, u32),
    scale: f32,
    (plan, cached): (Option<PanePlan>, Option<&PaneRaster>),
    verify: bool,
    graph_reference: Option<GraphReference>,
) -> Result<RasterizedFrame, super::Failure> {
    let draw = |id, scene: &Scene, width| {
        batch
            .rasterize(
                id,
                scene,
                width,
                size.1,
                ColorLoad::Clear(wgpu::Color::TRANSPARENT),
                scale,
            )
            .map_err(|failure| super::Failure::Raster(id, failure))
    };
    if let Some(plan) = plan.as_ref()
        && let Some(raster) = cached.filter(|raster| raster.matches(plan))
    {
        let reference = verify
            .then(|| {
                let draw = |scene: &Scene| draw(id, scene, size.0).map(|(texture, _)| texture);
                match graph_reference {
                    Some(source) => source.draw(core, &scene, draw),
                    None => draw(&scene),
                }
            })
            .transpose()?;
        // The cropped frame has its own tile state. Replace just the graph
        // side of the assembled frame so presentation samples it once.
        scene.viewport_width = plan.split_layout;
        let cropped = plan.cropped.as_deref().unwrap_or(&scene);
        let raster_started = std::time::Instant::now();
        let (left, _) = draw(0x0054_504E_0000_0000, cropped, plan.split_px)?;
        let raster_done = std::time::Instant::now();
        raster.update_graph(core, &left);
        let copy_ms = raster_done.elapsed().as_secs_f64() * 1000.0;
        if tracing::enabled!(target: "turnstone::shell::render", tracing::Level::DEBUG)
            && let Some(timings) = core.renderer().last_frame_timings()
        {
            let ms = |span| timings.span(span).unwrap_or_default().as_secs_f64() * 1000.0;
            tracing::debug!(target: "turnstone::shell::render", batch = batch.batch_id(),
                left_raster_ms = (raster_done - raster_started).as_secs_f64() * 1000.0,
                copy_ms, renderer_ms = timings.total.as_secs_f64() * 1000.0,
                cropped_ops = cropped.ops.len(),
                image_refresh_ms = ms("refresh_image_data"),
                tile_invalidate_ms = ms("tile_invalidate"),
                tile_rebuild_ms = ms("dirty_tile_rebuild"),
                master_compose_ms = ms("master_compose"),
                vello_render_ms = ms("vello_render"),
                "native cropped raster profile");
        }
        return Ok(RasterizedFrame {
            texture: raster.pixels.clone(),
            view: raster.view.clone(),
            pane_raster: Some(raster.clone()),
            width: plan.split_px,
            reference,
        });
    }
    let (texture, view) = draw(id, &scene, size.0)?;
    let pane_raster = plan.map(|plan| plan.retain(core, &texture));
    Ok(RasterizedFrame {
        texture,
        view,
        pane_raster,
        width: size.0,
        reference: None,
    })
}

#[derive(Clone)]
pub(in crate::shell) struct PaneRaster {
    plan: PanePlan,
    pixels: wgpu::Texture,
    view: wgpu::TextureView,
}

#[derive(Clone)]
pub(super) struct PanePlan {
    window: Identity,
    paint: Rc<()>,
    scale: f32,
    tile_size: u32,
    pub(super) split_layout: u32,
    pub(super) split_px: u32,
    cropped: Option<Rc<Scene>>,
}

impl PanePlan {
    pub(super) fn new(
        window: Identity,
        paint: Rc<()>,
        scale: f32,
        tile_size: u32,
        translated: &TranslatedDisplayList,
        retain_crop: bool,
    ) -> Option<Self> {
        let [graph] = translated.external_textures.as_slice() else {
            return None;
        };
        let scene = &translated.scene;
        if !scale.is_finite()
            || scale <= 0.0
            || tile_size == 0
            || !translated.box_shadow_masks.is_empty()
            || !scene.compositor_surfaces.is_empty()
            || scene.root_alpha != 1.0
            || scene.root_blend_mode != SceneBlendMode::Normal
        {
            return None;
        }
        let mut graph_edge = None;
        for op in &scene.ops {
            match op {
                SceneOp::Image(image)
                    if image.key == graph.image_key
                        && image.transform_id == 0
                        && image.x1.is_finite()
                        && image.x1 > 0.0
                        && graph_edge.is_none() =>
                {
                    graph_edge = Some(image.x1);
                },
                SceneOp::Image(_) | SceneOp::Pattern(_) | SceneOp::Fragment(_) => return None,
                SceneOp::PushLayer(layer)
                    if layer.backdrop_filter.is_some()
                        || !layer.filters.is_empty()
                        || matches!(layer.clip, SceneClip::None) =>
                {
                    return None;
                },
                _ => {},
            }
        }
        // Leave an antialiasing margin around the moving image and keep the
        // cropped raster on the original tile grid. All remaining paint here
        // is pixel-local, so the right side is independent of the image.
        let edge = graph_edge?;
        let split_layout = ((edge / tile_size as f32).floor() as u32)
            .checked_add(1)?
            .checked_mul(tile_size)?;
        let split = split_layout as f32 * scale;
        if split_layout >= scene.viewport_width
            || split.fract() != 0.0
            || split - edge * scale < 1.0
            || split >= window.dimensions.0 as f32
        {
            return None;
        }
        Some(Self {
            window,
            paint,
            scale,
            tile_size,
            split_layout,
            split_px: split as u32,
            cropped: retain_crop.then(|| {
                let mut cropped = scene.clone();
                cropped.viewport_width = split_layout;
                prune_outside(
                    &mut cropped,
                    window.dimensions.1 as f32 / scale,
                    2.0 / scale,
                );
                Rc::new(cropped)
            }),
        })
    }

    pub(super) fn retain(self, core: &RenderCore, full: &wgpu::Texture) -> PaneRaster {
        let (width, height) = self.window.dimensions;
        let size = extent(width, height);
        let pixels = core.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("Turnstone assembled frame"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = pixels.create_view(&Default::default());
        let mut encoder = core.device().create_command_encoder(&Default::default());
        encoder.copy_texture_to_texture(full.as_image_copy(), pixels.as_image_copy(), size);
        core.queue().submit([encoder.finish()]);
        PaneRaster {
            plan: self,
            pixels,
            view,
        }
    }
}

impl PaneRaster {
    /// Paint identity covers every scene operation and source placement. A
    /// same-sized graph updates only its pixels, so the accepted split proof
    /// can be carried forward on the same window and renderer tile grid.
    pub(super) fn reusable_plan(
        &self,
        window: Identity,
        paint: &Rc<()>,
        scale: f32,
        tile_size: u32,
    ) -> Option<PanePlan> {
        let plan = &self.plan;
        (plan.window == window
            && plan.scale == scale
            && plan.tile_size == tile_size
            && Rc::ptr_eq(&plan.paint, paint))
        .then(|| plan.clone())
    }

    pub(super) fn matches(&self, plan: &PanePlan) -> bool {
        self.plan.window == plan.window
            && self.plan.scale == plan.scale
            && self.plan.tile_size == plan.tile_size
            && self.plan.split_px == plan.split_px
            && self.plan.split_layout == plan.split_layout
            && Rc::ptr_eq(&self.plan.paint, &plan.paint)
    }

    fn update_graph(&self, core: &RenderCore, graph: &wgpu::Texture) {
        // Presentation and capture submit their reads on this same queue
        // before the next batch. Only the new left-side raster is copied.
        let mut encoder = core.device().create_command_encoder(&Default::default());
        encoder.copy_texture_to_texture(
            graph.as_image_copy(),
            self.pixels.as_image_copy(),
            extent(self.plan.split_px, self.plan.window.dimensions.1),
        );
        core.queue().submit([encoder.finish()]);
    }
}

/// Prune only draws with known bounds and complete offscreen clip groups.
/// Every other op stays in painter order, including strokes, text, and
/// transformed or compositing layers that could affect the retained pixels.
fn prune_outside(scene: &mut Scene, height: f32, margin: f32) {
    let width = scene.viewport_width as f32;
    let outside = |[x0, y0, x1, y1]: [f32; 4]| {
        [x0, y0, x1, y1].iter().all(|v| v.is_finite())
            && x0 <= x1
            && y0 <= y1
            && (x0 > width + margin || y0 > height + margin || x1 < -margin || y1 < -margin)
    };
    let mut omitted_depth = 0;
    scene.ops.retain(|op| {
        if omitted_depth > 0 {
            match op {
                SceneOp::PushLayer(_) => omitted_depth += 1,
                SceneOp::PopLayer => omitted_depth -= 1,
                _ => {},
            }
            return false;
        }
        match op {
            SceneOp::Rect(rect) if rect.transform_id == 0 => {
                !outside([rect.x0, rect.y0, rect.x1, rect.y1])
            },
            SceneOp::Shape(shape) if shape.transform_id == 0 && shape.stroke.is_none() => {
                !shape.path.local_aabb().is_some_and(outside)
            },
            SceneOp::PushLayer(layer)
                if layer.transform_id == 0
                    && layer.blend_mode == SceneBlendMode::Normal
                    && layer.compose == SceneCompose::SrcOver
                    && matches!(layer.clip, SceneClip::Rect { rect, .. } if outside(rect)) =>
            {
                omitted_depth = 1;
                false
            },
            _ => true,
        }
    });
}

fn extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

#[cfg(test)]
#[test]
fn crop_prunes_offscreen_layers_and_keeps_blend_and_edge_work() {
    let mut scene = Scene::new(100, 100);
    let color = [1.0; 4];
    let offscreen = SceneClip::Rect {
        rect: [110.0, 5.0, 120.0, 70.0],
        radii: [0.0; 4],
    };
    scene.push_rect(0.0, 0.0, 30.0, 30.0, color);
    scene.push_layer(netrender::SceneLayer::clip(offscreen.clone()));
    scene.push_layer(netrender::SceneLayer::clip(offscreen.clone()));
    scene.push_rect(0.0, 0.0, 30.0, 30.0, color);
    scene.pop_layer();
    scene.pop_layer();
    scene.push_rect(0.0, 110.0, 10.0, 120.0, color);
    scene.push_rect(101.0, 0.0, 105.0, 10.0, color);
    scene.push_layer(netrender::SceneLayer {
        compose: SceneCompose::DestIn,
        ..netrender::SceneLayer::clip(offscreen)
    });
    scene.push_rect(0.0, 0.0, 30.0, 30.0, color);
    scene.pop_layer();

    prune_outside(&mut scene, 100.0, 2.0);

    assert!(matches!(
        scene.ops.as_slice(),
        [
            SceneOp::Rect(first),
            SceneOp::Rect(edge),
            SceneOp::PushLayer(mask),
            SceneOp::Rect(_),
            SceneOp::PopLayer
        ] if first.x0 == 0.0 && edge.x0 == 101.0 && mask.compose == SceneCompose::DestIn
    ));
}
