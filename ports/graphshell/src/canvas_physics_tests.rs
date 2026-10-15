// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Host acceptance tests of `mere::canvas::physics_controls` on Graphshell's
//! reference fixture, as its tree page opens it. The actions moved to
//! pictograph (Graphshell stack lift plan, P2 and L5); these tests drive
//! `GraphshellApp`, so they stay with the host.

use mere::canvas::physics_controls::*;
use mere::canvas::{Canvas, PhysicsChoice, PhysicsKindSource, PhysicsLaw};
use mere::kernel::graph::NodeKey;

/// Prints the reference fixture's relations as node-index pairs, for a
/// probe elsewhere that needs the same topology. Diagnostic only.
#[test]
#[ignore = "diagnostic: prints the fixture topology"]
fn print_fixture_topology() {
    let persona = crate::mere_host::SelectedPersonaRef {
        persona: crate::mere_host::FIXTURE_PERSONA_ADDRESS.to_string(),
        profile: "profile:graphshell-tree".to_string(),
    };
    let app = crate::app::GraphshellApp::fixture(muniment::MemoryBackend::new(), persona)
        .expect("fixture");
    let graph = app.host.graph();
    let mut keys: Vec<_> = graph.nodes().map(|(key, _)| key).collect();
    keys.sort_by_key(|key| key.index());
    let index = |key| keys.iter().position(|k| *k == key).unwrap();
    let pairs: Vec<String> = graph
        .relations()
        .map(|r| format!("({}, {})", index(r.from), index(r.to)))
        .collect();
    println!(
        "FIXTURE nodes {} relations [{}]",
        keys.len(),
        pairs.join(", ")
    );
}

/// The tree page's canvas at the receipts' 1400 by 900 window.
const TREE_CANVAS: (u32, u32) = (982, 627);

/// The P2 fixture as the tree page opens it (`web_tree.rs`,
/// `web_graphs::prepared_canvas`): the reference host's graph on the boot
/// Spiral, fitted to the canvas.
fn p2_fixture_canvas() -> Canvas {
    use crate::app::GraphshellApp;
    use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, SelectedPersonaRef};
    const SPIRAL: &str = "phyllotaxis.default";
    let persona = SelectedPersonaRef {
        persona: FIXTURE_PERSONA_ADDRESS.to_string(),
        profile: "profile:graphshell-tree".to_string(),
    };
    let app = GraphshellApp::fixture(muniment::MemoryBackend::new(), persona).unwrap();
    let (width, height) = TREE_CANVAS;
    let mut canvas = Canvas::with_graph(app.host.graph().clone());
    canvas.resize(width, height);
    canvas.set_layout_strategy(Some(SPIRAL.to_string()));
    let (registry, graph) = canvas.registry_and_graph();
    let positions = mere::canvas::project_canvas_strategy(
        registry, SPIRAL, graph, None, width, height, None, None, true,
    );
    canvas.apply_strategy_positions(&positions);
    canvas.fit_to_content();
    canvas
}

/// F10 (dynamics grammar plan, G1): Kinds reached as its receipt reaches
/// it (Play, Free, Kinds by site), whose Play is the play control's
/// continuous run, then 1 800 frames at 60 Hz, one step each. Its kinetic
/// energy stays above the P2 floor of 1 at 6 s and at 30 s, so Kinds
/// never rests on the P2 fixture.
#[test]
fn kinds_never_rests_on_the_p2_fixture() {
    use std::time::Duration;
    let (width, height) = TREE_CANVAS;
    let mut canvas = p2_fixture_canvas();
    canvas.set_physics_paused(false);
    apply_arrangement(&mut canvas, FREE_ARRANGEMENT, TREE_CANVAS).unwrap();
    apply_physics(
        &mut canvas,
        &PhysicsChoice {
            law: PhysicsLaw::Kinds,
            kind: PhysicsKindSource::Site,
            ..PhysicsChoice::default()
        },
    );
    let mut steps = 0;
    let mut readings = Vec::new();
    for frame in 0..=1800u64 {
        canvas.frame_at(
            width,
            height,
            Duration::from_micros(frame * 1_000_000 / 60),
            Default::default(),
        );
        steps += canvas
            .elapsed_step_report()
            .map_or(0, |report| report.steps);
        if [60, 360, 1800].contains(&frame) {
            readings.push((frame, steps, canvas.physics_energy()));
        }
    }
    println!("kinds on the P2 fixture (frame, steps, energy): {readings:?}");
    for &(frame, steps, energy) in &readings {
        assert_eq!(
            u64::from(steps),
            frame - 1,
            "a step every frame after the first"
        );
        assert!(energy >= 1.0, "kinds at frame {frame}: energy {energy}");
    }
}

