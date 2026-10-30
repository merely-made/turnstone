// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn retained_voice_stop_cannot_discard_a_different_pending_preview() {
    let mut host = RedshankHost::default();
    *host.retained_voice.lock().unwrap() = Some(("clip:new".into(), vec![1, 2, 3]));
    host.stop_retained_voice("old").unwrap();
    assert!(host.retained_voice.lock().unwrap().is_some());
    host.stop_retained_voice("new").unwrap();
    assert!(host.retained_voice.lock().unwrap().is_none());
}

fn receipt(byte: char) -> RepresentationReceipt {
    RepresentationReceipt {
        byte_length: Some(1_000),
        complete_digest: Some(format!("blake3:{}", byte.to_string().repeat(64))),
        ..Default::default()
    }
}

fn note_host() -> (RedshankHost, PlaybackSnapshot, AnnotationId) {
    let mut host = RedshankHost::default();
    let item = ItemId("episode".into());
    host.model
        .add_item(LibraryItem::DirectAudio {
            id: item.clone(),
            title: "Episode".into(),
            source: MediaSource::Enclosure {
                url: "https://example.invalid/audio.mp3".into(),
            },
        })
        .unwrap();
    host.selected = Some(item.clone());
    host.token = 7;
    host.model.progress.insert(
        item.clone(),
        Progress {
            position_ms: 8_000,
            completed: false,
            updated_at_ms: 1,
        },
    );
    let id = AnnotationId("note".into());
    host.model
        .add_text_annotation(
            id.clone(),
            CaptureAnchor {
                item_id: item,
                offset_ms: 16_000,
                end_offset_ms: Some(19_000),
                pressed_offset_ms: None,
                representation: receipt('a'),
                fingerprint: Some(redshank_model::AudioFingerprint {
                    version: 1,
                    frame_ms: 100,
                    anchor_offset_ms: 5_000,
                    frames: vec![[1; 16]; 50],
                }),
            },
            "original note".into(),
            0,
        )
        .unwrap();
    let snapshot = PlaybackSnapshot {
        load_token: Some(7),
        state: PlaybackState::Paused,
        representation: Some(receipt('a')),
        position_ms: 8_000,
        duration_ms: Some(60_000),
        ..Default::default()
    };
    (host, snapshot, id)
}

#[test]
fn embedded_note_privacy_is_scoped_persisted_and_rechecked_for_export() {
    use redshank_model::NotePrivacy;
    let root = tempfile::tempdir().unwrap();
    let (mut host, _, id) = note_host();
    let item = ItemId("episode".into());
    host.store = Some(JsonDirectoryStore::new(RedshankHost::model_root(
        root.path(),
    )));
    let original = host.annotation(&id).unwrap().clone();
    assert!(host.model.export_selected_note(&id).is_err());
    let rejected = host.command(
        &ItemId("foreign".into()),
        CompactCommand::SetNotePrivacy {
            id: id.clone(),
            privacy: NotePrivacy::Shareable,
        },
        1,
    );
    assert!(rejected.notice.is_some() && !rejected.model_changed);
    assert_eq!(host.annotation(&id), Some(&original));
    let changed = host.command(
        &item,
        CompactCommand::SetNotePrivacy {
            id: id.clone(),
            privacy: NotePrivacy::Shareable,
        },
        2,
    );
    assert!(changed.model_changed && changed.notice.is_none());
    let shareable = host.annotation(&id).unwrap();
    assert_eq!(shareable.target, original.target);
    assert_eq!(shareable.body, original.body);
    assert!(host.model.export_selected_note(&id).is_ok());
    host.save();
    let mut reopened = RedshankHost::open_with(root.path(), Output::Silent, None);
    assert_eq!(
        reopened.annotation(&id).unwrap().privacy,
        NotePrivacy::Shareable
    );
    assert!(
        reopened
            .command(
                &item,
                CompactCommand::SetNotePrivacy {
                    id: id.clone(),
                    privacy: NotePrivacy::Private
                },
                3
            )
            .model_changed
    );
    assert!(reopened.model.export_selected_note(&id).is_err());
    assert!(
        reopened
            .command(
                &item,
                CompactCommand::SetNotePrivacy {
                    id: AnnotationId("missing".into()),
                    privacy: NotePrivacy::Shareable
                },
                4
            )
            .notice
            .is_some()
    );
}

