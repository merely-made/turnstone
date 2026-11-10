// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Personal graph record cases that open a saved scene's dynamics spec. They
//! stayed here when `personal_sync` moved to pandect (engine split D69): the
//! spec is the canvas's, and the canvas depends on pandect.

use mere::canvas::saved_scene;
use p2panda_core::cbor::{decode_cbor_strict, encode_cbor};
use pandect::personal_sync::{PersonalGraphEvent, PersonalGraphRecord};
use pandect::{CartographyGeometry, SavedSceneV2};
use uuid::Uuid;

const A: Uuid = Uuid::from_u128(0xa1);
const B: Uuid = Uuid::from_u128(0xb2);

/// The deepest nesting of arrays, maps and tags in definite-length CBOR,
/// what cbor-core's recursion limit (200) counts.
fn cbor_nesting(bytes: &[u8]) -> usize {
    fn argument(bytes: &[u8], at: &mut usize, info: u8) -> u64 {
        let width = match info {
            0..=23 => return u64::from(info),
            24 => 1,
            25 => 2,
            26 => 4,
            27 => 8,
            _ => panic!("indefinite lengths are not deterministic CBOR"),
        };
        let value = bytes[*at..*at + width]
            .iter()
            .fold(0u64, |v, b| (v << 8) | u64::from(*b));
        *at += width;
        value
    }
    fn item(bytes: &[u8], at: &mut usize, depth: usize, deepest: &mut usize) {
        let head = bytes[*at];
        *at += 1;
        let n = argument(bytes, at, head & 0x1f);
        let children = match head >> 5 {
            2 | 3 => {
                *at += n as usize;
                0
            },
            4 => n,
            5 => 2 * n,
            6 => 1,
            _ => 0,
        };
        if matches!(head >> 5, 4..=6) {
            *deepest = (*deepest).max(depth + 1);
        }
        for _ in 0..children {
            item(bytes, at, depth + 1, deepest);
        }
    }
    let (mut at, mut deepest) = (0, 0);
    item(bytes, &mut at, 0, &mut deepest);
    assert_eq!(at, bytes.len(), "one item, read whole");
    deepest
}

/// F110: a depth-32 dynamics spec round-trips through a personal graph
/// record's CBOR (cbor-core's recursion limit, 200); derive refuses 33.
#[test]
fn a_depth_32_dynamics_spec_round_trips_through_the_record_cbor() {
    use mere::canvas::dynamics_spec::{DynamicsSpec, Node, Stage, Stop};
    let chain = |depth: usize| {
        let mut node = Node::preset("spring.rapier");
        for _ in 1..depth {
            node = Node::Schedule {
                stages: vec![Stage {
                    node,
                    stop: Stop::Rest,
                    capture: None,
                }],
                weight: 1.0,
                overlays: Vec::new(),
            };
        }
        DynamicsSpec::new(node)
    };
    for depth in [32, 33] {
        let spec = chain(depth);
        let record = PersonalGraphRecord {
            events: vec![PersonalGraphEvent::SaveScene {
                node: A,
                scene: SavedSceneV2 {
                    name: "Deep spec".into(),
                    selected: vec![A],
                    layout_strategy: Some("grid.default".into()),
                    physics_paused: true,
                    physics_damping: 0.7,
                    physics_law: "spring.rapier".into(),
                    physics_overlays: Vec::new(),
                    physics_kind_source: "site".into(),
                    physics_group_source: "site".into(),
                    physics_mass_source: "degree".into(),
                    physics_depth_source: "roots".into(),
                    arrangement_pull: 0.4,
                    arrangement_roles: None,
                    dynamics: Some(saved_scene::saved_dynamics(&spec)),
                    camera_offset: (0.0, 0.0),
                    camera_zoom: 1.0,
                    default_handler: "graphshell.inspect".into(),
                    cartography: CartographyGeometry::default(),
                },
            }],
            parents: Vec::new(),
            writer_attestation: None,
        };
        let bytes = encode_cbor(&record).expect("encodes");
        println!(
            "depth {depth}: record CBOR nesting {} (cbor-core limit 200), {} bytes",
            cbor_nesting(&bytes),
            bytes.len()
        );
        let back: PersonalGraphRecord =
            decode_cbor_strict(bytes.as_slice()).expect("decodes within cbor-core's limit");
        let PersonalGraphEvent::SaveScene { scene, .. } = &back.events[0] else {
            panic!("the event comes back as it went");
        };
        assert_eq!(saved_scene::read_dynamics(scene.dynamics.as_ref().unwrap()), Ok(spec));
        assert_eq!(
            encode_cbor(&back).unwrap(),
            bytes,
            "byte for byte, deterministic"
        );
        assert_eq!(
            saved_scene::scene_from_v2(scene.clone())
                .and_then(|scene| saved_scene::scene_spec(&scene).map(|_| ()))
                .is_err_and(|e| e.contains("33 deep")),
            depth == 33
        );
    }
}

/// F114: a record carrying a scene whose spec this reader refuses still
/// decodes; the spec rides opaque, and that scene's open alone fails.
#[test]
fn a_refused_spec_fails_its_scenes_open_not_the_record() {
    use mere::canvas::dynamics_spec::{DynamicsSpec, Node};
    let spec = DynamicsSpec::new(Node::preset("spring.rapier"));
    let mut newer = serde_json::to_value(&spec).unwrap();
    newer["version"] = serde_json::json!(2);
    newer["realization"] = serde_json::json!({ "anneal": { "cooling": 0.99 } });
    let scene = |dynamics: serde_json::Value| SavedSceneV2 {
        name: "Newer spec".into(),
        selected: vec![A],
        layout_strategy: Some("grid.default".into()),
        physics_paused: true,
        physics_damping: 0.7,
        physics_law: "spring.rapier".into(),
        physics_overlays: Vec::new(),
        physics_kind_source: "site".into(),
        physics_group_source: "site".into(),
        physics_mass_source: "degree".into(),
        physics_depth_source: "roots".into(),
        arrangement_pull: 0.4,
        arrangement_roles: None,
        dynamics: Some(serde_json::from_value(dynamics).unwrap()),
        camera_offset: (0.0, 0.0),
        camera_zoom: 1.0,
        default_handler: "graphshell.inspect".into(),
        cartography: CartographyGeometry::default(),
    };
    let record = PersonalGraphRecord {
        events: vec![
            PersonalGraphEvent::SaveScene {
                node: A,
                scene: scene(newer),
            },
            PersonalGraphEvent::SaveScene {
                node: B,
                scene: scene(serde_json::to_value(&spec).unwrap()),
            },
        ],
        parents: Vec::new(),
        writer_attestation: None,
    };
    let bytes = encode_cbor(&record).expect("encodes");
    let back: PersonalGraphRecord =
        decode_cbor_strict(bytes.as_slice()).expect("the record decodes whole");
    let opened: Vec<Result<(), String>> = back
        .events
        .iter()
        .map(|event| match event {
            PersonalGraphEvent::SaveScene { scene, .. } => saved_scene::scene_from_v2(scene.clone())
                .and_then(|scene| saved_scene::scene_spec(&scene).map(|_| ())),
            _ => unreachable!("two scenes"),
        })
        .collect();
    assert_eq!(
        opened[0],
        Err(
            "dynamics spec: at version: dynamics spec version 2 is newer than this \
             reader's 1"
                .into()
        )
    );
    assert_eq!(opened[1], Ok(()), "the other scene in the record opens");
}
