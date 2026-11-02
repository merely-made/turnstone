// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

#[test]
fn redshank_reopen_preserves_retained_draft_but_rejects_old_controls() {
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
    let source = payload();
    let item = source.item_id();
    app.open_redshank_episode(source.clone());
    app.apply_redshank_note_command(
        &item,
        NoteCommand {
            scope: app.redshank_note_scope(&item),
            capture_generation: None,
            command: CompactCommand::BeginTextNote,
        },
    );
    let generation = app.redshank.pending_capture_generation(&item).unwrap();
    let docks = RedshankDocks::new();
    let mut pane = registry(&docks)
        .admit(&PaneKindId::new(PANE_KIND), &episode_source(source.clone()))
        .unwrap();
    let viewport = SurfaceViewport {
        width: 720.0,
        height: 500.0,
        scale_factor: 1.0,
    };
    let mut projection = app.redshank_note_projection(&item);
    projection.compact.transport = TransportState::Paused;
    docks.project_notes(&item, projection);
    pane.session_mut().sync_viewport(viewport);
    let textbox = {
        let dom = pane.dom_ref();
        let mut pending = vec![pane.session().root()];
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
    pane.session_mut().set_focus(Some(textbox));
    pane.session_mut()
        .dispatch(ResolvedSurfaceEvent::Key(KeyEvent::new(Key::Character(
            "draft survives reopening".into(),
        ))));
    let play = node_with_label(&pane.dom_ref(), pane.session().root(), "Play");
    pane.session_mut().dispatch(ResolvedSurfaceEvent::Click {
        target: play,
        event: PointerClick::at((1.0, 1.0)),
    });
    app.open_redshank_episode(source);
    assert_eq!(
        app.redshank.pending_capture_generation(&item),
        Some(generation)
    );
    reconcile_redshank_captures(&mut app, &docks);
    let queued = docks.drain_scoped();
    assert_eq!(queued.len(), 1);
    assert!(
        app.apply_redshank_note_command(&item, queued.into_iter().next().unwrap().1)
            .is_empty()
    );
    assert_eq!(
        app.redshank.pending_capture_generation(&item),
        Some(generation)
    );
    docks.project_notes(&item, app.redshank_note_projection(&item));
    pane.session_mut().sync_viewport(viewport);
    let save = node_with_label(&pane.dom_ref(), pane.session().root(), "Save text note");
    pane.session_mut().dispatch(ResolvedSurfaceEvent::Click {
        target: save,
        event: PointerClick::at((1.0, 1.0)),
    });
    let queued = docks.drain_scoped();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].1.capture_generation, Some(generation));
    assert!(
        matches!(&queued[0].1.command, CompactCommand::SaveTextNote { plain_text, .. } if plain_text == "draft survives reopening")
    );
    assert!(
        !app.apply_redshank_note_command(&item, queued.into_iter().next().unwrap().1)
            .is_empty()
    );
}
