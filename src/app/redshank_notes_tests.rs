// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::super::tests::{FEED, PAGE, subscribed};
use super::*;
use crate::action::Action;
use crate::panes::SessionId;

#[test]
fn note_commands_reject_stale_owner_and_capture_before_writing() {
    use redshank_model::{NoteBody, NotePrivacy};
    let root = tempfile::tempdir().unwrap();
    let session = SessionId::new();
    let mut app = subscribed(root.path(), session);
    app.update(Action::OpenAddress(PAGE.to_string()));
    let item = crate::redshank_episode_surface::item_id(FEED, "episode-1");
    let custody = crate::trail_memory::custody_test_fixture::custody();
    app.research.set_available(true);
    app.research.access(Some(crate::research::ClipAccess {
        root: root.path().to_path_buf(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    let scope = app.redshank_note_scope(&item).unwrap();
    app.apply_redshank_note_command(
        &item,
        NoteCommand {
            scope: Some(scope.clone()),
            capture_generation: None,
            command: CompactCommand::BeginTextNote,
        },
    );
    let anchor = app.redshank.pending_anchor(&item).unwrap().clone();
    let first = app.redshank.pending_capture_generation(&item).unwrap();
    let scope = app.redshank_note_scope(&item).unwrap();
    let mut stale = Vec::new();
    let mut changed = scope.clone();
    changed.session = SessionId::new();
    stale.push(changed);
    let mut changed = scope.clone();
    changed.generation = Uuid::new_v4();
    stale.push(changed);
    let mut changed = scope.clone();
    changed.custody_epoch = Some(changed.custody_epoch.unwrap() + 1);
    stale.push(changed);
    let mut changed = scope.clone();
    changed.item = ItemId("other".into());
    stale.push(changed);
    for scope in stale {
        app.apply_redshank_note_command(
            &item,
            NoteCommand {
                scope: Some(scope),
                capture_generation: Some(first),
                command: CompactCommand::SaveTextNote {
                    anchor: anchor.clone(),
                    plain_text: "stale".into(),
                },
            },
        );
        assert!(app.redshank.model().annotations.is_empty());
        assert_eq!(app.redshank.pending_capture_generation(&item), Some(first));
    }
    app.apply_redshank_note_command(
        &item,
        NoteCommand {
            scope: Some(scope.clone()),
            capture_generation: Some(first),
            command: CompactCommand::CancelTextNote,
        },
    );
    app.apply_redshank_note_command(
        &item,
        NoteCommand {
            scope: app.redshank_note_scope(&item),
            capture_generation: None,
            command: CompactCommand::BeginTextNote,
        },
    );
    assert_eq!(app.redshank.pending_anchor(&item), Some(&anchor));
    let second = app.redshank.pending_capture_generation(&item).unwrap();
    assert_ne!(first, second);
    let scope = app.redshank_note_scope(&item).unwrap();
    app.apply_redshank_note_command(
        &item,
        NoteCommand {
            scope: Some(scope.clone()),
            capture_generation: Some(first),
            command: CompactCommand::SaveTextNote {
                anchor: anchor.clone(),
                plain_text: "old editor".into(),
            },
        },
    );
    app.apply_redshank_note_command(
        &item,
        NoteCommand {
            scope: Some(scope.clone()),
            capture_generation: Some(first),
            command: CompactCommand::CancelTextNote,
        },
    );
    assert!(app.redshank.model().annotations.is_empty());
    assert_eq!(app.redshank.pending_capture_generation(&item), Some(second));
    app.apply_redshank_note_command(
        &item,
        NoteCommand {
            scope: Some(scope.clone()),
            capture_generation: Some(second),
            command: CompactCommand::SaveTextNote {
                anchor,
                plain_text: "actual note".into(),
            },
        },
    );
    let note = app
        .redshank
        .model()
        .annotations
        .values()
        .next()
        .unwrap()
        .clone();
    assert_eq!(note.privacy, NotePrivacy::Private);
    assert!(matches!(&note.body, NoteBody::Text { plain_text } if plain_text == "actual note"));
    assert!(app.redshank.model().export_selected_note(&note.id).is_err());
    let scope = app.redshank_note_scope(&item).unwrap();
    app.apply_redshank_note_command(
        &item,
        NoteCommand {
            scope: Some(scope),
            capture_generation: None,
            command: CompactCommand::SetNotePrivacy {
                id: note.id.clone(),
                privacy: NotePrivacy::Shareable,
            },
        },
    );
    assert!(app.redshank.model().export_selected_note(&note.id).is_ok());
    assert_eq!(app.redshank.annotation(&note.id).unwrap().body, note.body);
    let scope = app.redshank_note_scope(&item).unwrap();
    app.apply_redshank_note_command(
        &item,
        NoteCommand {
            scope: Some(scope.clone()),
            capture_generation: None,
            command: CompactCommand::BeginTextNote,
        },
    );
    let old_projection = app.redshank_note_projection(&item);
    let (generation, anchor) = old_projection.capture.clone().unwrap();
    let scope = app.redshank_note_scope(&item).unwrap();
    app.research.set_available(false);
    app.apply_redshank_note_command(
        &item,
        NoteCommand {
            scope: Some(scope),
            capture_generation: Some(generation),
            command: CompactCommand::SaveTextNote {
                anchor,
                plain_text: "after custody loss".into(),
            },
        },
    );
    assert_eq!(app.redshank.model().annotations.len(), 1);
    assert_eq!(
        app.redshank.pending_capture_generation(&item),
        Some(generation)
    );
    app.invalidate_redshank_capture(&item, &old_projection);
    assert!(app.redshank.pending_capture_generation(&item).is_none());
    assert_eq!(
        app.redshank.annotation(&note.id).unwrap().privacy,
        NotePrivacy::Shareable
    );
}
