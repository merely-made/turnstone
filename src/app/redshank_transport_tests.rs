// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

fn fixture() -> (tempfile::TempDir, App, ItemId, ItemId, Arc<AtomicU64>) {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    let a = app
        .redshank_episode_for_address("https://example.invalid/a.mp3")
        .unwrap();
    let b = app
        .redshank_episode_for_address("https://example.invalid/b.mp3")
        .unwrap();
    let a_id = a.item_id();
    let b_id = b.item_id();
    app.open_redshank_episode(b);
    app.open_redshank_episode(a);
    let custody = crate::trail_memory::custody_test_fixture::custody();
    let epoch = Arc::new(AtomicU64::new(1));
    let current = epoch.clone();
    app.research.set_available(true);
    app.research.access(Some(crate::research::ClipAccess {
        root: root.path().to_path_buf(),
        sealer: custody.sealer,
        epoch: Arc::new(move || Some(current.load(Ordering::SeqCst))),
    }));
    (root, app, a_id, b_id, epoch)
}

#[test]
fn stale_redshank_replay_rejects_every_owner_transition_before_side_effects() {
    for transition in [
        "item",
        "item-return",
        "capture-return",
        "session",
        "generation",
        "custody",
    ] {
        let (_root, mut app, a, b, epoch) = fixture();
        let scope = app.redshank_note_scope(&a).unwrap();
        let queued = NoteCommand {
            scope: Some(scope),
            capture_generation: None,
            command: CompactCommand::Replay,
        };
        match transition {
            "item" => {
                let source = app
                    .redshank_episode_for_address("https://example.invalid/b.mp3")
                    .unwrap();
                app.open_redshank_episode(source);
                assert_eq!(app.redshank.selected(), Some(&b));
            },
            "item-return" => {
                let other = app
                    .redshank_episode_for_address("https://example.invalid/b.mp3")
                    .unwrap();
                let original = app
                    .redshank_episode_for_address("https://example.invalid/a.mp3")
                    .unwrap();
                app.open_redshank_episode(other);
                app.open_redshank_episode(original);
            },
            "capture-return" => {
                let scope = app.redshank_note_scope(&a).unwrap();
                app.apply_redshank_note_command(
                    &a,
                    NoteCommand {
                        scope: Some(scope.clone()),
                        capture_generation: None,
                        command: CompactCommand::BeginTextNote,
                    },
                );
                let capture = app.redshank.pending_capture_generation(&a).unwrap();
                let current = app.redshank_note_scope(&a).unwrap();
                app.apply_redshank_note_command(
                    &a,
                    NoteCommand {
                        scope: Some(current),
                        capture_generation: Some(capture),
                        command: CompactCommand::CancelTextNote,
                    },
                );
                assert!(app.redshank.pending_capture_generation(&a).is_none());
            },
            "session" => app.session_id = crate::panes::SessionId::new(),
            "generation" => app.source_capture_generation = Uuid::new_v4(),
            "custody" => epoch.store(2, Ordering::SeqCst),
            _ => unreachable!(),
        }
        let selected = app.redshank.selected().cloned().unwrap();
        let before_model = app.redshank.model().clone();
        let before_transport = app.redshank_projection(&selected);
        let effects = app.apply_redshank_note_command(&a, queued);
        assert!(
            effects.is_empty(),
            "stale Replay admitted after {transition}"
        );
        assert_eq!(app.redshank.selected(), Some(&selected));
        assert_eq!(app.redshank.model(), &before_model);
        assert_eq!(app.redshank_projection(&selected), before_transport);
    }
}

#[test]
fn fresh_redshank_play_remains_available_when_custody_is_unavailable() {
    let (_root, mut app, a, _, _) = fixture();
    app.research.set_available(false);
    let scope = app.redshank_note_projection(&a).scope;
    let effects = app.apply_redshank_note_command(
        &a,
        NoteCommand {
            scope,
            capture_generation: None,
            command: CompactCommand::Play,
        },
    );
    assert!(!effects.is_empty());
    assert_eq!(app.redshank.selected(), Some(&a));
    let scope = app.redshank_note_projection(&a).scope;
    let effects = app.apply_redshank_note_command(
        &a,
        NoteCommand {
            scope,
            capture_generation: None,
            command: CompactCommand::Replay,
        },
    );
    assert!(
        effects.is_empty(),
        "Replay can write progress and requires custody"
    );
}
