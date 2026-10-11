// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta};
use muniment::MemoryBackend;
fn graph() -> (Graph, Uuid, Uuid) {
    let mut g = Graph::new();
    let a = Uuid::from_u128(1);
    let b = Uuid::from_u128(2);
    add_node(
        &mut g,
        Some(a),
        "https://example.org/same".into(),
        Default::default(),
    );
    add_node(
        &mut g,
        Some(b),
        "https://example.org/same".into(),
        Default::default(),
    );
    (g, a, b)
}
#[test]
fn two_accesses_share_resource_but_keep_separate_tiles_and_regions() {
    let (g, a, b) = graph();
    let ka = g.get_node_by_id(a).unwrap().0;
    let kb = g.get_node_by_id(b).unwrap().0;
    assert_ne!(ka, kb);
    assert!(g.shown_resource_id(ka).is_some());
    assert_eq!(g.shown_resource_id(ka), g.shown_resource_id(kb));
    let mut w = FormeWorkspace::new(Uuid::from_u128(10));
    w.open(a, &g).unwrap();
    w.open(b, &g).unwrap();
    w.open(a, &g).unwrap();
    assert_eq!(w.layout().tile_count(), 2);
    let r = w.region();
    assert_eq!(r.cells.len(), 2);
    assert!(r.cells[0].bounds[2] <= r.cells[1].bounds[0]);
    assert_eq!(g.node_count(), 2);
    assert_eq!(g.fields().count(), 0);
}

#[test]
fn validation_refuses_split_shares_that_collapse_projected_cells() {
    let (g, a, b) = graph();
    let mut workspace = FormeWorkspace::new(Uuid::from_u128(10));
    workspace.open(a, &g).unwrap();
    workspace.open(b, &g).unwrap();
    let Some(TreeGeometry::Split { children, .. }) = &mut workspace.geometry else {
        panic!("expected two split tiles");
    };
    children[0].fraction = f32::MIN_POSITIVE;
    children[1].fraction = f32::MAX / 2.;
    assert!(workspace.validate(workspace.document.graph_id).is_err());
}

#[test]
fn validation_refuses_cells_collapsed_by_world_coordinate_rounding() {
    let (g, a, b) = graph();
    let mut workspace = FormeWorkspace::new(Uuid::from_u128(10));
    workspace.open(a, &g).unwrap();
    workspace.open(b, &g).unwrap();
    let left = 1.0e20_f32;
    let right = f32::from_bits(left.to_bits() + 1);
    workspace.bounds = [left, 0., right, 100.];
    assert!(workspace.validate(workspace.document.graph_id).is_err());
}

#[test]
fn divider_events_require_one_share_per_child_and_leave_invalid_edits_unchanged() {
    let (g, a, b) = graph();
    let mut workspace = FormeWorkspace::new(Uuid::from_u128(10));
    workspace.open(a, &g).unwrap();
    workspace.open(b, &g).unwrap();
    workspace.locked = false;
    let original = serde_json::to_value(&workspace).unwrap();
    for fractions in [vec![], vec![0.2], vec![0.2, 0.3, 0.5]] {
        assert!(
            workspace
                .event(
                    TileEvent::DividerMoved {
                        split: workbench::TilePath(vec![]),
                        fractions,
                    },
                    &g
                )
                .is_err()
        );
        assert_eq!(serde_json::to_value(&workspace).unwrap(), original);
    }
}
#[test]
fn nested_placement_and_active_tabs_roundtrip_without_pixel_geometry() {
    let (mut g, a, b) = graph();
    let c = Uuid::from_u128(3);
    add_node(
        &mut g,
        Some(c),
        "https://example.org/other".into(),
        Default::default(),
    );
    let mut w = FormeWorkspace::new(Uuid::from_u128(10));
    for m in [a, b, c] {
        w.open(m, &g).unwrap();
    }
    w.locked = false;
    w.event(
        TileEvent::Dragged {
            tile: TileId(3),
            to: DropTarget::Edge {
                tile: TileId(2),
                edge: workbench::Edge::Bottom,
            },
        },
        &g,
    )
    .unwrap();
    let before = w.geometry.clone();
    let json = serde_json::to_string(&w).unwrap();
    let copy: FormeWorkspace = serde_json::from_str(&json).unwrap();
    copy.validate(w.document.graph_id).unwrap();
    assert_eq!(copy.geometry, before);
    assert_eq!(copy.region().cells.len(), 3);
    assert!(copy.region().cells[1].bounds[1] < copy.region().cells[2].bounds[1]);
}
#[test]
fn lock_blocks_layout_edits_but_allows_focus_and_close() {
    let (g, a, b) = graph();
    let mut w = FormeWorkspace::new(Uuid::from_u128(10));
    w.open(a, &g).unwrap();
    w.open(b, &g).unwrap();
    let before = w.geometry.clone();
    assert!(
        w.event(
            TileEvent::DividerMoved {
                split: workbench::TilePath(vec![]),
                fractions: vec![0.3, 0.7]
            },
            &g
        )
        .is_err()
    );
    assert_eq!(w.geometry, before);
    w.event(TileEvent::Activated(TileId(2)), &g).unwrap();
    w.event(TileEvent::Closed(TileId(1)), &g).unwrap();
    assert_eq!(w.members(), vec![b]);
}
#[test]
fn persistence_reconciles_removed_access_and_rejects_foreign_session() {
    pollster::block_on(async {
        let (mut g, a, b) = graph();
        let backend = MemoryBackend::new();
        let id = Uuid::from_u128(10);
        let mut w = FormeWorkspace::new(id);
        w.open(a, &g).unwrap();
        w.open(b, &g).unwrap();
        let identity = w.document.id;
        w.bounds = [10., 20., 610., 420.];
        w.locked = false;
        w.save(backend.clone()).await.unwrap();
        let restored = FormeWorkspace::load(backend.clone(), id, &g).await.unwrap();
        assert_eq!(restored.document.id, identity);
        assert_eq!(restored.bounds, w.bounds);
        let key = g.get_node_by_id(b).unwrap().0;
        apply_graph_delta(&mut g, GraphDelta::RemoveNode { key });
        let restored = FormeWorkspace::load(backend, id, &g).await.unwrap();
        assert_eq!(restored.members(), vec![a]);
        assert!(w.validate(Uuid::from_u128(20)).is_err());
    });
}
#[test]
fn malformed_geometry_is_refused_before_any_install_or_write() {
    let (g, a, _) = graph();
    let mut w = FormeWorkspace::new(Uuid::from_u128(10));
    w.open(a, &g).unwrap();
    w.geometry = Some(TreeGeometry::Stack {
        members: vec![a, a],
        active: 0,
    });
    assert!(w.validate(w.document.graph_id).is_err());
    w.geometry = Some(TreeGeometry::Stack {
        members: vec![a],
        active: 4,
    });
    assert!(w.validate(w.document.graph_id).is_err());
    w.geometry = Some(TreeGeometry::Stack {
        members: vec![a],
        active: 0,
    });
    w.bounds[0] = f32::NAN;
    assert!(w.validate(w.document.graph_id).is_err());
}

