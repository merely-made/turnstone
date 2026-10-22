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

#[test]
fn authored_mix_survives_the_canonical_v4_scene_facet_and_reopen() {
    use crate::local_scene::{capture_scene, restore_scene};
    use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, SelectedPersonaRef};
    use crate::product::SAVED_SCENE_FACET;

    let persona = SelectedPersonaRef {
        persona: FIXTURE_PERSONA_ADDRESS.into(),
        profile: "profile:graphshell-tree".into(),
    };
    let mut app =
        crate::app::GraphshellApp::fixture(muniment::MemoryBackend::new(), persona).unwrap();
    let mut subject = p2_fixture_canvas();
    apply_mix(
        &mut subject,
        &TwoLawMix {
            first: PhysicsLaw::Springs,
            second: PhysicsLaw::Charge,
            first_share: 0.25,
            second_share: 0.75,
        },
    )
    .unwrap();
    let saved = capture_scene(&subject, "phyllotaxis.default", "page").unwrap();
    const ADDRESS: &str = "mere://scene/g4b2-mix";
    app.host.save_product_scene(ADDRESS, &saved).unwrap();
    let facet = app.host.facet_value(ADDRESS, SAVED_SCENE_FACET).unwrap();
    assert_eq!(&facet["dynamics"], saved.dynamics.value());
    assert_eq!(facet["dynamics"]["root"]["node"], "mix");
    assert_eq!(facet["dynamics"]["root"]["parts"][1]["weight"], 0.75);
    let choice = PhysicsChoice::live(&subject);
    apply_single_law(&mut subject, &choice).unwrap();
    assert!(matches!(
        subject.dynamics_spec().unwrap().root,
        mere::canvas::dynamics_spec::Node::Preset { .. }
    ));
    let reopened = app.host.product_scene(ADDRESS).unwrap();
    restore_scene(&mut subject, app.host.graph(), &reopened, TREE_CANVAS).unwrap();
    assert_eq!(
        subject.dynamics_spec().unwrap(),
        saved.dynamics_spec().unwrap()
    );
    assert_eq!(subject.graph().node_count(), app.host.graph().node_count());
}

