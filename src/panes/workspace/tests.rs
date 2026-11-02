// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use workbench::{DropTarget, TileEvent, TilePath, WorkbenchEffect};

fn two() -> PaneWorkspace<String> {
    let mut frame = PaneWorkspace::single(1, "Graph".into(), "graph".into());
    assert!(frame.insert_beside(1, Edge::Right, 2, "Inspector".into(), "inspector".into()));
    frame
}

#[test]
fn stacking_switching_closing_and_restart_preserve_payload_identity() {
    let mut frame = two();
    frame
        .apply(
            &TileEvent::Dragged {
                tile: TileId(2),
                to: DropTarget::Stack {
                    stack: TilePath(vec![0]),
                    index: 1,
                },
            }
            .into(),
        )
        .unwrap();
    assert_eq!(frame.workspace.tiled().tiles().len(), 2);
    assert_eq!(frame.active_ids(), vec![TileId(2)]);
    frame
        .apply(&TileEvent::Activated(TileId(1)).into())
        .unwrap();
    assert_eq!(frame.active_ids(), vec![TileId(1)]);
    let text = serde_json::to_string(&frame).unwrap();
    let mut restored: PaneWorkspace<String> = serde_json::from_str(&text).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored, frame);
    restored
        .apply(&TileEvent::Closed(TileId(2)).into())
        .unwrap();
    assert_eq!(restored.panes, BTreeMap::from([(1, "graph".into())]));
}

#[test]
fn outside_drop_retains_custody_until_host_accepts() {
    let mut frame = two();
    let before = frame.clone();
    let outcome = frame
        .apply(
            &TileEvent::Dragged {
                tile: TileId(2),
                to: DropTarget::Outside,
            }
            .into(),
        )
        .unwrap();
    assert_eq!(
        outcome.effect(),
        Some(WorkbenchEffect::TearOut { tile: TileId(2) })
    );
    assert_eq!(frame, before);
    assert_eq!(frame.take(2), Some("inspector".into()));
    frame.validate().unwrap();
}

#[test]
fn malformed_or_orphaned_payloads_refuse_without_mutation() {
    let mut frame = two();
    frame.panes.remove(&1);
    let before = frame.clone();
    assert!(frame.apply(&TileEvent::Closed(TileId(2)).into()).is_err());
    assert_eq!(frame, before);
}

#[test]
fn orphaned_view_state_is_rejected_and_old_frames_default_empty() {
    let mut frame = two();
    frame
        .view_states
        .insert(99, serde_json::json!({"expanded":true}));
    assert!(frame.validate().is_err());

    let old_frame = serde_json::json!({
        "workspace": frame.workspace,
        "panes": frame.panes,
        "descriptors": {},
    });
    let restored: PaneWorkspace<String> = serde_json::from_value(old_frame).unwrap();
    assert!(restored.view_states.is_empty());
}

