// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! P5c-T's receipt: Turnstone's pool gives its canvases a device built from a
//! renderer's handles, and a 2,000-node settle at threshold 0 meets the CPU
//! path's bounds with the device counted as running. Skips where there is no
//! wgpu adapter.

use std::time::Duration;

use cartography::canvas::{Canvas, LayoutStats};

use super::*;
use crate::app::GraphRuntimePool;
use crate::panes::GraphId;

#[test]
fn the_threshold_reads_a_count_off_or_the_native_default() {
    assert_eq!(threshold_from(None), Some(NATIVE_THRESHOLD));
    assert_eq!(threshold_from(Some("")), Some(NATIVE_THRESHOLD));
    assert_eq!(threshold_from(Some("0")), Some(0));
    assert_eq!(threshold_from(Some(" 1500 ")), Some(1500));
    assert_eq!(threshold_from(Some("OFF")), None);
    assert_eq!(threshold_from(Some("many")), Some(NATIVE_THRESHOLD));
}

#[test]
fn the_node_tier_is_on_unless_switched_off() {
    assert!(nodes_enabled_from(None));
    assert!(nodes_enabled_from(Some("on")));
    assert!(nodes_enabled_from(Some("")));
    assert!(!nodes_enabled_from(Some("off")));
    assert!(!nodes_enabled_from(Some(" OFF ")));
}

fn handles() -> Option<netrender::WgpuHandles> {
    let handles = netrender::boot().ok()?;
    println!("adapter: {}", handles.adapter.get_info().name);
    Some(handles)
}

/// `n` nodes in a random tree, laid out the way Space seeds a graph.
pub(crate) fn tree_canvas(n: usize) -> Canvas {
    let mut canvas = Canvas::with_graph(crate::scenario::seed::tree_graph(n));
    canvas.resize(1280, 800);
    canvas.reseed();
    canvas
}

fn run(pool: &mut GraphRuntimePool, frames: usize) -> LayoutStats {
    for _ in 0..frames {
        pool.active_canvas_mut().frame(1280, 800);
        std::thread::sleep(Duration::from_millis(16));
    }
    pool.active_canvas().layout_stats()
}

fn pool_over(canvas: Canvas) -> GraphRuntimePool {
    GraphRuntimePool::new(GraphId::from_uuid(uuid::Uuid::from_u128(5)), None, canvas)
}

#[test]
fn a_two_thousand_node_settle_through_the_pool_meets_the_cpu_bounds() {
    let Some(handles) = handles() else {
        eprintln!("no wgpu adapter: skipping the native lagged settle receipt");
        return;
    };
    let (n, frames) = (2_000, 150);

    let mut cpu_pool = pool_over(tree_canvas(n));
    assert!(cpu_pool.physics_device().is_none());
    let cpu = run(&mut cpu_pool, frames);

    let device = device_for(&handles, 0);
    let mut pool = pool_over(tree_canvas(n));
    pool.set_physics_device(Some(device.clone()));
    assert!(pool.active_canvas().physics_device().is_some());
    // A runtime added later is given the same device.
    let later = GraphId::from_uuid(uuid::Uuid::from_u128(6));
    pool.activate_or_insert(later, None, Canvas::new());
    assert!(pool.canvas(later).unwrap().physics_device().is_some());
    pool.activate(GraphId::from_uuid(uuid::Uuid::from_u128(5)));
    let gpu = run(&mut pool, frames);
    let stats = pool.active_canvas().repulsion_stats().expect("a lane");

    println!(
        "n={n}, {frames} frames: cpu {cpu:?}; device {gpu:?}; {stats:?}; \
         device submissions {} answers {}",
        device.submissions(),
        device.answers()
    );
    assert!(
        gpu.overlaps <= cpu.overlaps.max(n / 100),
        "overlaps {} against the CPU's {}",
        gpu.overlaps,
        cpu.overlaps
    );
    assert!(
        (gpu.spread - cpu.spread).abs() / cpu.spread < 0.05,
        "spread {} against the CPU's {}",
        gpu.spread,
        cpu.spread
    );
    assert!(
        gpu.energy <= cpu.energy.max(1.0) * 2.0,
        "energy {} against the CPU's {}",
        gpu.energy,
        cpu.energy
    );
    // The call count: the device answered, and served most of the steps.
    assert!(device.answers() > 0 && device.submissions() >= device.answers());
    assert!(stats.device_steps > stats.cpu_steps, "{stats:?}");
    assert_eq!(stats.failures, 0, "{stats:?}");

    // No device: back on the CPU, the lane gone.
    pool.set_physics_device(None);
    assert!(pool.active_canvas().physics_device().is_none());
    assert_eq!(pool.active_canvas().repulsion_stats(), None);
}