#[test]
fn authored_grouping_preserves_the_v4_record_and_reopens_exactly() {
    use crate::local_scene::{capture_scene, restore_scene};
    use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, SelectedPersonaRef};
    use crate::product::SAVED_SCENE_FACET;
    use mere::canvas::Role;
    use mere::canvas::dynamics_spec::{Bar, GroupRoles, Observable, Realization, Target};
    use mere::canvas::{Channel, PhysicsOverlay};
    use std::collections::BTreeMap;

    let persona = SelectedPersonaRef {
        persona: FIXTURE_PERSONA_ADDRESS.into(),
        profile: "profile:graphshell-tree".into(),
    };
    let mut app =
        crate::app::GraphshellApp::fixture(muniment::MemoryBackend::new(), persona).unwrap();
    let mut subject = p2_fixture_canvas();

    // Make every surrounding v4 field observable, including both role-map
    // scopes, so the grouped action's root replacement cannot hide a loss.
    let keys: Vec<_> = subject.graph().nodes().map(|(key, _)| key).collect();
    let mut same_site = None;
    'outer: for (i, &a) in keys.iter().enumerate() {
        let Some(site) = subject.role_group_of(a) else {
            continue;
        };
        for &b in keys.iter().skip(i + 1) {
            if subject.role_group_of(b).as_deref() == Some(site.as_str()) {
                same_site = Some((site, a, b));
                break 'outer;
            }
        }
    }
    let (site, group_member, item_member) = same_site.expect("fixture has a multi-node site");
    let item_id = subject.graph().get_node(item_member).unwrap().id;
    let mut original = subject.dynamics_spec().unwrap();
    original.seed = 104_375_397_257_316;
    original.bars = vec![Bar {
        observable: Observable::Overlaps,
        at_least: None,
        at_most: Some(4.0),
    }];
    original.realization = Realization::Integrate { damping: Some(0.7) };
    original.channels.insert("kind".into(), "kind.site".into());
    original
        .root
        .overlays_mut()
        .push(mere::canvas::dynamics_spec::Node::preset(
            PhysicsOverlay::GravityLocus.id(),
        ));
    original.target = Some(Target {
        arrangement: "grid.default".into(),
        anchored_pull: 0.3,
        default_role: Role::Seeded,
        groups: Some(GroupRoles {
            channel: Channel::Groups(PhysicsKindSource::Site).id(),
            roles: BTreeMap::from([(site, Role::Anchored)]),
        }),
        items: BTreeMap::from([(item_id.to_string(), Role::Pinned)]),
    });
    subject.set_dynamics_spec(&original).unwrap();

    apply_grouping(
        &mut subject,
        &TwoLawGrouping {
            partition: PhysicsKindSource::Cluster,
            outer: PhysicsLaw::Charge,
            inner: PhysicsLaw::Springs,
            outer_share: 0.1,
        },
    )
    .unwrap();
    let grouped = subject.dynamics_spec().unwrap();
    let authored_root = grouped.root.clone();
    let mere::canvas::dynamics_spec::Node::Grouped {
        partition,
        outer,
        inner,
        weight,
        overlays,
    } = &grouped.root
    else {
        panic!("typed action writes a grouped root")
    };
    assert_eq!(partition, &Channel::Groups(PhysicsKindSource::Cluster).id());
    assert_eq!(*weight, 1.0);
    assert_eq!(
        outer.weight(),
        0.1,
        "authored outer share remains exact f64"
    );
    assert_eq!(inner.weight(), 1.0);
    assert_eq!(overlays, original.root.overlays());
    assert_eq!(grouped.seed, original.seed);
    assert_eq!(grouped.bars, original.bars);
    assert_eq!(grouped.realization, original.realization);
    assert_eq!(grouped.channels, original.channels);
    assert_eq!(grouped.target, original.target);

    // The partition, the Group pull source, and role-target source are three
    // independent choices. Editing Group pull retains the authored root and
    // both role maps while writing its own channel.
    let mut choice = PhysicsChoice::live(&subject);
    choice.groups = PhysicsKindSource::Component;
    apply_physics(&mut subject, &choice);
    let sourced = subject.dynamics_spec().unwrap();
    assert_eq!(sourced.root, authored_root);
    assert_eq!(sourced.channels["groups"], "groups.component");
    assert_eq!(
        sourced
            .target
            .as_ref()
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .channel,
        Channel::Groups(PhysicsKindSource::Site).id()
    );
    assert_eq!(
        sourced
            .target
            .as_ref()
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .roles,
        grouped
            .target
            .as_ref()
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .roles
    );
    assert_eq!(
        sourced.target.as_ref().unwrap().items,
        grouped.target.as_ref().unwrap().items
    );
    assert_eq!(subject.arrangement_role_of(group_member), Role::Anchored);
    assert_eq!(subject.arrangement_role_of(item_member), Role::Pinned);

    let saved = capture_scene(&subject, "phyllotaxis.default", "page").unwrap();
    let saved_spec = saved.dynamics_spec().unwrap();
    const ADDRESS: &str = "mere://scene/g4b2-grouping";
    app.host.save_product_scene(ADDRESS, &saved).unwrap();
    // Clone the canonical facet before the later write replaces its stored
    // value, so the byte-for-byte dynamics comparison has a stable witness.
    let facet = app
        .host
        .facet_value(ADDRESS, SAVED_SCENE_FACET)
        .unwrap()
        .clone();
    assert_eq!(
        facet["dynamics"],
        serde_json::to_value(&saved_spec).unwrap()
    );
    assert_eq!(facet["dynamics"]["root"]["node"], "grouped");
    assert_eq!(facet["dynamics"]["root"]["partition"], "groups.cluster");
    assert_eq!(facet["dynamics"]["root"]["outer"]["weight"], 0.1);
    assert_eq!(facet["dynamics"]["channels"]["groups"], "groups.component");
    assert_eq!(
        facet["dynamics"]["target"]["groups"]["channel"],
        "groups.site"
    );

    let reopened = app.host.product_scene(ADDRESS).unwrap();
    let choice = PhysicsChoice::live(&subject);
    apply_single_law(&mut subject, &choice).unwrap();
    assert!(matches!(
        subject.dynamics_spec().unwrap().root,
        mere::canvas::dynamics_spec::Node::Preset { .. }
    ));
    restore_scene(&mut subject, app.host.graph(), &reopened, TREE_CANVAS).unwrap();
    assert_eq!(subject.dynamics_spec().unwrap(), saved_spec);
    assert_eq!(subject.dynamics_spec().unwrap().root, authored_root);
    assert_eq!(subject.arrangement_role_of(group_member), Role::Anchored);
    assert_eq!(subject.arrangement_role_of(item_member), Role::Pinned);

    let resaved = capture_scene(&subject, "phyllotaxis.default", "page").unwrap();
    app.host.save_product_scene(ADDRESS, &resaved).unwrap();
    assert_eq!(
        app.host.facet_value(ADDRESS, SAVED_SCENE_FACET).unwrap()["dynamics"],
        facet["dynamics"]
    );
}

