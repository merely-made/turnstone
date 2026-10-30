// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::tests::note_host;
use super::*;

#[test]
fn text_note_save_requires_the_current_frozen_anchor() {
    let (mut host, _, existing) = note_host();
    let item = ItemId("episode".into());
    host.command(&item, CompactCommand::BeginTextNote, 1);
    let frozen = host.pending_anchor(&item).unwrap().clone();
    let mut stale = frozen.clone();
    stale.offset_ms += 1;
    let refused = host.command(
        &item,
        CompactCommand::SaveTextNote {
            anchor: stale,
            plain_text: "stale".into(),
        },
        2,
    );
    assert!(refused.notice.is_some());
    assert!(!refused.model_changed);
    assert_eq!(host.pending_anchor(&item), Some(&frozen));
    assert_eq!(host.model.annotations.len(), 1);
    assert!(host.annotation(&existing).is_some());
    host.command(&item, CompactCommand::CancelTextNote, 3);
    let refused = host.command(
        &item,
        CompactCommand::SaveTextNote {
            anchor: frozen,
            plain_text: "after cancellation".into(),
        },
        4,
    );
    assert!(refused.notice.is_some());
    assert!(!refused.model_changed);
    assert_eq!(host.model.annotations.len(), 1);
}

#[test]
fn same_offset_captures_have_distinct_generations() {
    let (mut host, _, _) = note_host();
    let item = ItemId("episode".into());
    host.command(&item, CompactCommand::BeginTextNote, 1);
    let first_anchor = host.pending_anchor(&item).unwrap().clone();
    let first = host
        .pending_capture_generation(&item)
        .expect("first capture generation");
    host.command(&item, CompactCommand::CancelTextNote, 2);
    host.command(&item, CompactCommand::BeginTextNote, 3);
    let second_anchor = host.pending_anchor(&item).unwrap();
    let second = host
        .pending_capture_generation(&item)
        .expect("second capture generation");
    assert_eq!(&first_anchor, second_anchor);
    assert_ne!(first, second);
    let other = ItemId("other".into());
    host.model
        .add_item(LibraryItem::DirectAudio {
            id: other.clone(),
            title: "Other".into(),
            source: MediaSource::Enclosure {
                url: "https://example.invalid/other.mp3".into(),
            },
        })
        .unwrap();
    assert!(host.load_selection(&other, None).is_some());
    assert!(host.pending_anchor(&item).is_none());
    assert!(host.pending_capture_generation(&item).is_none());
}
