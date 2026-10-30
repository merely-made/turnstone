// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn waiting_lens_gets_its_turn_before_primary_restarts() {
    let primary = WindowId::from(1);
    let lens = WindowId::from(2);
    let mut queue = NativeFrameQueue::default();
    assert!(queue.prepare(primary));
    assert!(!queue.prepare(lens));
    queue.cancel(primary);
    assert!(
        !queue.prepare(primary),
        "primary stole the waiting lens turn"
    );
    assert!(queue.prepare(lens));
    queue.cancel(lens);
    assert!(queue.prepare(primary));
}

#[test]
fn retired_window_stops_driving_the_native_completion_clock() {
    let window = WindowId::from(1);
    let mut queue = NativeFrameQueue::default();
    assert!(queue.prepare(window));
    assert!(queue.active());
    queue.cancel(window);
    assert!(!queue.active());
    assert!(!queue.wants(window));
}

#[test]
fn model_invalidation_survives_a_submission_snapshot() {
    let queue = NativeFrameQueue::default();
    let submitted_revision = queue.revision();
    queue.invalidate();
    assert_ne!(submitted_revision, queue.revision());
    let next_submission = queue.revision();
    assert_eq!(
        next_submission,
        queue.revision(),
        "completion alone changed model revision"
    );
}

#[test]
fn native_and_checked_child_windows_share_one_fair_turn_queue() {
    let primary = WindowId::from(1);
    let child = WindowId::from(2);
    let lens = WindowId::from(3);
    let mut queue = NativeFrameQueue::default();
    assert!(queue.prepare(primary));
    assert!(!queue.prepare(child));
    assert!(!queue.prepare(lens));
    queue.cancel(primary);
    assert!(!queue.prepare(primary));
    assert!(queue.prepare(child));
    queue.cancel(child);
    assert!(!queue.prepare(primary));
    assert!(queue.prepare(lens));
    queue.cancel(lens);
    assert!(queue.prepare(primary));
}