#[test]
fn duplicate_ids_and_missing_drop_targets_do_not_orphan_a_pane() {
    let mut frame = two();
    let before = frame.clone();
    assert!(!frame.insert_beside(1, Edge::Bottom, 2, "Duplicate".into(), "duplicate".into()));
    assert!(!frame.insert_beside(999, Edge::Bottom, 3, "Missing".into(), "missing".into()));
    assert_eq!(frame, before);
}
#[test]
fn invalid_stack_drop_does_not_remove_source_custody() {
    let mut frame = two();
    let before = frame.clone();
    frame
        .apply(
            &TileEvent::Dragged {
                tile: TileId(2),
                to: DropTarget::Stack {
                    stack: TilePath(vec![99]),
                    index: 0,
                },
            }
            .into(),
        )
        .unwrap();
    assert_eq!(frame, before);
}
#[test]
fn non_finite_split_edit_is_refused_atomically() {
    let mut frame = two();
    let before = frame.clone();
    let _ = frame.apply(
        &TileEvent::DividerMoved {
            split: TilePath(vec![]),
            fractions: vec![f32::NAN, 1.0],
        }
        .into(),
    );
    assert_eq!(frame, before);
}
#[test]
fn duplicate_station_is_rejected_before_a_close() {
    let mut frame = two();
    let duplicate = frame.workspace.tiled().clone();
    *frame.workspace.tiled_mut() = TileTree::Split {
        axis: workbench::SplitAxis::Column,
        children: vec![
            workbench::TileBranch::new(0.5, duplicate.clone()),
            workbench::TileBranch::new(0.5, duplicate),
        ],
    };
    let before = frame.clone();
    assert!(frame.apply(&TileEvent::Closed(TileId(1)).into()).is_err());
    assert_eq!(frame, before);
}
#[test]
fn shared_float_and_dock_keep_the_same_host_payload() {
    let mut frame = two();
    frame
        .apply(
            &workbench::FloatEvent::Float {
                tile: TileId(2),
                rect: workbench::RelativeRect {
                    x: 0.1,
                    y: 0.2,
                    width: 0.4,
                    height: 0.5,
                },
            }
            .into(),
        )
        .unwrap();
    assert_eq!(frame.workspace.floating().len(), 1);
    assert_eq!(frame.panes.get(&2).unwrap(), "inspector");
    frame
        .apply(
            &workbench::FloatEvent::Dock {
                tile: TileId(2),
                target: workbench::FloatDockTarget::Tab { target: TileId(1) },
            }
            .into(),
        )
        .unwrap();
    assert!(frame.workspace.floating().is_empty());
    assert_eq!(frame.panes.len(), 2);
    frame.validate().unwrap();
}
#[test]
fn float_transfer_and_reopen_preserve_payload_descriptor_geometry_and_policy() {
    let mut source = two();
    source.descriptors.insert(
        2,
        serde_json::json!({"source":"same address, this access","draft":"retained"}),
    );
    source
        .view_states
        .insert(2, serde_json::json!({"expanded_groups":["settings"]}));
    source
        .apply(
            &workbench::FloatEvent::Float {
                tile: TileId(2),
                rect: workbench::RelativeRect {
                    x: 0.12,
                    y: 0.23,
                    width: 0.45,
                    height: 0.56,
                },
            }
            .into(),
        )
        .unwrap();
    source
        .apply(
            &workbench::FloatEvent::SetConstraints {
                tile: TileId(2),
                constraints: workbench::FloatSizeConstraints {
                    min_width: 220.0,
                    min_height: 90.0,
                    max_width: Some(800.0),
                    max_height: None,
                },
            }
            .into(),
        )
        .unwrap();
    source
        .apply(
            &workbench::FloatEvent::SetPinned {
                tile: TileId(2),
                pinned: true,
            }
            .into(),
        )
        .unwrap();
    let station = source.workspace.floating()[0].clone();
    let mut lens = PaneWorkspace::single(9, "Lens".into(), "other".into());
    source.transfer_to(&mut lens, 2, Some(9)).unwrap();
    assert_eq!(source.panes.len(), 1);
    assert!(source.descriptors.is_empty());
    assert!(source.view_states.is_empty());
    let moved = &lens.workspace.floating()[0];
    assert_eq!(moved.tile, station.tile);
    assert_eq!(moved.rect, station.rect);
    assert_eq!(moved.constraints, station.constraints);
    assert!(moved.pinned);
    let mut reopened: PaneWorkspace<String> =
        serde_json::from_str(&serde_json::to_string(&lens).unwrap()).unwrap();
    reopened.validate().unwrap();
    reopened.transfer_to(&mut source, 2, Some(1)).unwrap();
    source
        .apply(
            &workbench::FloatEvent::Dock {
                tile: TileId(2),
                target: workbench::FloatDockTarget::Tab { target: TileId(1) },
            }
            .into(),
        )
        .unwrap();
    assert_eq!(source.active_ids(), vec![TileId(2)]);
    assert_eq!(source.panes[&2], "inspector");
    assert_eq!(source.descriptors[&2]["draft"], "retained");
    assert_eq!(source.view_states[&2]["expanded_groups"][0], "settings");
    assert_eq!(reopened.panes.len(), 1);
}
#[test]
fn failed_cross_window_transfer_changes_neither_custody() {
    let mut source = two();
    let mut destination = PaneWorkspace::single(9, "Lens".into(), "other".into());
    let before = (source.clone(), destination.clone());
    assert!(source.transfer_to(&mut destination, 2, Some(999)).is_err());
    assert_eq!((source.clone(), destination.clone()), before);
    assert!(source.transfer_to(&mut destination, 999, Some(9)).is_err());
    assert_eq!((source, destination), before);
}
#[test]
fn malformed_float_geometry_is_refused_before_publish() {
    let mut frame = two();
    let before = frame.clone();
    assert!(
        frame
            .apply(
                &workbench::FloatEvent::Float {
                    tile: TileId(2),
                    rect: workbench::RelativeRect {
                        x: 0.0,
                        y: f32::NAN,
                        width: 0.5,
                        height: 0.5
                    }
                }
                .into()
            )
            .is_err()
    );
    assert_eq!(frame, before);
    frame
        .apply(
            &workbench::FloatEvent::Float {
                tile: TileId(2),
                rect: workbench::RelativeRect {
                    x: 0.0,
                    y: 0.0,
                    width: 0.5,
                    height: 0.5,
                },
            }
            .into(),
        )
        .unwrap();
    let before = frame.clone();
    assert!(
        frame
            .apply(
                &workbench::FloatEvent::SetConstraints {
                    tile: TileId(2),
                    constraints: workbench::FloatSizeConstraints {
                        min_width: 200.0,
                        min_height: 0.0,
                        max_width: Some(100.0),
                        max_height: None
                    }
                }
                .into()
            )
            .is_err()
    );
    assert_eq!(frame, before);
}
#[test]
fn closing_a_float_retires_its_descriptor_and_invisible_float_keeps_custody() {
    let mut frame = two();
    frame
        .descriptors
        .insert(2, serde_json::json!({"draft":true}));
    frame
        .apply(
            &workbench::FloatEvent::Float {
                tile: TileId(2),
                rect: workbench::RelativeRect {
                    x: 0.0,
                    y: 0.0,
                    width: 0.5,
                    height: 0.5,
                },
            }
            .into(),
        )
        .unwrap();
    frame
        .apply(
            &workbench::FloatEvent::SetVisible {
                tile: TileId(2),
                visible: false,
            }
            .into(),
        )
        .unwrap();
    assert_eq!(frame.active_ids(), vec![TileId(1)]);
    assert_eq!(frame.panes.len(), 2);
    frame.validate().unwrap();
    frame.apply(&TileEvent::Closed(TileId(2)).into()).unwrap();
    assert!(frame.descriptors.is_empty());
    assert_eq!(frame.panes.len(), 1);
}