#[derive(Clone, Copy, Debug)]
enum NarrowedNumber {
    MixPart,
    GroupedOuter,
    TargetPull,
    Damping,
    ScheduledMixPart,
    ScheduledGroupedOuter,
}

impl NarrowedNumber {
    fn path(self) -> &'static str {
        match self {
            Self::MixPart => "root.parts[1].weight",
            Self::GroupedOuter => "root.outer.weight",
            Self::TargetPull => "target.anchored_pull",
            Self::Damping => "realization.damping",
            Self::ScheduledMixPart => "root.stages[0].node.parts[1].weight",
            Self::ScheduledGroupedOuter => "root.stages[0].node.outer.weight",
        }
    }
}

fn numeric_spec(
    mut spec: mere::canvas::dynamics_spec::DynamicsSpec,
    field: NarrowedNumber,
    value: f64,
) -> mere::canvas::dynamics_spec::DynamicsSpec {
    use mere::canvas::dynamics_spec::{Node, Realization, Stage, Stop};

    fn weighted_law(value: f64) -> Node {
        Node::Preset {
            id: PhysicsLaw::Springs.id().to_string(),
            weight: value,
            terms: Default::default(),
            rung: None,
            overlays: Vec::new(),
        }
    }
    fn weighted_root(field: NarrowedNumber, value: f64) -> Node {
        match field {
            NarrowedNumber::MixPart | NarrowedNumber::ScheduledMixPart => Node::Mix {
                parts: vec![Node::preset(PhysicsLaw::Springs.id()), weighted_law(value)],
                weight: 1.0,
                overlays: Vec::new(),
            },
            NarrowedNumber::GroupedOuter | NarrowedNumber::ScheduledGroupedOuter => Node::Grouped {
                partition: "groups.site".into(),
                outer: Box::new(weighted_law(value)),
                inner: Box::new(Node::preset(PhysicsLaw::Springs.id())),
                weight: 1.0,
                overlays: Vec::new(),
            },
            _ => unreachable!("this field does not live in the root"),
        }
    }

    match field {
        NarrowedNumber::MixPart | NarrowedNumber::GroupedOuter => {
            spec.root = weighted_root(field, value);
        },
        NarrowedNumber::TargetPull => {
            spec.target.as_mut().expect("fixture target").anchored_pull = value;
        },
        NarrowedNumber::Damping => {
            spec.realization = Realization::Integrate {
                damping: Some(value),
            };
        },
        NarrowedNumber::ScheduledMixPart | NarrowedNumber::ScheduledGroupedOuter => {
            spec.root = Node::Schedule {
                stages: vec![Stage {
                    node: weighted_root(field, value),
                    stop: Stop::Rest,
                    capture: None,
                }],
                weight: 1.0,
                overlays: Vec::new(),
            };
        },
    }
    spec
}

fn codicil_with_scene<B: muniment::Backend + Clone>(
    host: &crate::mere_host::MereHost<B>,
    member: uuid::Uuid,
    scene: &crate::product::SavedSceneV3,
) -> Vec<u8> {
    use crate::product::{ExportRequest, TransferScope};

    host.export_product_codicil(ExportRequest {
        focused: member,
        selected: vec![member],
        scope: TransferScope::ObjectOnly,
        exported_at_ms: 22,
        include_local_file_locations: false,
        scene: Some(scene.clone()),
    })
    .unwrap()
}

