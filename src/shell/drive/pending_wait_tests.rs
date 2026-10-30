// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::PendingScenarioWait;
#[test]
fn pending_file_wait_observes_empty_then_populated_file_on_later_frames() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ready");
    let mut wait = PendingScenarioWait::file(path.clone(), 3);
    assert!(!wait.poll(&[]).unwrap());
    std::fs::write(&path, b"").unwrap();
    assert!(!wait.poll(&[]).unwrap());
    std::fs::write(&path, b"ready").unwrap();
    assert!(wait.poll(&[]).unwrap());
}
#[test]
fn pending_file_wait_refuses_an_exhausted_budget_but_accepts_a_ready_last_frame() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ready");
    let mut wait = PendingScenarioWait::file(path.clone(), 1);
    assert!(!wait.poll(&[]).unwrap());
    assert!(wait.poll(&[]).is_err());
    std::fs::write(&path, b"ready").unwrap();
    assert!(wait.poll(&[]).unwrap());
}
#[test]
fn status_wait_returns_between_frames_and_names_the_last_status_on_failure() {
    let mut wait = PendingScenarioWait::status("3 messages".into(), 1);
    assert!(!wait.poll(&["2 messages".into()]).unwrap());
    let error = wait.poll(&["still 2 messages".into()]).unwrap_err();
    assert!(error.contains("3 messages") && error.contains("still 2 messages"));
    assert!(
        wait.poll(&["Retained history: 1 shared nodes, 3 messages".into()])
            .unwrap()
    );
}
