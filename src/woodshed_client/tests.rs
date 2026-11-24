// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0


use super::*;

#[test]
fn local_recording_selection_accepts_only_absolute_supported_files() {
    let directory = tempfile::tempdir().unwrap();
    let wav = directory.path().join("practice.wav");
    fs::write(&wav, b"RIFF").unwrap();

    let selected = LocalWavSelection::from_native_file(&wav).unwrap();
    assert!(selected.path().is_absolute());
    assert_eq!(selected.path(), fs::canonicalize(&wav).unwrap());

    for name in ["practice.MP3", "practice.Mp4"] {
        let recording = directory.path().join(name);
        fs::write(&recording, b"recording").unwrap();
        assert!(LocalWavSelection::from_native_recording(&recording).is_ok());
        assert!(LocalWavSelection::from_native_file(recording).is_err());
    }
    let text = directory.path().join("practice.txt");
    fs::write(&text, b"not audio").unwrap();
    assert!(matches!(
        LocalWavSelection::from_native_file(text),
        Err(ClientError::InvalidWav(_))
    ));
    assert!(matches!(
        LocalWavSelection::from_native_file("https://example.test/practice.wav"),
        Err(ClientError::InvalidWav(_))
    ));
}

#[test]
fn wav_import_wire_shape_uses_the_local_file_url_and_path() {
    let directory = tempfile::tempdir().unwrap();
    let wav = directory.path().join("practice loop.wav");
    fs::write(&wav, b"RIFF").unwrap();
    let selected = LocalWavSelection::from_native_file(&wav).unwrap();
    let path = selected.checked_path().unwrap();
    let command = WireCommand::ImportWav {
        source: WireWavSource {
            title: "Local practice".into(),
            source_page_url: url::Url::from_file_path(path).unwrap().to_string(),
            start_ms: 120,
            end_ms: 1_200,
            note: "first phrase".into(),
        },
        wav_path: path.to_path_buf(),
        preview_path: None,
    };
    let value = serde_json::to_value(command).unwrap();
    assert_eq!(value["action"], "import-wav");
    assert!(value.get("preview_path").is_none());
    assert_eq!(value["wav_path"], path.to_string_lossy().as_ref());
    assert_eq!(
        value["source"]["source_page_url"],
        url::Url::from_file_path(path).unwrap().as_str()
    );
    assert_eq!(value["source"]["start_ms"], 120);
    assert_eq!(value["source"]["end_ms"], 1_200);
}

#[test]
fn loopback_frames_can_be_read_repeatedly_on_one_connection() {
    let frames = b"{\"token\":\"owner\",\"ok\":true,\"error\":null,\"status\":{\"view\":\"pane\"}}\n{\"token\":\"owner\",\"ok\":true,\"error\":null,\"status\":{\"view\":\"window\"}}\n";
    let mut reader = BufReader::new(&frames[..]);
    let first: WireResponse = read_frame(&mut reader).unwrap();
    let second: WireResponse = read_frame(&mut reader).unwrap();
    assert_eq!(first.status.unwrap()["view"], "pane");
    assert_eq!(second.status.unwrap()["view"], "window");
}

#[test]
fn development_discovery_matches_the_sibling_workspace_profile() {
    let directory = tempfile::tempdir().unwrap();
    let common = directory.path();
    let turnstone = common.join("turnstone/debug/turnstone.exe");
    let woodshed = common.join("woodshed/debug").join(executable_name());
    fs::create_dir_all(turnstone.parent().unwrap()).unwrap();
    fs::create_dir_all(woodshed.parent().unwrap()).unwrap();
    fs::write(&turnstone, b"turnstone").unwrap();
    fs::write(&woodshed, b"woodshed").unwrap();

    assert_eq!(development_sibling_executable(&turnstone), Some(woodshed));
    assert!(development_sibling_executable(&common.join("turnstone\nonsense.exe")).is_none());
}

#[test]
fn default_handoff_root_uses_the_same_project_dirs_as_woodshed() {
    let expected = directories::ProjectDirs::from("dev", "Woodshed", "Woodshed")
        .unwrap()
        .config_dir()
        .to_path_buf();
    assert_eq!(default_data_root().unwrap(), expected);
}

#[test]
fn only_lost_or_invalid_stream_responses_invalidate_a_session() {
    assert!(ClientError::Protocol("closed".into()).invalidates_session());
    assert!(ClientError::IdentityChanged.invalidates_session());
    assert!(
        !ClientError::Remote {
            message: "practice is locked".into(),
            status: None,
        }
        .invalidates_session()
    );
    assert!(!ClientError::InvalidRequest("bad gain".into()).invalidates_session());
}
