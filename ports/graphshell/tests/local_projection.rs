// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use graphshell::local_projection::ProjectionSession;
use graphshell::projection_editor::{Channel, EditorAction};

const SOURCE: &str = include_str!("../web/fixtures/woodshed-stage.json");

#[test]
fn projection_selection_survives_label_edit_and_saved_snapshot_reopen() {
    let mut session = ProjectionSession::load(SOURCE, None).unwrap();
    session.select("card:2").unwrap();
    let solves = session.solves();
    let mut appearance = session.editor().draft().appearance.clone();
    appearance.title = "My practice session".into();
    session.edit(EditorAction::SetAppearance(appearance), 1);
    assert_eq!(session.solves(), solves, "a title edit reuses placement");
    assert!(session.placement_reuses() > 0);
    assert_eq!(session.selected(), Some("card:2"));
    let saved = session.snapshot().unwrap();
    let mut reopened = ProjectionSession::load(SOURCE, None).unwrap();
    reopened.reopen(&saved).unwrap();
    assert_eq!(
        reopened.editor().draft().appearance.title,
        "My practice session"
    );
    assert_eq!(reopened.selected(), Some("card:2"));
    let compiled = reopened.compiled().unwrap();
    assert_eq!(
        compiled.selected,
        compiled.instance_by_occurrence.get("card:2").copied()
    );
}

#[test]
fn invalid_encoding_refuses_save_and_undo_restores_executable_selection() {
    let mut session = ProjectionSession::load(SOURCE, None).unwrap();
    session.select("card:2").unwrap();
    let mut encoding = session.editor().draft().encoding.clone();
    encoding.x = Channel::Field("missing".into());
    session.edit(EditorAction::SetEncoding(encoding), 1);
    assert!(session.compiled().is_none());
    assert!(session.snapshot().is_err());
    assert!(session.error().contains("encoding.x"));
    assert!(session.undo());
    assert!(session.compiled().is_some());
    assert_eq!(session.selected(), Some("card:2"));
    assert!(session.redo());
    assert!(session.snapshot().is_err());
}

#[test]
fn invalid_reopen_keeps_the_current_executable_projection() {
    let mut session = ProjectionSession::load(SOURCE, None).unwrap();
    session.select("card:2").unwrap();
    let before = session.snapshot().unwrap();
    let mut invalid = before.clone();
    invalid.definition.encoding.x = Channel::Field("missing".into());
    assert!(session.reopen(&invalid).is_err());
    assert_eq!(session.snapshot().unwrap(), before);
}
