// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

#[test]
fn stale_redshank_controls_preserve_newer_selection_and_typed_draft() {
    use cambium::{Key, KeyEvent, ResolvedSurfaceEvent};

    let root = tempfile::tempdir().unwrap();
    let mut app = crate::app::App::test_stub_at(root.path().to_path_buf());
    let custody = crate::trail_memory::custody_test_fixture::custody();
    app.research.set_available(true);
    app.research.access(Some(crate::research::ClipAccess {
        root: root.path().to_path_buf(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    let a_source = payload();
    let a = a_source.item_id();
    app.open_redshank_episode(a_source.clone());
    let scope = app.redshank_note_scope(&a).unwrap();
    app.apply_redshank_note_command(
        &a,
        NoteCommand {
            scope: Some(scope),
            capture_generation: None,
            command: CompactCommand::BeginTextNote,
        },
    );
    let anchor = app.redshank.pending_anchor(&a).unwrap().clone();
    let generation = app.redshank.pending_capture_generation(&a).unwrap();
    let scope = app.redshank_note_scope(&a).unwrap();
    app.apply_redshank_note_command(
        &a,
        NoteCommand {
            scope: Some(scope),
            capture_generation: Some(generation),
            command: CompactCommand::SaveTextNote {
                anchor,
                plain_text: "existing A note".into(),
            },
        },
    );
    let old_note = app
        .redshank
        .model()
        .annotations
        .values()
        .next()
        .unwrap()
        .id
        .clone();
    let docks = RedshankDocks::new();
    let mut a_pane = registry(&docks)
        .admit(&PaneKindId::new(PANE_KIND), &episode_source(a_source))
        .unwrap();
    let viewport = SurfaceViewport {
        width: 720.0,
        height: 500.0,
        scale_factor: 1.0,
    };
    let mut a_projection = app.redshank_note_projection(&a);
    a_projection.compact.transport = TransportState::Paused;
    docks.project_notes(&a, a_projection.clone());
    a_pane.session_mut().sync_viewport(viewport);
    let play = node_with_label(&a_pane.dom_ref(), a_pane.session().root(), "Play");
    a_pane.session_mut().dispatch(ResolvedSurfaceEvent::Click {
        target: play,
        event: PointerClick::at((1.0, 1.0)),
    });
    let mut queued = docks.drain_scoped();
    assert_eq!(queued.len(), 1);
    assert!(matches!(queued[0].1.command, CompactCommand::Play));
    let old_scope = queued[0].1.scope.clone();
    for command in [
        CompactCommand::Pause,
        CompactCommand::Replay,
        CompactCommand::Seek(2_000),
        CompactCommand::SkipBackward(1_000),
        CompactCommand::SkipForward(1_000),
        CompactCommand::OpenNote(old_note.clone()),
        CompactCommand::OpenNoteApproximately(old_note.clone()),
        CompactCommand::OpenAlignedNote(old_note.clone()),
        CompactCommand::PlaySpan(old_note.clone()),
        CompactCommand::OpenVoiceNote(old_note.clone()),
        CompactCommand::PlayVoiceNote(old_note.clone()),
        CompactCommand::StopVoiceNote(old_note),
    ] {
        queued.push((
            a.clone(),
            NoteCommand {
                scope: old_scope.clone(),
                capture_generation: None,
                command,
            },
        ));
    }

    let mut b_source = payload();
    b_source.guid = "episode-2".into();
    b_source.enclosure_url = "https://example.invalid/b.mp3".into();
    let b = b_source.item_id();
    app.open_redshank_episode(b_source.clone());
    let b_scope = app.redshank_note_scope(&b).unwrap();
    queued.push((
        b.clone(),
        NoteCommand {
            scope: Some(b_scope.clone()),
            capture_generation: None,
            command: CompactCommand::Pause,
        },
    ));
    app.apply_redshank_note_command(
        &b,
        NoteCommand {
            scope: Some(b_scope),
            capture_generation: None,
            command: CompactCommand::BeginTextNote,
        },
    );
    let capture = app.redshank.pending_capture_generation(&b).unwrap();
    let mut b_pane = registry(&docks)
        .admit(&PaneKindId::new(PANE_KIND), &episode_source(b_source))
        .unwrap();
    docks.project_notes(&b, app.redshank_note_projection(&b));
    b_pane.session_mut().sync_viewport(viewport);
    let textbox = {
        let dom = b_pane.dom_ref();
        let mut pending = vec![b_pane.session().root()];
        let namespace = Namespace::from("");
        let role = LocalName::from("role");
        loop {
            let node = pending.pop().expect("note textbox");
            if dom
                .attribute(node, &namespace, &role)
                .is_some_and(|value| value == "textbox")
            {
                break node;
            }
            pending.extend(dom.dom_children(node));
        }
    };
    b_pane.session_mut().set_focus(Some(textbox));
    b_pane
        .session_mut()
        .dispatch(ResolvedSurfaceEvent::Key(KeyEvent::new(Key::Character(
            "newer draft".into(),
        ))));
    let before_model = app.redshank.model().clone();
    let before_projection = app.redshank_projection(&b);
    for (item, command) in queued {
        let effects = app.apply_redshank_note_command(&item, command);
        assert!(effects.is_empty(), "stale control was admitted");
        assert_eq!(app.redshank.selected(), Some(&b));
        assert_eq!(app.redshank.pending_capture_generation(&b), Some(capture));
        assert_eq!(app.redshank.model(), &before_model);
        assert_eq!(app.redshank_projection(&b), before_projection);
    }
    docks.project_notes(&b, app.redshank_note_projection(&b));
    b_pane.session_mut().sync_viewport(viewport);
    let save = node_with_label(&b_pane.dom_ref(), b_pane.session().root(), "Save text note");
    b_pane.session_mut().dispatch(ResolvedSurfaceEvent::Click {
        target: save,
        event: PointerClick::at((1.0, 1.0)),
    });
    let issued = docks.drain_scoped();
    assert_eq!(issued.len(), 1);
    assert_eq!(issued[0].1.capture_generation, Some(capture));
    assert!(
        matches!(&issued[0].1.command, CompactCommand::SaveTextNote { plain_text, .. } if plain_text == "newer draft")
    );

    // A freshly rendered inactive dock may still deliberately select A.
    let mut fresh_a = app.redshank_note_projection(&a);
    for command in [
        CompactCommand::Replay,
        CompactCommand::Pause,
        CompactCommand::Seek(2_000),
    ] {
        let effects = app.apply_redshank_note_command(
            &a,
            NoteCommand {
                scope: fresh_a.scope.clone(),
                capture_generation: None,
                command,
            },
        );
        assert!(effects.is_empty());
        assert_eq!(app.redshank.selected(), Some(&b));
        assert_eq!(app.redshank.pending_capture_generation(&b), Some(capture));
    }
    fresh_a.compact.transport = TransportState::Paused;
    docks.project_notes(&a, fresh_a);
    a_pane.session_mut().sync_viewport(viewport);
    let play = node_with_label(&a_pane.dom_ref(), a_pane.session().root(), "Play");
    a_pane.session_mut().dispatch(ResolvedSurfaceEvent::Click {
        target: play,
        event: PointerClick::at((1.0, 1.0)),
    });
    let issued = docks.drain_scoped();
    assert_eq!(issued.len(), 1);
    app.apply_redshank_note_command(&a, issued.into_iter().next().unwrap().1);
    assert_eq!(app.redshank.selected(), Some(&a));
}
