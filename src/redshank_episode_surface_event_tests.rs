// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

#[test]
fn redshank_session_routes_typed_draft_and_note_privacy() {
    use cambium::{Key, KeyEvent, ResolvedSurfaceEvent};
    use redshank_model::{AnnotationId, CaptureAnchor, NotePrivacy};
    use redshank_surfaces::{NoteSummary, NoteSummaryBody};
    use uuid::Uuid;

    let docks = RedshankDocks::new();
    let item = payload().item_id();
    let mut pane = registry(&docks)
        .admit(&PaneKindId::new(PANE_KIND), &episode_source(payload()))
        .expect("Redshank episode admission");
    let mut scope = NoteScope {
        session: crate::panes::SessionId::new(),
        generation: Uuid::new_v4(),
        custody_epoch: Some(9),
        item: item.clone(),
        selected: Some(item.clone()),
        selected_capture_generation: None,
        control_generation: Uuid::new_v4(),
    };
    let mut compact = seed_state(&payload());
    compact.transport = TransportState::Unavailable("No output".into());
    compact.text_capture_available = Some(true);
    let mut projected = NoteProjection {
        compact,
        scope: Some(scope.clone()),
        capture: None,
        notes: Vec::new(),
    };
    let viewport = SurfaceViewport {
        width: 720.0,
        height: 500.0,
        scale_factor: 1.0,
    };
    docks.project_notes(&item, projected.clone());
    pane.session_mut().sync_viewport(viewport);
    let add = node_with_label(&pane.dom_ref(), pane.session().root(), "Add text note");
    pane.session_mut().dispatch(ResolvedSurfaceEvent::Click {
        target: add,
        event: PointerClick::at((1.0, 1.0)),
    });
    let issued = docks.drain_scoped();
    assert_eq!(issued.len(), 1);
    assert_eq!(issued[0].1.scope.as_ref(), Some(&scope));
    assert!(matches!(issued[0].1.command, CompactCommand::AddTextNote));

    let anchor = CaptureAnchor {
        item_id: item.clone(),
        offset_ms: 0,
        end_offset_ms: None,
        pressed_offset_ms: None,
        representation: Default::default(),
        fingerprint: None,
    };
    let generation = Uuid::new_v4();
    scope.selected_capture_generation = Some(generation);
    scope.control_generation = Uuid::new_v4();
    projected.scope = Some(scope.clone());
    projected.capture = Some((generation, anchor.clone()));
    projected.compact.text_capture_available = Some(false);
    docks.project_notes(&item, projected.clone());
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
            "typed note".into(),
        ))));
    projected.compact.volume_percent = 37;
    docks.project_notes(&item, projected.clone());
    pane.session_mut().sync_viewport(viewport);
    let save = node_with_label(&pane.dom_ref(), pane.session().root(), "Save text note");
    pane.session_mut().dispatch(ResolvedSurfaceEvent::Click {
        target: save,
        event: PointerClick::at((1.0, 1.0)),
    });
    let issued = docks.drain_scoped();
    assert_eq!(issued.len(), 1);
    assert_eq!(issued[0].1.capture_generation, Some(generation));
    assert_eq!(issued[0].1.scope.as_ref(), Some(&scope));
    assert!(
        matches!(&issued[0].1.command, CompactCommand::SaveTextNote { anchor: actual, plain_text } if actual == &anchor && plain_text == "typed note")
    );

    projected.capture = None;
    scope.selected_capture_generation = None;
    scope.control_generation = Uuid::new_v4();
    projected.scope = Some(scope.clone());
    projected.notes = vec![NoteSummary {
        id: AnnotationId("note-0".into()),
        item_id: item.clone(),
        offset_ms: 0,
        end_offset_ms: None,
        body: NoteSummaryBody::Text("typed note".into()),
        private: true,
    }];
    docks.project_notes(&item, projected);
    pane.session_mut().sync_viewport(viewport);
    let more = node_with_label(
        &pane.dom_ref(),
        pane.session().root(),
        "More actions for note at 0:00",
    );
    pane.session_mut().dispatch(ResolvedSurfaceEvent::Click {
        target: more,
        event: PointerClick::at((1.0, 1.0)),
    });
    let html = pane.dom_ref().outer_html(pane.session().root());
    assert!(!html.contains("Edit note at 0:00"));
    assert!(!html.contains("Delete note at 0:00"));
    let share = node_with_label(
        &pane.dom_ref(),
        pane.session().root(),
        "Make shareable: note at 0:00",
    );
    pane.session_mut().dispatch(ResolvedSurfaceEvent::Click {
        target: share,
        event: PointerClick::at((1.0, 1.0)),
    });
    let issued = docks.drain_scoped();
    assert_eq!(issued.len(), 1);
    assert_eq!(issued[0].1.scope.as_ref(), Some(&scope));
    assert!(
        matches!(&issued[0].1.command, CompactCommand::SetNotePrivacy { id, privacy: NotePrivacy::Shareable } if id.0 == "note-0")
    );
}

#[test]
fn redshank_note_body_has_room_above_wrapped_controls() {
    use redshank_model::AnnotationId;
    use redshank_surfaces::{NoteSummary, NoteSummaryBody};
    let docks = RedshankDocks::new();
    let item = payload().item_id();
    let mut pane = registry(&docks)
        .admit(&PaneKindId::new(PANE_KIND), &episode_source(payload()))
        .expect("Redshank episode admission");
    let mut compact = seed_state(&payload());
    compact.transport = TransportState::Unavailable("No output".into());
    docks.project_notes(
        &item,
        NoteProjection {
            compact,
            scope: None,
            capture: None,
            notes: vec![NoteSummary {
                id: AnnotationId("note-0".into()),
                item_id: item.clone(),
                offset_ms: 0,
                end_offset_ms: None,
                body: NoteSummaryBody::Text(
                    "A readable note with enough words to measure its width".into(),
                ),
                private: false,
            }],
        },
    );
    pane.session_mut().sync_viewport(SurfaceViewport {
        width: 600.0,
        height: 420.0,
        scale_factor: 1.0,
    });
    let dom = pane.dom_ref();
    let root = pane.session().root();
    let notes = dom.all_with_class(root, "rs-listen-notes")[0];
    assert!(
        !dom.has_class(notes, "rs-pane-off"),
        "the Notes pane must be visible at this width"
    );
    let body = dom.all_with_class(notes, "rs-note-text")[0];
    let actions = dom.all_with_class(notes, "rs-row-tail")[0];
    let sheet = compact_stylesheet();
    let (_, by, bw, bh) = crate::ui::subtree_node_rect(&dom, root, body, &sheet, 600, 420).unwrap();
    let (_, ay, aw, _) =
        crate::ui::subtree_node_rect(&dom, root, actions, &sheet, 600, 420).unwrap();
    assert!(bw >= 120.0, "note body was squeezed to {bw}px");
    assert!(
        ay >= by + bh,
        "actions must follow the note body: {ay} < {}",
        by + bh
    );
    assert!(aw <= 600.0, "actions extend beyond the surface: {aw}");
}
