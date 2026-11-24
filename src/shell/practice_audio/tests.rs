// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use document_session_api::{SessionInput, SessionPointerButton};

#[test]
fn closing_a_prepared_document_releases_its_decoded_recording() {
    let recording = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scenarios/fixtures/practice_audio/tone.mp3");
    let address = url::Url::from_file_path(recording).unwrap();
    let mut document = PracticeAudioEngine
        .spawn(&SessionSpawnRequest::new(address.as_str()))
        .unwrap();
    let session = document.as_any().downcast_mut::<PracticeSession>().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while session.runner.state().preparing.is_some() {
        assert!(std::time::Instant::now() < deadline);
        session.pump(0.0);
        std::thread::yield_now();
    }
    let preview = session
        .runner
        .state()
        .prepared
        .as_ref()
        .unwrap()
        .to_path_buf();
    assert!(preview.is_file());
    drop(document);
    assert!(
        !preview.exists(),
        "Closing the pane must release its decoded preview"
    );
}

#[test]
fn compressed_native_file_routes_preserve_names_and_block_document_navigation() {
    let dir = tempfile::tempdir().unwrap();
    for ext in ["MP3", "mP4"] {
        let path = dir.path().join(format!("take Ω #1%.{ext}"));
        let address = normalize_recording_path(path.to_str().unwrap()).unwrap();
        assert_eq!(local_recording(&address).unwrap(), path);
        assert_eq!(normalize_recording_path(&address).unwrap(), address);
        assert!(blocks_content_navigation(&address));
    }
    for address in [
        "https://example.test/take.mp3",
        "file://remote/take.mp4",
        "file:///take.mp3?remote=1",
    ] {
        assert!(local_recording(address).is_none());
        assert!(normalize_recording_path(address).is_none());
    }
}

#[test]
fn empty_note_accepts_ime_after_native_pointer_click_and_pump() {
    let wav = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    let mut writer = hound::WavWriter::create(
        wav.path(),
        hound::WavSpec {
            channels: 1,
            sample_rate: 8_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    writer.write_sample(0_i16).unwrap();
    writer.finalize().unwrap();
    let url = normalize_recording_path(wav.path().to_str().unwrap()).unwrap();
    let mut document = PracticeAudioEngine
        .spawn(&SessionSpawnRequest::new(url).with_viewport(512, 202))
        .unwrap();
    let session = document.as_any().downcast_mut::<PracticeSession>().unwrap();
    session.frame(512, 202);
    let (x, y) = {
        let dom = session.dom.borrow();
        let field = session
            .runner
            .focusables()
            .into_iter()
            .find(|&node| {
                dom.attribute(node, &Namespace::default(), &LocalName::from("aria-label"))
                    == Some("Practice note")
            })
            .expect("practice note control registers focus");
        let (left, top, width, height) = session
            .layout
            .visible_rect(&dom, field)
            .expect("empty practice note has visible bounds");
        (left + width / 2.0, top + height / 2.0)
    };
    session.input(SessionInput::Focus(true));
    for state in [SessionButtonState::Pressed, SessionButtonState::Released] {
        session.input(SessionInput::PointerButton {
            x,
            y,
            button: SessionPointerButton::Primary,
            state,
            modifiers: SessionModifiers::default(),
        });
    }
    session.pump(1.0);
    assert!(
        session.editable_focus(),
        "textbox focus survives release and pump"
    );
    let note = "Hear the chord change. Ω";
    assert_eq!(
        session
            .input(SessionInput::Ime(SessionIme::Commit(note.into())))
            .effect,
        SessionEffect::Handled
    );
    session.pump(2.0);
    assert_eq!(session.runner.state().note.text(), note);
    session.frame(320, 110);
    assert!(!session.scroll_by(0.0, -60.0), "initial pane is at top");
    assert!(
        session.scroll_by(0.0, 60.0),
        "positive wheel reveals lower controls"
    );
    assert!(session.scroll.offset().1 > 0.0);
}