#[test]
fn v4_codicil_refuses_f64_values_that_do_not_survive_canvas_binding_atomically() {
    use crate::local_scene::{capture_scene, open_codicil};
    use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, FIXTURE_WEB_ADDRESS, SelectedPersonaRef};
    use crate::product::{SAVED_SCENE_FACET, SavedDynamics};

    let fields = [
        NarrowedNumber::MixPart,
        NarrowedNumber::GroupedOuter,
        NarrowedNumber::TargetPull,
        NarrowedNumber::Damping,
        NarrowedNumber::ScheduledMixPart,
        NarrowedNumber::ScheduledGroupedOuter,
    ];
    let persona = SelectedPersonaRef {
        persona: FIXTURE_PERSONA_ADDRESS.into(),
        profile: "profile:graphshell-tree".into(),
    };
    let mut app =
        crate::app::GraphshellApp::fixture(muniment::MemoryBackend::new(), persona).unwrap();
    let mut canvas = p2_fixture_canvas();
    let member = app
        .host
        .graph()
        .get_node_by_url(FIXTURE_WEB_ADDRESS)
        .unwrap()
        .1
        .id;
    const ADDRESS: &str = "mere://scene/numeric-guard";
    for field in fields {
        for value in [1.0e300, 1.0e-300] {
            let mut scene = capture_scene(&canvas, "phyllotaxis.default", "page").unwrap();
            app.host.save_product_scene(ADDRESS, &scene).unwrap();
            let saved_facet = app
                .host
                .facet_value(ADDRESS, SAVED_SCENE_FACET)
                .unwrap()
                .clone();
            let spec = numeric_spec(scene.dynamics_spec().unwrap(), field, value);
            scene.dynamics = SavedDynamics::from_spec(&spec);
            let session = app.host.graph_session().id().0;
            let graph = serde_json::to_value(app.host.graph().to_snapshot_at(0)).unwrap();
            let record = canvas.dynamics_spec().unwrap();
            let geometry = canvas.cartography_geometry();
            let canvas_graph = serde_json::to_value(canvas.graph().to_snapshot_at(0)).unwrap();
            let bytes = codicil_with_scene(&app.host, member, &scene);

            let error = match open_codicil(&mut app.host, &mut canvas, &bytes, TREE_CANVAS) {
                Ok(_) => panic!("unrepresentable f64 scene values must be refused before import"),
                Err(error) => error,
            };
            assert!(error.contains(field.path()), "{field:?}: {error}");
            assert_eq!(app.host.graph_session().id().0, session, "{field:?}");
            assert_eq!(
                serde_json::to_value(app.host.graph().to_snapshot_at(0)).unwrap(),
                graph,
                "{field:?} host graph changed"
            );
            assert_eq!(canvas.dynamics_spec().unwrap(), record, "{field:?}");
            assert_eq!(canvas.cartography_geometry(), geometry, "{field:?}");
            assert_eq!(
                serde_json::to_value(canvas.graph().to_snapshot_at(0)).unwrap(),
                canvas_graph,
                "{field:?} canvas graph changed"
            );
            assert_eq!(
                app.host.facet_value(ADDRESS, SAVED_SCENE_FACET).unwrap(),
                &saved_facet,
                "{field:?} saved scene facet changed"
            );
        }
    }
}

#[test]
fn v4_scene_save_reopen_preserves_zero_subnormal_and_exact_decimal_f64_values() {
    use crate::local_scene::{capture_scene, restore_scene};
    use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, SelectedPersonaRef};
    use crate::product::{SAVED_SCENE_FACET, SavedDynamics};

    let persona = SelectedPersonaRef {
        persona: FIXTURE_PERSONA_ADDRESS.into(),
        profile: "profile:graphshell-tree".into(),
    };
    let mut app =
        crate::app::GraphshellApp::fixture(muniment::MemoryBackend::new(), persona).unwrap();
    let mut canvas = p2_fixture_canvas();
    const ADDRESS: &str = "mere://scene/numeric-roundtrip";
    let fields = [
        NarrowedNumber::MixPart,
        NarrowedNumber::GroupedOuter,
        NarrowedNumber::TargetPull,
        NarrowedNumber::Damping,
        NarrowedNumber::ScheduledMixPart,
        NarrowedNumber::ScheduledGroupedOuter,
    ];
    let values = [0.0, f64::from(f32::from_bits(1)), 0.1];
    for field in fields {
        for value in values {
            let baseline = capture_scene(&canvas, "phyllotaxis.default", "page").unwrap();
            let spec = numeric_spec(baseline.dynamics_spec().unwrap(), field, value);
            let expected = spec.clone();
            let mut scene = baseline;
            scene.dynamics = SavedDynamics::from_spec(&spec);
            app.host.save_product_scene(ADDRESS, &scene).unwrap();

            let facet = app
                .host
                .facet_value(ADDRESS, SAVED_SCENE_FACET)
                .unwrap()
                .clone();
            assert_eq!(
                &facet["dynamics"],
                scene.dynamics.value(),
                "{field:?} {value}"
            );
            let reopened = app.host.product_scene(ADDRESS).unwrap();
            assert_eq!(
                reopened.dynamics_spec().unwrap(),
                expected,
                "{field:?} {value}"
            );
            restore_scene(&mut canvas, app.host.graph(), &reopened, TREE_CANVAS).unwrap();
            assert_eq!(
                canvas.dynamics_spec().unwrap(),
                expected,
                "{field:?} {value}"
            );
        }
    }
}

