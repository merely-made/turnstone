// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn opening_a_library_does_not_start_playback() {
    let directory = tempfile::tempdir().unwrap();
    let fetch = Arc::new(fetch::NetFetch::new(&fetch::Stores::in_memory()).unwrap());
    let host = RedshankHost::open(directory.path(), Output::Device, fetch);
    assert!(
        host.runtime().is_none(),
        "opening a session must not start the audio worker"
    );
}

#[test]
fn passive_audio_controls_and_reopen_keep_playback_dormant() {
    let directory = tempfile::tempdir().unwrap();
    let fetch = Arc::new(fetch::NetFetch::new(&fetch::Stores::in_memory()).unwrap());
    let mut host = RedshankHost::open(directory.path(), Output::Device, fetch);
    host.set_wake(Arc::new(|| {}));
    assert!(!host.send(PlaybackCommand::Pause));
    assert!(!host.send(PlaybackCommand::StopPreview));
    assert!(!host.record_progress(0));
    assert!(host.runtime().is_none());
    let reopened = host.reopen(directory.path());
    assert!(reopened.runtime().is_none());
    assert!(reopened.playback_wake.is_some());
    assert!(host.suspended().playback_wake.is_some());
}

#[cfg(not(feature = "audio"))]
#[test]
fn an_audio_free_build_refuses_first_play_without_starting_a_worker() {
    let directory = tempfile::tempdir().unwrap();
    let fetch = Arc::new(fetch::NetFetch::new(&fetch::Stores::in_memory()).unwrap());
    let mut host = RedshankHost::open(directory.path(), Output::Device, fetch);
    assert!(
        host.play_retained_voice_bytes("unavailable", vec![0; 16])
            .is_err()
    );
    assert!(host.runtime().is_none());
    assert!(host.retained_voice.lock().unwrap().is_none());
}

#[cfg(feature = "audio")]
#[test]
fn first_media_demand_starts_one_runtime_and_keeps_the_wake() {
    let directory = tempfile::tempdir().unwrap();
    let fetch = Arc::new(fetch::NetFetch::new(&fetch::Stores::in_memory()).unwrap());
    let mut host = RedshankHost::open(directory.path(), Output::Device, fetch);
    let wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = wakes.clone();
    host.set_wake(Arc::new(move || {
        seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    // Invalid bytes exercise worker admission/error delivery without opening a device.
    host.play_retained_voice_bytes("bad-audio", vec![0; 16])
        .unwrap();
    let first = host.runtime().unwrap() as *const PlaybackRuntime;
    assert_eq!(
        host.ensure_runtime().unwrap() as *const PlaybackRuntime,
        first
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while wakes.load(std::sync::atomic::Ordering::SeqCst) == 0
        && std::time::Instant::now() < deadline
    {
        std::thread::yield_now();
    }
    assert!(wakes.load(std::sync::atomic::Ordering::SeqCst) > 0);
}

#[cfg(feature = "audio")]
#[test]
fn volume_chosen_before_first_use_survives_reopen_and_reaches_the_backend() {
    let directory = tempfile::tempdir().unwrap();
    let fetch = Arc::new(fetch::NetFetch::new(&fetch::Stores::in_memory()).unwrap());
    let mut configured = RedshankHost::open(directory.path(), Output::Device, fetch);
    configured.command(
        &ItemId("not-loaded".into()),
        CompactCommand::SetVolume(37),
        0,
    );
    configured.command(
        &ItemId("not-loaded".into()),
        CompactCommand::SetRate(140),
        0,
    );
    assert!(configured.runtime().is_none());
    configured.save();
    let mut host = configured.reopen(directory.path());
    assert!(host.runtime().is_none());
    assert_eq!(host.model().settings.playback_rate_percent, 140);
    host.play_retained_voice_bytes("invalid-audio", vec![0; 16])
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while host.snapshot().volume_percent != 37 && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert_eq!(
        host.snapshot().volume_percent,
        37,
        "first media demand must apply the stored output settings"
    );
}

#[test]
fn unconfigured_output_is_unavailable_and_never_starts_a_worker() {
    let mut host = RedshankHost {
        output: Output::Device,
        ..Default::default()
    };
    let id = ItemId("unavailable".into());
    host.model
        .add_item(LibraryItem::DirectAudio {
            id: id.clone(),
            title: "Unavailable output".into(),
            source: MediaSource::Enclosure {
                url: "https://example.invalid/media.wav".into(),
            },
        })
        .unwrap();
    assert!(
        matches!(host.project(&id).transport, TransportState::Unavailable(_)),
        "a missing backend must not appear to buffer forever"
    );
    assert!(host.runtime().is_none());
}
