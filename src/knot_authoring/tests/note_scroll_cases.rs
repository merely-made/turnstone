// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn long_note_scrolls_to_unicode_text_and_pointer_selection_uses_scrolled_geometry() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("note.djot");
    let token = format!("mere://saved-source/{}", "a".repeat(160));
    let source = format!(
        "# Note\n\n{token}\n{}\nselected café 雪 passage\n",
        "metadata line\n".repeat(60)
    );
    fs::write(&path, &source).unwrap();
    let (hub, _) = KnotHub::host(
        HostedKnot::Directory {
            root: root.path().into(),
        },
        None,
        None,
        65536,
        Arc::new(|| {}),
    )
    .unwrap();
    let address = file_address(&path);
    let mut session = KnotDocumentSession::new(
        hub.clone(),
        hub.open(&address).unwrap(),
        (500, 300),
        false,
        false,
        false,
    );
    let _ = session.frame(500, 300);
    let field = find_text_control(&session.dom.borrow(), session.runner.root()).unwrap();
    let token_end = source.find(&token).unwrap() + token.len();
    let mut last_visible = None;
    for y in 0..250 {
        for x in 20..330 {
            let caret = session.layout.text_caret_at(
                &mut session.dom.borrow_mut(),
                field,
                x as f32 + 0.5,
                y as f32 + 0.5,
            );
            if caret.is_some_and(|p| p.byte == token_end) {
                last_visible = Some((x, y));
                break;
            }
        }
        if last_visible.is_some() {
            break;
        }
    }
    let (_, token_end_y) = last_visible.expect("end of long URI must wrap into the editor column");
    assert!(
        token_end_y > 100,
        "the long URI should occupy multiple lines"
    );
    assert!(
        !session.scroll_by(0.0, -100.0),
        "scroll at the top does not move"
    );
    assert!(session.scroll_by(0.0, 100.0));
    let _ = session.frame(500, 300);
    assert!(
        session.scroll.offset().1 > 0.0,
        "long note must move within the viewport"
    );
    let start = source.find("selected café 雪 passage").unwrap();
    let end = start + "selected café 雪 passage".len();
    let mut row = None;
    for _ in 0..30 {
        let _ = session.frame(500, 300);
        for y in 0..300 {
            let y = y as f32 + 0.5;
            let at_left =
                session
                    .layout
                    .text_caret_at(&mut session.dom.borrow_mut(), field, 25.0, y);
            if !at_left.is_some_and(|p| (start..=end).contains(&p.byte)) {
                continue;
            }
            let mut first = None;
            let mut last = None;
            for x in 10..340 {
                let x = x as f32 + 0.5;
                let caret =
                    session
                        .layout
                        .text_caret_at(&mut session.dom.borrow_mut(), field, x, y);
                if caret.is_some_and(|p| p.byte == start) {
                    first = Some(x);
                }
                if caret.is_some_and(|p| p.byte == end) {
                    last = Some(x);
                    break;
                }
            }
            if let (Some(a), Some(b)) = (first, last) {
                row = Some((a, b, y));
                break;
            }
        }
        if row.is_some() {
            break;
        }
        assert!(session.scroll_by(0.0, 100.0));
    }
    let (a, b, y) = row.expect("selected phrase must be visible after scrolling");
    session.pointer_down(a, y);
    session.pointer_move(b, y);
    session.pointer_up(b, y);
    let selection = session.runner.state().editor.selection();
    assert_eq!(
        &source[selection.anchor.byte.min(selection.focus.byte)
            ..selection.anchor.byte.max(selection.focus.byte)],
        "selected café 雪 passage"
    );
    assert_eq!(session.runner.state().editor.source(), source);
}