#[test]
fn role_target_edits_preserve_the_v4_mix_and_nonempty_group_roles() {
    use crate::local_scene::{capture_scene, restore_scene};
    use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, SelectedPersonaRef};
    use crate::product::SAVED_SCENE_FACET;
    use mere::canvas::Role;
    use mere::canvas::dynamics_spec::GroupRoles;
    use mere::canvas::physics_controls;
    use std::collections::BTreeMap;

    let persona = SelectedPersonaRef {
        persona: FIXTURE_PERSONA_ADDRESS.into(),
        profile: "profile:graphshell-tree".into(),
    };
    let mut app =
        crate::app::GraphshellApp::fixture(muniment::MemoryBackend::new(), persona).unwrap();
    let mut subject = p2_fixture_canvas();
    apply_mix(
        &mut subject,
        &TwoLawMix {
            first: PhysicsLaw::Springs,
            second: PhysicsLaw::Charge,
            first_share: 0.1,
            second_share: 0.3,
        },
    )
    .unwrap();

    let keys: Vec<_> = subject.graph().nodes().map(|(key, _)| key).collect();
    let mut same_site = None;
    'outer: for (i, &a) in keys.iter().enumerate() {
        let Some(site) = subject.role_group_of(a) else {
            continue;
        };
        for &b in keys.iter().skip(i + 1) {
            if subject.role_group_of(b).as_deref() == Some(site.as_str()) {
                same_site = Some((site, a, b));
                break 'outer;
            }
        }
    }
    let (site, group_member, item_member) = same_site.expect("fixture has a multi-node site");
    let item_id = subject.graph().get_node(item_member).unwrap().id;
    let mut spec = subject.dynamics_spec().unwrap();
    let target = spec.target.as_mut().expect("dynamics spec target");
    target.default_role = Role::Seeded;
    target.groups = Some(GroupRoles {
        channel: "groups.site".into(),
        roles: BTreeMap::from([(site, Role::Anchored)]),
    });
    target.items = BTreeMap::from([(item_id.to_string(), Role::Pinned)]);
    subject.set_dynamics_spec(&spec).unwrap();

    assert_eq!(subject.arrangement_role_of(group_member), Role::Anchored);
    assert_eq!(subject.arrangement_role_of(item_member), Role::Pinned);
    let authored_root = spec.root.clone();
    let authored_channels = spec.channels.clone();
    physics_controls::apply_role_group_source(&mut subject, PhysicsKindSource::Cluster).unwrap();
    assert_eq!(subject.role_group_source(), PhysicsKindSource::Cluster);
    assert_eq!(subject.arrangement_role_of(group_member), Role::Seeded);
    assert_eq!(subject.arrangement_role_of(item_member), Role::Pinned);
    physics_controls::apply_role_group_source(&mut subject, PhysicsKindSource::Site).unwrap();
    assert_eq!(subject.role_group_source(), PhysicsKindSource::Site);
    assert_eq!(subject.arrangement_role_of(group_member), Role::Anchored);
    assert_eq!(subject.arrangement_role_of(item_member), Role::Pinned);
    physics_controls::apply_role(&mut subject, Role::Anchored).unwrap();
    let after_default = subject.dynamics_spec().unwrap();
    assert_eq!(after_default.root, authored_root);
    assert_eq!(after_default.channels, authored_channels);
    assert_eq!(after_default.seed, spec.seed);
    assert_eq!(after_default.realization, spec.realization);
    assert_eq!(after_default.bars, spec.bars);
    assert_eq!(
        after_default.target.as_ref().unwrap().arrangement,
        spec.target.as_ref().unwrap().arrangement
    );
    assert_eq!(
        after_default.target.as_ref().unwrap().anchored_pull,
        spec.target.as_ref().unwrap().anchored_pull
    );
    assert_eq!(
        after_default
            .target
            .as_ref()
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .roles,
        spec.target.as_ref().unwrap().groups.as_ref().unwrap().roles
    );
    assert_eq!(
        after_default.target.as_ref().unwrap().items,
        spec.target.as_ref().unwrap().items
    );
    assert_eq!(
        after_default.target.as_ref().unwrap().default_role,
        Role::Anchored
    );
    assert_eq!(subject.arrangement_role_of(group_member), Role::Anchored);
    assert_eq!(subject.arrangement_role_of(item_member), Role::Pinned);

    physics_controls::apply_role_group_source(&mut subject, PhysicsKindSource::Cluster).unwrap();
    physics_controls::apply_role_group_source(&mut subject, PhysicsKindSource::Site).unwrap();
    let after_role_source = subject.dynamics_spec().unwrap();
    assert_eq!(after_role_source.root, authored_root);
    assert_eq!(after_role_source.channels, authored_channels);
    assert_eq!(after_role_source.seed, spec.seed);
    assert_eq!(after_role_source.realization, spec.realization);
    assert_eq!(after_role_source.bars, spec.bars);
    assert_eq!(
        after_role_source
            .target
            .as_ref()
            .unwrap()
            .groups
            .as_ref()
            .unwrap()
            .roles,
        spec.target.as_ref().unwrap().groups.as_ref().unwrap().roles
    );
    assert_eq!(
        after_role_source.target.as_ref().unwrap().items,
        spec.target.as_ref().unwrap().items
    );

    let final_spec = subject.dynamics_spec().unwrap();
    assert_eq!(final_spec.root, authored_root);
    assert_eq!(final_spec.channels, authored_channels);
    let root_value = serde_json::to_value(&final_spec.root).unwrap();
    assert_eq!(root_value["parts"][0]["weight"], 0.1);
    assert_eq!(root_value["parts"][1]["weight"], 0.3);
    let saved = capture_scene(&subject, "phyllotaxis.default", "page").unwrap();
    let saved_spec = saved.dynamics_spec().unwrap();
    assert_eq!(
        saved_spec.target.as_ref().unwrap().arrangement,
        "phyllotaxis.default"
    );
    const ADDRESS: &str = "mere://scene/g4b2-role-target";
    app.host.save_product_scene(ADDRESS, &saved).unwrap();
    let facet = app
        .host
        .facet_value(ADDRESS, SAVED_SCENE_FACET)
        .unwrap()
        .clone();
    assert_eq!(
        facet["dynamics"],
        serde_json::to_value(&saved_spec).unwrap()
    );
    assert_eq!(
        facet["dynamics"]["target"]["groups"]["channel"],
        "groups.site"
    );
    assert_eq!(
        facet["dynamics"]["target"]["groups"]["roles"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        facet["dynamics"]["target"]["items"]
            .as_object()
            .unwrap()
            .len(),
        1
    );

    let reopened = app.host.product_scene(ADDRESS).unwrap();
    apply_role(&mut subject, Role::Seeded).unwrap();
    restore_scene(&mut subject, app.host.graph(), &reopened, TREE_CANVAS).unwrap();
    assert_eq!(subject.dynamics_spec().unwrap(), saved_spec);
    assert_eq!(subject.arrangement_role_of(group_member), Role::Anchored);
    assert_eq!(subject.arrangement_role_of(item_member), Role::Pinned);
    let resaved = capture_scene(&subject, "phyllotaxis.default", "page").unwrap();
    app.host.save_product_scene(ADDRESS, &resaved).unwrap();
    let resaved_facet = app.host.facet_value(ADDRESS, SAVED_SCENE_FACET).unwrap();
    assert_eq!(resaved_facet["dynamics"], facet["dynamics"]);

    // A malformed typed spec is rejected atomically; the UI picker itself
    // only constructs catalog variants and cannot emit this wrong family.
    let before_bad = subject.dynamics_spec().unwrap();
    let before_bad_geometry = subject.cartography_geometry();
    let mut unknown = before_bad.clone();
    unknown
        .target
        .as_mut()
        .unwrap()
        .groups
        .as_mut()
        .unwrap()
        .channel = "groups.unknown".into();
    let unknown_error = subject.set_dynamics_spec(&unknown).unwrap_err();
    assert!(matches!(
        unknown_error,
        mere::canvas::dynamics_spec::BindError::Unbound { ref place, ref reason }
            if place == "target.groups.channel" && reason.contains("groups.unknown")
    ));
    assert_eq!(subject.dynamics_spec().unwrap(), before_bad);
    assert_eq!(subject.cartography_geometry(), before_bad_geometry);
    assert_eq!(
        app.host.facet_value(ADDRESS, SAVED_SCENE_FACET).unwrap()["dynamics"],
        facet["dynamics"]
    );

    let mut wrong_family = before_bad.clone();
    wrong_family
        .target
        .as_mut()
        .unwrap()
        .groups
        .as_mut()
        .unwrap()
        .channel = "kind.site".into();
    let wrong_family_error = subject.set_dynamics_spec(&wrong_family).unwrap_err();
    // Seiche rejects a wrong family before Canvas resolves the channel id.
    let refusal = wrong_family_error.to_string();
    assert!(refusal.contains("target.groups"), "{refusal}");
    assert!(refusal.contains("kind.site"), "{refusal}");
    assert_eq!(subject.dynamics_spec().unwrap(), before_bad);
    assert_eq!(subject.cartography_geometry(), before_bad_geometry);
    assert_eq!(
        app.host.facet_value(ADDRESS, SAVED_SCENE_FACET).unwrap()["dynamics"],
        facet["dynamics"]
    );
}

