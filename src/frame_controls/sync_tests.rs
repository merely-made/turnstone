// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::cell::Cell;
use workbench::{ContentSource, SplitAxis, Tile, TileBranch};

thread_local! {
    static ENHANCED: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn enhanced() {
    ENHANCED.with(|count| count.set(count.get() + 1));
}

fn passes(work: impl FnOnce()) -> usize {
    ENHANCED.with(|count| count.set(0));
    work();
    ENHANCED.with(Cell::get)
}

fn workspace(left: f32) -> Workspace {
    let tile = |id: u64| Tile {
        id: TileId(id),
        title: format!("Tile {id}"),
        accent: None,
        content: ContentSource::Open {
            kind: "roster".into(),
            id: id.to_string(),
        },
    };
    Workspace::new(TileTree::split(
        SplitAxis::Row,
        vec![
            TileBranch {
                fraction: left,
                tree: TileTree::single(tile(1)),
            },
            TileBranch {
                fraction: 1.0 - left,
                tree: TileTree::single(tile(2)),
            },
        ],
    ))
}

fn divider_value(frame: &FrameControls) -> Option<String> {
    let dom = frame.dom.borrow();
    nodes(&dom, frame.root())
        .into_iter()
        .find(|node| matches!(frame.control_at(*node), Some(Control::Divider { .. })))
        .and_then(|node| attr(&dom, node, "aria-valuenow"))
}

#[test]
fn settled_sync_skips_and_every_input_resumes_it() {
    let mut frame = FrameControls::default();
    let settle = |frame: &mut FrameControls, workspace: &Workspace| {
        for _ in 0..3 {
            frame.sync(1, workspace.clone(), Some(TileId(1)));
        }
        passes(|| frame.sync(1, workspace.clone(), Some(TileId(1))))
    };
    let even = workspace(0.5);
    assert_eq!(settle(&mut frame, &even), 0);
    assert_eq!(divider_value(&frame).as_deref(), Some("50"));

    // An outside DOM write resumes the pass that restores the control.
    let divider = {
        let dom = frame.dom.borrow();
        nodes(&dom, frame.root())
            .into_iter()
            .find(|node| matches!(frame.control_at(*node), Some(Control::Divider { .. })))
            .unwrap()
    };
    frame
        .dom
        .borrow_mut()
        .set_attribute(divider, qual("aria-valuenow"), "1");
    assert_eq!(passes(|| frame.sync(1, even.clone(), Some(TileId(1)))), 1);
    assert_eq!(divider_value(&frame).as_deref(), Some("50"));
    assert_eq!(settle(&mut frame, &even), 0);

    // Runner focus moved without a DOM write is restored to the host's focus.
    assert!(frame.set_focus(Some(Control::Tab(TileId(2)))));
    assert_eq!(settle(&mut frame, &even), 0);
    let focused = frame.focus_node();
    assert!(focused.is_some());
    frame.runner.set_focus(None);
    assert_eq!(passes(|| frame.sync(1, even.clone(), Some(TileId(1)))), 1);
    assert_eq!(frame.runner.focus(), focused);

    // A changed workspace and an outside invalidation each resume it.
    let wide = workspace(0.75);
    frame.sync(1, wide.clone(), Some(TileId(1)));
    assert_eq!(divider_value(&frame).as_deref(), Some("75"));
    assert_eq!(settle(&mut frame, &wide), 0);
    frame.invalidate();
    assert_eq!(passes(|| frame.sync(1, wide.clone(), Some(TileId(1)))), 1);
}