/// Diagnostic (energy-frame lane): every law reached as its tree receipt
/// reaches it, then 3 600 frames at 60 Hz; the layout's world extent, the
/// view's, the nodes off screen and each component's centroid over time.
#[test]
#[ignore = "diagnostic: prints the framing readings"]
fn diag_law_framing_on_the_p2_fixture() {
    use std::collections::HashMap;
    use std::time::Duration;
    let (width, height) = TREE_CANVAS;
    let probe = p2_fixture_canvas();
    // Components by union-find over the relation edges.
    let keys: Vec<_> = probe.graph().nodes().map(|(key, _)| key).collect();
    let mut parent: HashMap<_, _> = keys.iter().map(|&k| (k, k)).collect();
    fn root<K: Copy + Eq + std::hash::Hash>(parent: &mut HashMap<K, K>, k: K) -> K {
        let mut r = k;
        while parent[&r] != r {
            r = parent[&r];
        }
        parent.insert(k, r);
        r
    }
    for relation in probe.graph().relations() {
        let (a, b) = (
            root(&mut parent, relation.from),
            root(&mut parent, relation.to),
        );
        parent.insert(a, b);
    }
    let mut component: HashMap<_, usize> = HashMap::new();
    let mut roots = Vec::new();
    for &k in &keys {
        let r = root(&mut parent, k);
        let index = roots.iter().position(|&x| x == r).unwrap_or_else(|| {
            roots.push(r);
            roots.len() - 1
        });
        component.insert(probe.graph().get_node(k).unwrap().id, index);
    }
    let sizes: Vec<usize> = (0..roots.len())
        .map(|c| component.values().filter(|&&x| x == c).count())
        .collect();
    println!(
        "fixture: {} nodes, {} relations, components {:?}",
        keys.len(),
        probe.graph().relations().count(),
        sizes
    );
    for (id, _) in mere::canvas::CANVAS_PHYSICS_LAWS {
        let law = PhysicsLaw::parse(id).unwrap();
        let mut canvas = p2_fixture_canvas();
        let fitted = canvas.layout_framing(0.0);
        canvas.set_physics_paused(false);
        apply_arrangement(&mut canvas, FREE_ARRANGEMENT, TREE_CANVAS).unwrap();
        apply_physics(
            &mut canvas,
            &PhysicsChoice {
                law,
                ..PhysicsChoice::default()
            },
        );
        println!(
            "{id}: boot view [{:.0} {:.0} {:.0} {:.0}] zoom {:.3}, boot extent [{:.0} {:.0} {:.0} {:.0}]",
            fitted.view[0],
            fitted.view[1],
            fitted.view[2],
            fitted.view[3],
            canvas.camera().zoom,
            fitted.extent[0],
            fitted.extent[1],
            fitted.extent[2],
            fitted.extent[3],
        );
        for frame in 0..=3600u64 {
            canvas.frame_at(
                width,
                height,
                Duration::from_micros(frame * 1_000_000 / 60),
                Default::default(),
            );
            if [1, 30, 60, 120, 360, 600, 1200, 1800, 3600].contains(&frame) {
                let f = canvas.layout_framing(0.0);
                let geometry = canvas.cartography_geometry();
                let mut sums = vec![(0.0f32, 0.0f32, 0usize); roots.len()];
                for (node, (x, y)) in geometry.iter() {
                    let c = component[&node];
                    sums[c] = (sums[c].0 + x, sums[c].1 + y, sums[c].2 + 1);
                }
                let centroids: Vec<String> = sums
                    .iter()
                    .map(|(x, y, n)| format!("({:.0},{:.0})", x / *n as f32, y / *n as f32))
                    .collect();
                println!(
                    "  frame {frame:>4} ({:>5.1} s): extent [{:.0} {:.0} {:.0} {:.0}] {:.0}x{:.0}, outside {}/{}, energy {:.1}, components {}",
                    frame as f32 / 60.0,
                    f.extent[0],
                    f.extent[1],
                    f.extent[2],
                    f.extent[3],
                    f.extent[2] - f.extent[0],
                    f.extent[3] - f.extent[1],
                    f.outside,
                    f.nodes,
                    canvas.physics_energy(),
                    centroids.join(" "),
                );
            }
        }
    }
}

