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

/// Per-stage frame cost of the canvas on its own, inline physics, paced at
/// 60 Hz: `PROFILE_N=2000,10000 cargo test ... profile_canvas_frames --
/// --ignored --nocapture`. `PROFILE_DEVICE=0` runs the CPU/rapier path.
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
        for scene in ["settle", "drag+pan"] {
            let mut rows: Vec<CanvasFrameProfile> = Vec::new();
            let mut totals: Vec<f64> = Vec::new();
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
                rows.push(profile);
                std::thread::sleep(Duration::from_millis(16));
            }
            if scene == "drag+pan" {
                pool.active_canvas_mut().pointer_up(PointerButton::Left, x, y);
            }
            // Drop the first third: the settle's start.
            let skip = frames / 3;
            let mut line = format!(
                "n={n} {} {scene}: frames={} build={build_s:.1}s visible={} cmds {}->{}",
                if device.is_some() { "gpu" } else { "cpu" },
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
            if let (Some(device), Some((s, a))) = (&device, before) {
                line += &format!(" | submits {} answers {}", device.submissions() - s, device.answers() - a);
            }
            println!("{line}");
        }
    }
}