/// The node tier end to end on the renderer's own device: the canvas draws its
/// nodes into a texture, the host stages it under the pane's key, and the
/// scene's external-texture item resolves to it in a raster of the scene.
/// `TURNSTONE_NODE_TIER_PNG=<path>` writes the raster.
#[test]
fn the_node_tier_texture_composes_into_the_scene_raster() {
    use netrender::{ColorLoad, NetrenderOptions, SceneOp, SourceAlpha, external_image_key};
    let Some(handles) = handles() else {
        eprintln!("no wgpu adapter: skipping the node tier composition receipt");
        return;
    };
    let renderer = netrender::create_netrender_instance(
        handles,
        NetrenderOptions { tile_cache_size: Some(256), enable_vello: true, ..Default::default() },
    )
    .expect("vello renderer");
    let (w, h) = (1280u32, 800u32);
    let mut canvas = tree_canvas(400);
    // The renderer's own handles, as the shell hands them over.
    canvas.set_gpu_node_device(Some(cartography::canvas::gpu_node_device_for(
        &renderer.wgpu_device.core,
    )));
    canvas.set_gpu_node_tier(true);
    canvas.set_gpu_node_target(0x51, 1.0);
    for _ in 0..120 {
        canvas.frame(w, h);
    }
    let (scene, _) = canvas.frame(w, h);
    let target = canvas.gpu_node_target().expect("the tier is on");
    let key = external_image_key(target.key);
    assert!(
        scene.ops.iter().any(|op| matches!(op, SceneOp::Image(image) if image.key == key)),
        "the scene places the node texture"
    );
    renderer.stage_external_image(key, &target.view, target.size, SourceAlpha::Premultiplied, target.generation);
    let out = renderer.wgpu_device.core.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("node tier composition"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let clear = wgpu::Color { r: 0.08, g: 0.09, b: 0.11, a: 1.0 };
    renderer.render_vello_scaled(&scene, &out.create_view(&Default::default()), ColorLoad::Clear(clear), 1.0);
    let pixels = renderer.wgpu_device.read_rgba8_texture(&out, w, h);
    let at = |x: f32, y: f32| {
        let i = ((y as u32).min(h - 1) * w + (x as u32).min(w - 1)) as usize * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let background = at(1.0, 1.0);
    // Every GPU-drawn node on screen reads as a node, not the ground.
    let mut checked = 0;
    for (key, _) in canvas.graph().nodes() {
        let Some(body) = canvas.node_billboard(key) else { continue };
        let (x, y) = (body.center.x, body.center.y);
        if x < 8.0 || y < 8.0 || x > w as f32 - 8.0 || y > h as f32 - 8.0 {
            continue;
        }
        if canvas.node_body_rect(key).is_none() {
            continue;
        }
        assert_ne!(at(x, y), background, "node at ({x}, {y}) is on the ground");
        checked += 1;
    }
    assert!(checked > 20, "{checked} nodes checked");
    println!("composed {checked} nodes; {:?}", canvas.gpu_node_stats());
    if let Some(path) = std::env::var_os("TURNSTONE_NODE_TIER_PNG") {
        image::save_buffer(path, &pixels, w, h, image::ColorType::Rgba8).expect("write png");
    }
}

/// Per-stage frame cost of the canvas on its own, inline physics, paced at
/// 60 Hz: `PROFILE_N=2000,10000 cargo test ... profile_canvas_frames --
/// --ignored --nocapture`. `PROFILE_DEVICE=0` runs the CPU/rapier path; `PROFILE_FRAMES` counts per scene and
/// `PROFILE_SCENE=settle|drag+pan` runs one.
#[test]
#[ignore = "a measurement, not a receipt; prints its table"]
fn profile_canvas_frames() {
    use cartography::canvas::{CanvasFrameProfile, PointerButton};
    let sizes: Vec<usize> = std::env::var("PROFILE_N")
        .unwrap_or_else(|_| "2000".into())
        .split(',')
        .filter_map(|n| n.parse().ok())
        .collect();
    let frames: usize = std::env::var("PROFILE_FRAMES").ok().and_then(|n| n.parse().ok()).unwrap_or(240);
    let gpu = std::env::var("PROFILE_DEVICE").map_or(true, |v| v != "0");
    let handles = if gpu { handles() } else { None };
    let t0 = std::time::Instant::now();
    let clock = move || t0.elapsed().as_secs_f64() * 1000.0;
    let pct = |sorted: &[f64], p: f64| sorted[((sorted.len() - 1) as f64 * p).round() as usize];
    for n in sizes {
        let build = std::time::Instant::now();
        let mut pool = pool_over(tree_canvas(n));
        let build_s = build.elapsed().as_secs_f32();
        let device = handles.as_ref().map(|h| device_for(h, 0));
        if let Some(device) = &device {
            pool.set_physics_device(Some(device.clone()));
        }
        let tier = handles.is_some() && std::env::var("PROFILE_NODES").map_or(true, |v| v != "off");
        if let (true, Some(handles)) = (tier, handles.as_ref()) {
            pool.set_node_device(Some(cartography::canvas::gpu_node_device_for(handles)));
            pool.active_canvas_mut().set_gpu_node_target(0x51, 1.0);
        }
        // The first frame builds the DOM for every node; it is not a frame.
        let warm = std::time::Instant::now();
        pool.active_canvas_mut().frame_profiled(1280, 800, clock);
        println!("n={n} first frame {:.0} ms", warm.elapsed().as_secs_f64() * 1e3);
        let only = std::env::var("PROFILE_SCENE").ok();
        for scene in ["settle", "drag+pan"] {
            if only.as_deref().is_some_and(|only| only != scene) {
                continue;
            }
            let mut rows: Vec<CanvasFrameProfile> = Vec::new();
            let mut totals: Vec<f64> = Vec::new();
            let mut tiers: Vec<cartography::canvas::NodeTierStats> = Vec::new();
            let before = device.as_ref().map(|d| (d.submissions(), d.answers()));
            let (mut x, mut y) = (640.0f32, 400.0f32);
            for frame in 0..frames {
                if scene == "drag+pan" {
                    let canvas = pool.active_canvas_mut();
                    if frame == 0 {
                        canvas.fit_visible();
                        let key = canvas.graph().nodes().next().map(|(key, _)| key).unwrap();
                        (x, y) = canvas.screen_position_of(key).unwrap_or((640.0, 400.0));
                        canvas.pointer_down(PointerButton::Left, x, y);
                    }
                    let t = frame as f32 * 0.1;
                    canvas.cursor_moved(x + 60.0 * t.sin(), y + 40.0 * t.cos());
                    if frame % 3 == 0 {
                        canvas.wheel(2.0, 0.0);
                    }
                }
                let start = std::time::Instant::now();
                let (_, _, profile) = pool.active_canvas_mut().frame_profiled(1280, 800, clock);
                totals.push(start.elapsed().as_secs_f64() * 1e3);
                tiers.push(pool.active_canvas().gpu_node_stats());
                rows.push(profile);
                std::thread::sleep(Duration::from_millis(16));
            }
            if scene == "drag+pan" {
                pool.active_canvas_mut().pointer_up(PointerButton::Left, x, y);
            }
            // Drop the first quarter: the settle's start.
            let skip = frames / 4;
            let mut line = format!(
                "n={n} {}{} {scene}: frames={} build={build_s:.1}s visible={} cmds {}->{}",
                if device.is_some() { "gpu" } else { "cpu" },
                if tier { "+nodes" } else { "" },
                frames - skip,
                rows[frames - 1].visible_nodes,
                rows[frames - 1].paint_commands_before_cull,
                rows[frames - 1].paint_commands_after_cull,
            );
            for (i, stage) in CanvasFrameProfile::STAGES.iter().enumerate() {
                let mut v: Vec<f64> = rows[skip..].iter().map(|r| r.stage_ms[i]).collect();
                v.sort_by(f64::total_cmp);
                line += &format!(" | {stage} {:.2}/{:.2}", pct(&v, 0.5), pct(&v, 0.95));
            }
            let mut v = totals[skip..].to_vec();
            v.sort_by(f64::total_cmp);
            line += &format!(" | total {:.2}/{:.2}", pct(&v, 0.5), pct(&v, 0.95));
            if tier {
                let mut cpu: Vec<f64> = tiers[skip..].iter().map(|t| t.cpu_ms).collect();
                cpu.sort_by(f64::total_cmp);
                let mut up: Vec<usize> = tiers[skip..].iter().map(|t| t.uploaded_bytes).collect();
                up.sort_unstable();
                let last = tiers[frames - 1];
                line += &format!(
                    " | tier cpu {:.2}/{:.2} ms, upload {}/{} B (median/p95), dom {} nodes {} links {}, passes {}/{}",
                    pct(&cpu, 0.5),
                    pct(&cpu, 0.95),
                    up[(up.len() - 1) / 2],
                    up[((up.len() - 1) as f64 * 0.95).round() as usize],
                    last.dom_nodes,
                    last.nodes,
                    last.links,
                    tiers[skip..].iter().filter(|t| t.rendered).count(),
                    tiers.len() - skip,
                );
            }
            if let (Some(device), Some((s, a))) = (&device, before) {
                line += &format!(" | submits {} answers {}", device.submissions() - s, device.answers() - a);
            }
            println!("{line}");
        }
    }
}