#[test]
fn authored_two_stage_schedule_survives_v4_reopen_capture_progress_and_refusal() {
    use crate::local_scene::{capture_scene, restore_scene};
    use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, SelectedPersonaRef};
    use crate::product::SAVED_SCENE_FACET;
    use mere::canvas::dynamics_spec::{GroupRoles, Node, Target};
    use mere::canvas::physics_controls::{
        StageDraft, TwoStageSchedule, apply_two_stage_schedule, read_two_stage_schedule,
    };
    use mere::canvas::{Channel, PhysicsOverlay, Role, StageStop};
    use std::collections::BTreeMap;

    let persona = SelectedPersonaRef {
        persona: FIXTURE_PERSONA_ADDRESS.into(),
        profile: "profile:graphshell-tree".into(),
    };
    let mut app =
        crate::app::GraphshellApp::fixture(muniment::MemoryBackend::new(), persona).unwrap();
    let mut subject = p2_fixture_canvas();

    // Retain nonempty group and item target maps around the schedule root.
    let keys: Vec<_> = subject.graph().nodes().map(|(key, _)| key).collect();
    let mut same_site = None;
    'outer: for (i, &a) in keys.iter().enumerate() {
        let Some(site) = subject.role_group_of(a) else {
            continue;
        };
        for &b in keys.iter().skip(i + 1) {
            if subject.role_group_of(b).as_deref() == Some(site.as_str()) {
                same_site = Some((site, a, b));
                break 'outer;
            }
        }
    }
    let (site, _group_member, item_member) = same_site.expect("fixture has a multi-node site");
    let capture_member = keys
        .iter()
        .copied()
        .find(|key| subject.role_group_of(*key).as_deref() != Some(site.as_str()))
        .expect("fixture has a node outside the selected site");
    let item_id = subject.graph().get_node(item_member).unwrap().id;
    let mut original = subject.dynamics_spec().unwrap();
    original.target = Some(Target {
        arrangement: "free".into(),
        anchored_pull: 0.4,
        default_role: Role::Seeded,
        groups: Some(GroupRoles {
            channel: Channel::Groups(PhysicsKindSource::Site).id(),
            roles: BTreeMap::from([(site.clone(), Role::Anchored)]),
        }),
        items: BTreeMap::from([(item_id.to_string(), Role::Pinned)]),
    });
    subject.set_dynamics_spec(&original).unwrap();
    assert_eq!(subject.arrangement_role_of(capture_member), Role::Seeded);

    let schedule = TwoStageSchedule {
        first: StageDraft {
            law: PhysicsLaw::Springs,
            stop: StageStop::Frames(1),
            capture: Some(Role::Anchored),
        },
        second: StageDraft {
            law: PhysicsLaw::Charge,
            stop: StageStop::Frames(10_000),
            capture: None,
        },
        overlays: vec![PhysicsOverlay::GravityLocus],
    };
    apply_two_stage_schedule(&mut subject, &schedule).unwrap();
    assert_eq!(subject.physics_schedule_stage(), Some(0));
    assert!(!subject.physics_paused());
    let _ = subject.frame(800, 600);
    assert_eq!(subject.physics_schedule_stage(), Some(1));
    assert_eq!(subject.arrangement_role_of(capture_member), Role::Anchored);
    assert_eq!(
        subject.captured_positions().unwrap().len(),
        subject.graph().node_count()
    );

    // A rejected shared overlay cannot replace the live schedule or its state.
    subject.set_physics_paused(true);
    let before_spec = subject.dynamics_spec().unwrap();
    let before_positions = subject.cartography_geometry();
    let mut refused = schedule.clone();
    refused.second.law = PhysicsLaw::Density;
    refused.overlays = vec![PhysicsOverlay::DomainCluster];
    let error = apply_two_stage_schedule(&mut subject, &refused).unwrap_err();
    assert!(
        error.contains("Density") || error.contains("overlay"),
        "{error}"
    );
    assert_eq!(subject.dynamics_spec().unwrap(), before_spec);
    assert_eq!(subject.cartography_geometry(), before_positions);
    assert_eq!(subject.physics_schedule_stage(), Some(1));
    assert!(subject.physics_paused());

    // Source and role edits keep the paused stage and progress. The group/item
    // role maps survive alongside the default role edit.
    let mut choice = PhysicsChoice::live(&subject);
    choice.kind = PhysicsKindSource::Degree;
    apply_physics(&mut subject, &choice);
    assert_eq!(subject.physics_schedule_stage(), Some(1));
    assert!(subject.physics_paused());
    apply_role(&mut subject, Role::Pinned).unwrap();
    assert_eq!(subject.physics_schedule_stage(), Some(1));
    assert!(subject.physics_paused());

    let saved = capture_scene(&subject, "phyllotaxis.default", "page").unwrap();
    let saved_spec = saved.dynamics_spec().unwrap();
    let Node::Schedule { stages, .. } = &saved_spec.root else {
        panic!("saved v4 scene keeps the authored schedule recipe")
    };
    assert_eq!(stages.len(), 2);
    assert_eq!(stages[0].capture, Some(Role::Anchored));
    assert_eq!(stages[1].capture, None);
    let target = saved_spec.target.as_ref().unwrap();
    assert_eq!(target.default_role, Role::Pinned);
    assert_eq!(target.groups.as_ref().unwrap().roles[&site], Role::Anchored);
    assert_eq!(target.items[&item_id.to_string()], Role::Pinned);
    const ADDRESS: &str = "mere://scene/g4b3-schedule";
    app.host.save_product_scene(ADDRESS, &saved).unwrap();
    let facet = app
        .host
        .facet_value(ADDRESS, SAVED_SCENE_FACET)
        .unwrap()
        .clone();
    assert_eq!(
        facet["dynamics"],
        serde_json::to_value(&saved_spec).unwrap()
    );
    assert_eq!(facet["dynamics"]["root"]["node"], "schedule");
    assert_eq!(
        facet["dynamics"]["root"]["stages"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        read_two_stage_schedule(&subject).unwrap(),
        Some(schedule.clone())
    );

    // Clearing the recipe is explicit; reopening the saved scene restores the
    // canonical two-stage root at stage zero, paused as saved.
    let choice = PhysicsChoice::live(&subject);
    apply_single_law(&mut subject, &choice).unwrap();
    assert_eq!(subject.physics_schedule_stage(), None);
    let reopened = app.host.product_scene(ADDRESS).unwrap();
    restore_scene(&mut subject, app.host.graph(), &reopened, TREE_CANVAS).unwrap();
    assert_eq!(subject.dynamics_spec().unwrap(), saved_spec);
    assert_eq!(subject.physics_schedule_stage(), Some(0));
    assert!(subject.physics_paused());
    assert_eq!(read_two_stage_schedule(&subject).unwrap(), Some(schedule));
    assert_eq!(subject.graph().node_count(), app.host.graph().node_count());

    let resaved = capture_scene(&subject, "phyllotaxis.default", "page").unwrap();
    app.host.save_product_scene(ADDRESS, &resaved).unwrap();
    assert_eq!(
        app.host.facet_value(ADDRESS, SAVED_SCENE_FACET).unwrap()["dynamics"],
        facet["dynamics"]
    );
}
