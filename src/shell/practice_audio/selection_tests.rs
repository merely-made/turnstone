// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::super::{PracticeAudioEngine, PracticeSession, connection};
use super::*;
use inker::{SessionEngine, SessionSpawnRequest};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
    time::{Duration, Instant},
};

#[test]
fn ontology_matched_selection_sends_exact_frames_and_source_over_the_existing_handoff() {
    const NOTE: &str = "urn:example:lesson:turnaround";
    const SOURCE: &str = "https://example.invalid/archive/chord-study";
    let directory = tempfile::tempdir().unwrap();
    let recording = directory.path().join("my personal copy.wav");
    let rate = 44_100;
    let mut wav = hound::WavWriter::create(
        &recording,
        hound::WavSpec {
            channels: 2,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
            sample_rate: rate,
        },
    )
    .unwrap();
    for frame in 0..1100_i16 {
        wav.write_sample(frame).unwrap();
        wav.write_sample(2 * frame).unwrap();
    }
    wav.finalize().unwrap();

    let note_file = directory.path().join("lessons.jsonld");
    fs::write(&note_file, json!({
        "@context":"http://www.w3.org/ns/anno.jsonld","type":"Annotation","id":NOTE,
        "target":{"source":SOURCE,"type":"SpecificResource","selector":{
            "type":"FragmentSelector","conformsTo":"http://www.w3.org/TR/media-frags/","value":"t=0.009,0.021"}}
    }).to_string()).unwrap();
    let address = url::Url::from_file_path(recording).unwrap();
    let mut document = PracticeAudioEngine
        .spawn(&SessionSpawnRequest::new(address.as_str()))
        .unwrap();
    let session = document.as_any().downcast_mut::<PracticeSession>().unwrap();
    session
        .runner
        .update(|state| state.selections.load_file(note_file).unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    while session.runner.state().selections.busy() {
        assert!(Instant::now() < deadline);
        session.runner.update(State::tick);
        std::thread::yield_now();
    }
    assert!(session.runner.state().selections.matched.is_none());
    let target = taproot::Selector::class("practice-button")
        .containing("Match this recording to this source");
    let buttons = taproot::matching_estimated_fixture(&*session.dom.borrow(), &target);
    assert_eq!(buttons.len(), 1);
    session
        .runner
        .dispatch_click(buttons[0], cambium::PointerClick::at((0.0, 0.0)));
    session.runner.update(|state| {
        assert_eq!(state.selections.matched.as_ref().unwrap().note, NOTE);
        state.note = TextInput::new("Practice the change");
        state.end = TextInput::new("0.020");
        assert!(state.recording_import().is_err());
        state.end = TextInput::new("0.021");
    });
    let request = session.runner.state().recording_import().unwrap();
    let saved = directory.path().join("practice clip.wav");
    assert!(request.save(request.request.wav.path()).is_err());
    request.save(&saved).unwrap();
    let mut decoded = hound::WavReader::open(&saved).unwrap();
    let saved_samples = decoded
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let saved_info = info_fields(&fs::read(&saved).unwrap());
    assert_eq!(saved_info["ISRC"], format!("{SOURCE}#t=0.009,0.021"));
    assert_eq!(saved_info["ISBJ"], NOTE);
    assert_eq!(saved_info["ICMT"], "Practice the change");
    let command = connection::Command::Send(request);

    // A protocol recorder, so the test observes the exact file while the actual
    // client awaits acknowledgment. The receiver and any audio device are independent.
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = directory.path().join("endpoint.json");
    fs::write(&endpoint, json!({"version":1,"port":listener.local_addr().unwrap().port(),"pid":41,"token":"held-owner"}).to_string()).unwrap();
    let receipt = std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut socket = BufReader::new(socket);
        let acknowledge = br#"{"token":"held-owner","ok":true,"status":{"pid":41}}"#;
        let mut frame = String::new();
        socket.read_line(&mut frame).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&frame).unwrap()["command"]["action"],
            "status"
        );
        socket.get_mut().write_all(acknowledge).unwrap();
        socket.get_mut().write_all(b"\n").unwrap();
        frame.clear();
        socket.read_line(&mut frame).unwrap();
        let request: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(request["token"], "held-owner");
        let command = &request["command"];
        assert_eq!(command["action"], "import-page-phrase");
        let source = &command["source"];
        assert_eq!(source["source_page_url"], SOURCE);
        assert_eq!(source["note"], format!("Practice the change\n{NOTE}"));
        assert_eq!(source["start_ms"], 9);
        assert_eq!(source["end_ms"], 21);
        let clip = PathBuf::from(command["phrase_path"].as_str().unwrap());
        let mut wav = hound::WavReader::open(&clip).unwrap();
        assert_eq!((wav.spec().sample_rate, wav.spec().channels), (44_100, 2));
        let frames: Vec<i16> = wav.samples::<i16>().collect::<Result<_, _>>().unwrap();
        // floor(9 * 44.1) = 396; floor(21 * 44.1) = 926. Check every channel.
        let expected: Vec<_> = (396..926_i16).flat_map(|f| [f, 2 * f]).collect();
        assert_eq!(frames, expected);
        assert_eq!(frames, saved_samples);
        socket.get_mut().write_all(acknowledge).unwrap();
        socket.get_mut().write_all(b"\n").unwrap();
        clip
    });
    let mut owner = crate::woodshed_client::connect_existing(&endpoint).unwrap();
    assert!(connection::execute(&mut owner, None, command).is_ok_and(|reply| reply.imported));
    assert!(!receipt.join().unwrap().exists());
    if let Some(root) = std::env::var_os("TURNSTONE_CAPTURE_DIR") {
        let mut capture = tempfile::Builder::new()
            .prefix("matched-passage-")
            .suffix(".wav")
            .tempfile_in(root)
            .unwrap();
        std::io::copy(&mut fs::File::open(&saved).unwrap(), &mut capture).unwrap();
        println!(
            "Retained verified WAV at {}",
            capture.keep().unwrap().1.display()
        );
    }
}

// Independently walk RIFF chunks, including the per-field NUL and pad.
fn info_fields(bytes: &[u8]) -> std::collections::BTreeMap<String, String> {
    fn chunks(mut bytes: &[u8]) -> Vec<(&[u8], &[u8])> {
        let mut chunks = Vec::new();
        while !bytes.is_empty() {
            let len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
            chunks.push((&bytes[..4], &bytes[8..8 + len]));
            bytes = &bytes[8 + len + len % 2..];
        }
        chunks
    }
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
        bytes.len()
    );
    let info = chunks(&bytes[12..])
        .into_iter()
        .find(|(id, chunk)| *id == b"LIST" && chunk.starts_with(b"INFO"))
        .unwrap()
        .1;
    chunks(&info[4..])
        .into_iter()
        .map(|(id, field)| {
            (
                String::from_utf8(id.to_vec()).unwrap(),
                String::from_utf8(field.strip_suffix(&[0]).unwrap().to_vec()).unwrap(),
            )
        })
        .collect()
}