/// Orbit reached as its receipts reach it (Play, Free, Orbit), then
/// `seconds` at 60 Hz under the host's `damping` (`None`, the canvas's
/// default; ruled 2026-10-04, "Frictionless orbits + centring" and
/// "Radial floor at 0.82"):
/// the extent stays within 3x its first second's, the energy stays above
/// the P2 floor of 1 every second, every node is on the tree page's canvas
/// through the canvas's own camera at the receipts' two reads (1 s and
/// 6 s), and the graph orbits: each node's angle about the centroid turns
/// at least a revolution per 120 s on average.
fn orbit_on_the_p2_fixture(seconds: u64, damping: Option<f32>) {
    use std::time::Duration;
    let (width, height) = TREE_CANVAS;
    let mut canvas = p2_fixture_canvas();
    if let Some(damping) = damping {
        canvas.set_physics_damping(damping);
    }
    canvas.set_physics_paused(false);
    apply_arrangement(&mut canvas, FREE_ARRANGEMENT, TREE_CANVAS).unwrap();
    apply_physics(
        &mut canvas,
        &PhysicsChoice {
            law: PhysicsLaw::Orbit,
            ..PhysicsChoice::default()
        },
    );
    let keys: Vec<NodeKey> = canvas.graph().nodes().map(|(key, _)| key).collect();
    let at = |canvas: &Canvas| -> Vec<(f32, f32)> {
        keys.iter()
            .filter_map(|&key| canvas.node_position(key).map(|p| (p.x, p.y)))
            .collect()
    };
    let centroid = |points: &[(f32, f32)]| {
        let n = points.len() as f32;
        let (x, y) = points
            .iter()
            .fold((0.0, 0.0), |(x, y), p| (x + p.0, y + p.1));
        (x / n, y / n)
    };
    let angles = |points: &[(f32, f32)]| {
        let c = centroid(points);
        points
            .iter()
            .map(|p| (p.1 - c.1).atan2(p.0 - c.0))
            .collect::<Vec<f32>>()
    };
    let mut angle = angles(&at(&canvas));
    let mut turned = vec![0.0f32; keys.len()];
    let (mut first, mut largest) = (0.0f32, 0.0f32);
    let mut readings = Vec::new();
    for frame in 1..=seconds * 60 {
        canvas.frame_at(
            width,
            height,
            Duration::from_micros(frame * 1_000_000 / 60),
            Default::default(),
        );
        let points = at(&canvas);
        assert_eq!(points.len(), keys.len(), "every node placed");
        for (i, a) in angles(&points).into_iter().enumerate() {
            let mut d = a - angle[i];
            while d > std::f32::consts::PI {
                d -= std::f32::consts::TAU;
            }
            while d < -std::f32::consts::PI {
                d += std::f32::consts::TAU;
            }
            turned[i] += d;
            angle[i] = a;
        }
        if frame % 60 != 0 {
            continue;
        }
        let (mut lo, mut hi) = (points[0], points[0]);
        for p in &points {
            lo = (lo.0.min(p.0), lo.1.min(p.1));
            hi = (hi.0.max(p.0), hi.1.max(p.1));
        }
        let extent = (hi.0 - lo.0).max(hi.1 - lo.1);
        let outside = keys
            .iter()
            .filter_map(|&key| canvas.screen_position_of(key))
            .filter(|&(x, y)| x < 0.0 || y < 0.0 || x > width as f32 || y > height as f32)
            .count();
        if frame == 60 {
            first = extent;
        }
        largest = largest.max(extent);
        let energy = canvas.physics_energy();
        readings.push((frame / 60, extent.round(), energy.round(), outside));
        assert!(energy >= 1.0, "orbit at {} s: energy {energy}", frame / 60);
        if frame == 60 || frame == 360 {
            assert_eq!(
                outside,
                0,
                "orbit at {} s: nodes off the canvas",
                frame / 60
            );
        }
    }
    let revolutions =
        turned.iter().map(|t| t.abs()).sum::<f32>() / keys.len() as f32 / std::f32::consts::TAU;
    println!(
        "orbit on the P2 fixture at damping {damping:?} (s, extent, energy, outside): {readings:?}; revolutions {revolutions:.2}"
    );
    assert!(
        largest <= 3.0 * first,
        "orbit stays bound: {largest:.0} against {first:.0} at 1 s"
    );
    assert!(
        revolutions >= seconds as f32 / 120.0,
        "orbit orbits: {revolutions:.2} revolutions in {seconds} s"
    );
}

/// The quick default: Orbit's bars over 30 s.
#[test]
fn orbit_stays_bound_and_orbiting_on_the_p2_fixture() {
    orbit_on_the_p2_fixture(30, None);
}

/// The full claim, 120 s (about 130 s in a debug build).
#[test]
#[ignore = "receipt: Orbit's bars over 120 s; run by Orbit-touching lanes"]
fn orbit_stays_bound_and_orbiting_on_the_p2_fixture_for_two_minutes() {
    orbit_on_the_p2_fixture(120, None);
}

/// The same at no host damping, where the radial floor does the settling.
#[test]
#[ignore = "receipt: Orbit's bars over 120 s at no damping; run by Orbit-touching lanes"]
fn orbit_stays_bound_and_orbiting_on_the_p2_fixture_at_no_damping() {
    orbit_on_the_p2_fixture(120, Some(0.0));
}