#[test]
#[ignore = "explicit marked embedded Redshank text-note process fixture"]
fn embedded_text_retention_process_fixture() {
    use redshank_model::{NotePrivacy, TimedTarget};
    let root = PathBuf::from(std::env::var_os("REDSHANK_TEXT_FIXTURE_ROOT").expect("fixture root"));
    assert_eq!(
        std::fs::read_to_string(root.join("fixture-marker")).unwrap(),
        "redshank-text-retention-20261020\n"
    );
    let profile = root.join("producer");
    let session = std::fs::read_to_string(profile.join("current_session")).unwrap();
    let session: uuid::Uuid = session.trim().parse().unwrap();
    let session_dir = profile.join("sessions").join(session.to_string());
    let mut host = RedshankHost::open_with(&session_dir, Output::Silent, None);
    assert!(host.model.annotations.is_empty() && host.model.library.is_empty());
    let source = root.join("origin-recording.wav");
    let bytes = std::fs::read(&source).unwrap();
    let item = ItemId("embedded-recording".into());
    host.model
        .add_item(LibraryItem::DirectAudio {
            id: item.clone(),
            title: "Embedded text-note fixture".into(),
            source: MediaSource::Local {
                path: source.to_string_lossy().into_owned(),
            },
        })
        .unwrap();
    for (id, text, shareable, offset) in [
        (
            "embedded-text-selected",
            "Saved from the embedded listener: café → harbor.\nKeep this timed note.",
            true,
            1250,
        ),
        (
            "embedded-text-private",
            "PRIVATE CONTROL: do not retain this note",
            false,
            7000,
        ),
        (
            "embedded-text-other",
            "UNSELECTED CONTROL: another shareable note",
            true,
            9000,
        ),
    ] {
        let id = AnnotationId(id.into());
        host.model
            .add_annotation(Annotation {
                id: id.clone(),
                target: TimedTarget {
                    item_id: item.clone(),
                    offset_ms: offset,
                    end_offset_ms: Some(offset + 3250),
                    pressed_offset_ms: Some(offset + 150),
                    fingerprint: None,
                    representation: RepresentationReceipt {
                        byte_length: Some(bytes.len() as u64),
                        complete_digest: Some(format!("blake3:{}", blake3::hash(&bytes).to_hex())),
                        ..Default::default()
                    },
                },
                body: NoteBody::Text {
                    plain_text: text.into(),
                },
                created_at_ms: 7,
                privacy: NotePrivacy::Private,
            })
            .unwrap();
        if shareable {
            assert!(
                host.command(
                    &item,
                    CompactCommand::SetNotePrivacy {
                        id,
                        privacy: NotePrivacy::Shareable
                    },
                    8
                )
                .model_changed
            );
        }
    }
    host.save();
    std::fs::write(
        root.join("expected-note-export.json"),
        serde_json::to_vec_pretty(
            &host
                .model
                .export_selected_note(&AnnotationId("embedded-text-selected".into()))
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    std::fs::write(root.join("source-session"), session.to_string()).unwrap();
    std::fs::write(
        root.join("seed-model.json"),
        serde_json::to_vec_pretty(&host.model).unwrap(),
    )
    .unwrap();
}

fn request(
    host: &RedshankHost,
    id: &AnnotationId,
    approximate: bool,
    aligned: bool,
    span: bool,
) -> PendingNoteOpen {
    PendingNoteOpen {
        token: host.token,
        id: id.clone(),
        approximate,
        aligned,
        span,
    }
}

#[test]
fn note_opens_require_ready_matching_receipts_or_explicit_approximation() {
    let (mut host, snapshot, id) = note_host();
    for representation in [
        None,
        Some(RepresentationReceipt::default()),
        Some(receipt('b')),
    ] {
        let changed = PlaybackSnapshot {
            representation,
            ..snapshot.clone()
        };
        assert!(
            host.finish_note_open(request(&host, &id, false, false, false), &changed)
                .is_err()
        );
        assert!(host.pending_note_seek.is_none());
        let commands = host
            .finish_note_open(request(&host, &id, true, false, false), &changed)
            .unwrap();
        assert!(matches!(
            &commands[0],
            PlaybackCommand::SeekNote {
                token: 7,
                position_ms: 16_000,
                ..
            }
        ));
        host.pending_note_seek = None;
    }
    for state in [
        PlaybackState::Loading,
        PlaybackState::Unavailable("failed".into()),
    ] {
        let loading = PlaybackSnapshot {
            state,
            ..snapshot.clone()
        };
        assert!(
            host.finish_note_open(request(&host, &id, true, false, false), &loading)
                .is_err()
        );
    }
    let stale = PlaybackSnapshot {
        load_token: Some(6),
        ..snapshot
    };
    assert!(
        host.finish_note_open(request(&host, &id, true, false, false), &stale)
            .is_err()
    );
}

#[test]
fn another_items_note_loads_paused_at_zero_and_holds_saved_progress() {
    let (mut host, snapshot, id) = note_host();
    host.selected = None;
    let commands = host
        .note_commands(id, false, false, false, &snapshot)
        .unwrap();
    assert_eq!(commands.len(), 1);
    assert!(matches!(
        &commands[0],
        PlaybackCommand::Load {
            token: 8,
            resume_ms: 0,
            ..
        }
    ));
    assert!(host.pending_note_open.is_some());
    let ready = PlaybackSnapshot {
        load_token: Some(8),
        position_ms: 0,
        representation: None,
        ..snapshot
    };
    let pending = host.pending_note_open.take().unwrap();
    assert!(host.finish_note_open(pending, &ready).is_err());
    assert!(!host.record_snapshot_progress(&ready, 9));
    assert_eq!(
        host.model.progress[&ItemId("episode".into())].position_ms,
        8_000
    );
}

#[test]
fn only_the_latest_seek_acknowledgement_releases_saved_progress() {
    let (mut host, mut snapshot, id) = note_host();
    host.finish_note_open(request(&host, &id, false, false, false), &snapshot)
        .unwrap();
    let earlier = host.pending_note_seek.unwrap();
    host.finish_note_open(request(&host, &id, false, false, false), &snapshot)
        .unwrap();
    let latest = host.pending_note_seek.unwrap();
    snapshot.completed_note_seek = Some(earlier);
    host.acknowledge_note_seek(&snapshot);
    assert!(!host.record_snapshot_progress(&snapshot, 10));
    snapshot.completed_note_seek = Some(latest);
    snapshot.load_token = Some(6);
    host.acknowledge_note_seek(&snapshot);
    assert!(host.hold_progress);
    snapshot.load_token = Some(7);
    snapshot.state = PlaybackState::Unavailable("seek failed".into());
    host.acknowledge_note_seek(&snapshot);
    assert!(host.hold_progress);
    snapshot.state = PlaybackState::Paused;
    snapshot.position_ms = 16_000;
    host.acknowledge_note_seek(&snapshot);
    assert!(host.record_snapshot_progress(&snapshot, 11));
    assert_eq!(
        host.model.progress[&ItemId("episode".into())].position_ms,
        16_000
    );
}

#[test]
fn refused_replacement_and_failed_runtime_dispatch_keep_the_progress_hold() {
    let (mut host, snapshot, id) = note_host();
    let commands = host
        .finish_note_open(request(&host, &id, false, false, false), &snapshot)
        .unwrap();
    let accepted = host.pending_note_seek;
    let changed = PlaybackSnapshot {
        representation: Some(receipt('b')),
        ..snapshot.clone()
    };
    assert!(
        host.finish_note_open(request(&host, &id, false, false, false), &changed)
            .is_err()
    );
    assert_eq!(host.pending_note_seek, accepted);
    assert!(
        !host.send_note_commands(&id, commands),
        "silent host has no runtime to accept the seek"
    );
    host.command(&ItemId("episode".into()), CompactCommand::Pause, 12);
    assert!(host.pending_note_seek.is_none());
    assert!(host.hold_progress);
    assert!(!host.record_snapshot_progress(&snapshot, 12));
}

#[cfg(feature = "audio")]
#[test]
fn new_transport_supersedes_an_unacknowledged_note_seek_when_delivery_succeeds() {
    let (mut host, snapshot, id) = note_host();
    host.finish_note_open(request(&host, &id, false, false, false), &snapshot)
        .unwrap();
    assert!(host.pending_note_seek.is_some());
    assert!(host.hold_progress);
    // An empty runtime accepts Pause without opening a decoder or device.
    assert!(
        host.runtime
            .set(PlaybackRuntime::start_with(Arc::new(Inert)))
            .is_ok()
    );
    host.command(&ItemId("episode".into()), CompactCommand::Pause, 13);
    assert!(host.pending_note_seek.is_none());
    assert!(!host.hold_progress);
}

#[test]
fn missing_replacement_note_cancels_the_old_deferred_open_and_span() {
    let (mut host, mut snapshot, id) = note_host();
    snapshot.state = PlaybackState::Loading;
    host.note_commands(id, false, false, true, &snapshot)
        .unwrap();
    assert!(host.pending_note_open.is_some());
    host.span_stop_ms = Some(19_000);
    assert!(
        host.note_commands(
            AnnotationId("deleted-note".into()),
            false,
            false,
            false,
            &snapshot
        )
        .is_err()
    );
    assert!(host.pending_note_open.is_none());
    assert!(host.span_stop_ms.is_none());
    assert!(host.hold_progress);
}

#[test]
fn capturing_cancels_a_pending_open_and_compact_projection_keeps_its_warning() {
    let (mut host, mut snapshot, id) = note_host();
    snapshot.state = PlaybackState::Loading;
    host.note_commands(id.clone(), false, false, false, &snapshot)
        .unwrap();
    assert!(host.pending_note_open.is_some());
    let item = ItemId("episode".into());
    host.command(&item, CompactCommand::BeginTextNote, 1);
    assert!(host.pending_note_open.is_none());
    host.note_warning = Some(NoteOpenWarning {
        id: id.clone(),
        message: "Couldn't verify this copy.".into(),
    });
    assert_eq!(host.project(&item).note_warning.unwrap().id, id);
    assert!(host.project(&ItemId("other".into())).note_warning.is_none());
}

#[test]
fn aligned_points_require_the_loaded_digest_and_never_remap_span_ends() {
    let (mut host, mut snapshot, id) = note_host();
    let original = host.model.annotations[&id].target.clone();
    let derived = redshank_model::DerivedPosition {
        original_target: original.clone(),
        destination: receipt('b'),
        offset_ms: 46_000,
        algorithm_version: 1,
        confidence_per_mille: 990,
        runner_up_per_mille: 500,
        reference_duration_ms: 5_000,
    };
    host.model.store_derived_position(&id, derived).unwrap();
    snapshot.representation = Some(receipt('b'));
    // A saved estimate never changes the meaning of plain Open at.
    assert_eq!(
        host.finish_note_open(request(&host, &id, false, false, false), &snapshot)
            .unwrap_err(),
        "This copy differs."
    );
    assert!(host.pending_note_seek.is_none());
    let commands = host
        .finish_note_open(request(&host, &id, false, true, false), &snapshot)
        .unwrap();
    assert!(matches!(
        &commands[0],
        PlaybackCommand::SeekNote {
            position_ms: 46_000,
            ..
        }
    ));
    assert_eq!(host.model.annotations[&id].target, original);
    assert!(
        host.finish_note_open(request(&host, &id, false, false, true), &snapshot)
            .is_err()
    );
    snapshot.representation = Some(receipt('c'));
    assert!(
        host.finish_note_open(request(&host, &id, false, true, false), &snapshot)
            .is_err()
    );
    snapshot.representation = None;
    assert!(
        host.finish_note_open(request(&host, &id, false, true, false), &snapshot)
            .is_err()
    );
}

/// A handle that is never called: what the test needs is its identity.
struct Inert;

impl Fetch for Inert {
    fn read_range(
        &self,
        _: &str,
        _: fetch::Range,
        _: Option<&str>,
    ) -> Result<fetch::RangeReply, fetch::FetchError> {
        unreachable!("nothing fetches in this test")
    }
    fn read_all(&self, _: &str, _: Option<&str>, _: u64) -> Result<fetch::Body, fetch::FetchError> {
        unreachable!("nothing fetches in this test")
    }
    fn read_into(
        &self,
        _: &str,
        _: &mut dyn std::io::Write,
        _: Option<u64>,
    ) -> Result<fetch::Facts, fetch::FetchError> {
        unreachable!("nothing fetches in this test")
    }
}

#[test]
fn the_shells_fetch_handle_survives_a_session_reopen() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let handle: Arc<dyn Fetch> = Arc::new(Inert);

    let host = RedshankHost::open(first.path(), Output::Silent, handle.clone());
    assert!(Arc::ptr_eq(&host.fetch().unwrap(), &handle));
    let reopened = host.reopen(second.path());
    assert!(
        Arc::ptr_eq(&reopened.fetch().unwrap(), &handle),
        "a reopened session must stream through the same handle, not a new one"
    );

    // A host that never had a handle reopens its model and starts nothing.
    let bare = RedshankHost::default().reopen(second.path());
    assert!(bare.fetch().is_none());
    assert!(bare.runtime().is_none());
}
#[test]
fn suspended_host_keeps_output_and_fetch_without_opening_storage() {
    let handle: Arc<dyn Fetch> = Arc::new(Inert);
    let configured = RedshankHost::detached(Output::Device, handle.clone());
    let suspended = configured.suspended();
    assert_eq!(suspended.output(), Output::Device);
    assert!(Arc::ptr_eq(&suspended.fetch().unwrap(), &handle));
    assert!(suspended.runtime().is_none());
    assert!(suspended.store.is_none());
    // Verify the same reopen seam without opening a physical audio device.
    let silent = RedshankHost::detached(Output::Silent, handle.clone()).suspended();
    let root = tempfile::tempdir().unwrap();
    let reopened = silent.reopen(root.path());
    assert_eq!(reopened.output(), Output::Silent);
    assert!(Arc::ptr_eq(&reopened.fetch().unwrap(), &handle));
    assert!(reopened.runtime().is_none());
}