#[cfg(test)]
mod stable_identity_tests {
    use super::*;
    use mere::kernel::graph::apply::add_node;
    #[test]
    fn closing_and_reopening_an_access_preserves_all_opaque_tile_ids() {
        let mut graph = Graph::new();
        let ids = [Uuid::from_u128(3), Uuid::from_u128(1), Uuid::from_u128(2)];
        for id in ids {
            add_node(
                &mut graph,
                Some(id),
                "https://example.net".into(),
                Default::default(),
            );
        }
        let mut w = FormeWorkspace::new(Uuid::from_u128(10));
        w.open(ids[0], &graph).unwrap();
        w.open(ids[1], &graph).unwrap();
        let first = w.tile_for_member(ids[0]).unwrap();
        let second = w.tile_for_member(ids[1]).unwrap();
        w.event(TileEvent::Closed(second), &graph).unwrap();
        w.open(ids[2], &graph).unwrap();
        assert_eq!(w.tile_for_member(ids[0]), Some(first));
        assert_ne!(w.tile_for_member(ids[2]), Some(second));
        w.open(ids[1], &graph).unwrap();
        assert_eq!(w.tile_for_member(ids[1]), Some(second));
    }
}

#[cfg(test)]
mod tab_placement_tests {
    use super::*;
    use mere::kernel::graph::apply::add_node;
    #[test]
    fn tab_drop_order_and_active_member_survive_persistence() {
        pollster::block_on(async {
            let mut g = Graph::new();
            let ids = [Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)];
            let mut w = FormeWorkspace::new(Uuid::from_u128(10));
            for id in ids {
                add_node(
                    &mut g,
                    Some(id),
                    "https://example.net".into(),
                    Default::default(),
                );
                w.open(id, &g).unwrap();
            }
            let mut layout = w.layout();
            layout.stack_all();
            w.keep_layout(&layout);
            w.locked = false;
            w.event(
                TileEvent::Dragged {
                    tile: w.tile_for_member(ids[2]).unwrap(),
                    to: DropTarget::Stack {
                        stack: workbench::TilePath(vec![]),
                        index: 0,
                    },
                },
                &g,
            )
            .unwrap();
            assert_eq!(
                w.geometry,
                Some(TreeGeometry::Stack {
                    members: vec![ids[2], ids[0], ids[1]],
                    active: 0
                })
            );
            w.event(TileEvent::Activated(w.tile_for_member(ids[1]).unwrap()), &g)
                .unwrap();
            let backend = muniment::MemoryBackend::new();
            w.save(backend.clone()).await.unwrap();
            let restored = FormeWorkspace::load(backend, w.document.graph_id, &g)
                .await
                .unwrap();
            assert_eq!(restored.geometry, w.geometry);
            assert_eq!(restored.region().cells[0].member, ids[1]);
        });
    }
}
