// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Download custody at the product edge.
//!
//! The network response keeps its source URL as graph identity. Exact bytes
//! enter the session's Muniment store, while the ordinary filesystem copy is
//! a convenience destination recorded as metadata.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use armillary::{ActorHandle, Emitter, Wake, spawn_named};

use crate::action::{FetchedPage, StoredDownload, Update};

pub(crate) const REPRESENTATIONS_FILE: &str = "representations.redb";

/// One completed network response handed from the shell to the custody lane.
/// Paths are captured at command time so a session switch cannot redirect an
/// in-flight write into the newly active session.
pub(crate) struct DownloadCommand {
    pub node: uuid::Uuid,
    pub url: String,
    pub content_type: Option<String>,
    pub content_disposition: Option<String>,
    pub received_at_ms: u64,
    pub session_dir: PathBuf,
    pub download_dir: PathBuf,
    pub bytes: Vec<u8>,
}

/// Spawn the serialized custody writer. Blob-store and filesystem work stays
/// off the event-loop thread; every command answers with one app-owned update.
pub(crate) fn spawn_downloads(wake: Wake) -> (ActorHandle<DownloadCommand>, Receiver<Update>) {
    spawn_named(
        "download-custody",
        wake,
        move |commands: Receiver<DownloadCommand>, out: Emitter<Update>| {
            while let Ok(command) = commands.recv() {
                let byte_size = command.bytes.len() as u64;
                let result = store(
                    &command.session_dir,
                    &command.download_dir,
                    &command.url,
                    command.content_disposition.as_deref(),
                    &command.bytes,
                );
                out.emit(Update::DownloadStored {
                    node: command.node,
                    url: command.url,
                    content_type: command.content_type,
                    content_disposition: command.content_disposition,
                    received_at_ms: command.received_at_ms,
                    byte_size,
                    result,
                });
            }
        },
    )
}

/// Explicit attachments and response types Turnstone cannot render become
/// downloads. A missing media type retains the existing render attempt.
pub(crate) fn is_download_response(fetched: &FetchedPage) -> bool {
    let attachment = fetched.content_disposition.as_deref().is_some_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("attachment"))
    });
    attachment
        || fetched.content_type.as_deref().is_some_and(|value| {
            let media = value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            !(media.starts_with("text/")
                || media == "application/xml"
                || media == "application/xhtml+xml"
                || media.ends_with("+xml")
                || media == "application/gopher-menu")
        })
}

/// Resolve the display/destination name without letting response metadata
/// escape the configured directory.
pub(crate) fn suggested_filename(url: &str, content_disposition: Option<&str>) -> String {
    content_disposition
        .and_then(disposition_filename)
        .or_else(|| {
            url::Url::parse(url).ok().and_then(|parsed| {
                parsed
                    .path_segments()
                    .and_then(|mut segments| segments.next_back())
                    .filter(|segment| !segment.is_empty())
                    .map(percent_decode)
            })
        })
        .map(|name| safe_filename(&name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "download".to_string())
}

fn disposition_filename(value: &str) -> Option<String> {
    let mut plain = None;
    for parameter in value.split(';').skip(1) {
        let Some((name, raw)) = parameter.trim().split_once('=') else {
            continue;
        };
        let raw = raw.trim().trim_matches('"');
        if name.trim().eq_ignore_ascii_case("filename*") {
            let encoded = raw.splitn(3, '\'').nth(2).unwrap_or(raw);
            return Some(percent_decode(encoded));
        }
        if name.trim().eq_ignore_ascii_case("filename") {
            plain = Some(raw.to_string());
        }
    }
    plain
}

fn percent_decode(value: &str) -> String {
    let source = value.as_bytes();
    let mut decoded = Vec::with_capacity(source.len());
    let mut index = 0;
    while index < source.len() {
        if source[index] == b'%'
            && index + 2 < source.len()
            && let (Some(high), Some(low)) =
                (hex_digit(source[index + 1]), hex_digit(source[index + 2]))
        {
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(source[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn safe_filename(value: &str) -> String {
    let leaf = Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(value);
    let mut safe = leaf
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            {
                '_'
            } else {
                ch
            }
        })
        .collect::<String>();
    safe = safe.trim_matches([' ', '.']).to_string();
    let stem = Path::new(&safe)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    if matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        safe.insert(0, '_');
    }
    safe
}

pub(crate) fn configured_download_dir(data_root: &Path) -> PathBuf {
    std::env::var_os("TURNSTONE_DOWNLOAD_DIR")
        .map(PathBuf::from)
        .or_else(dirs::download_dir)
        .unwrap_or_else(|| data_root.join("downloads"))
}

/// Deposit bytes and create one collision-free user-visible copy.
pub(crate) fn store(
    session_dir: &Path,
    download_dir: &Path,
    url: &str,
    content_disposition: Option<&str>,
    bytes: &[u8],
) -> Result<StoredDownload, String> {
    fs::create_dir_all(session_dir)
        .map_err(|error| format!("could not create the session directory: {error}"))?;
    let backend = muniment::RedbBackend::open(session_dir.join(REPRESENTATIONS_FILE))
        .map_err(|error| format!("could not open the representation store: {error}"))?;
    let blobs = muniment::BlobStore::new(backend);
    let content = pollster::block_on(blobs.put(bytes))
        .map_err(|error| format!("could not deposit the representation: {error}"))?;

    fs::create_dir_all(download_dir)
        .map_err(|error| format!("could not create the download directory: {error}"))?;
    let name = suggested_filename(url, content_disposition);
    let (destination, mut file) = create_destination(download_dir, &name)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("could not write {}: {error}", destination.display()))?;

    Ok(StoredDownload {
        content,
        destination: destination.to_string_lossy().into_owned(),
        byte_size: bytes.len() as u64,
    })
}

fn create_destination(directory: &Path, name: &str) -> Result<(PathBuf, File), String> {
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("download");
    let extension = path.extension().and_then(|value| value.to_str());
    for suffix in 0..10_000u32 {
        let candidate_name = if suffix == 0 {
            name.to_string()
        } else if let Some(extension) = extension {
            format!("{stem} ({suffix}).{extension}")
        } else {
            format!("{stem} ({suffix})")
        };
        let candidate = directory.join(candidate_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!("could not create {}: {error}", candidate.display()));
            }
        }
    }
    Err(format!("could not allocate a destination for {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_and_unrenderable_media_download_but_text_stays_a_document() {
        assert!(is_download_response(&FetchedPage {
            content_type: Some("text/plain".into()),
            content_disposition: Some("attachment; filename=notes.txt".into()),
            bytes: b"notes".to_vec(),
            body: "notes".into(),
        }));
        assert!(is_download_response(&FetchedPage {
            content_type: Some("application/octet-stream".into()),
            content_disposition: None,
            bytes: vec![0, 1],
            body: "\0\u{1}".into(),
        }));
        assert!(!is_download_response(&FetchedPage::text(
            Some("text/gemini; charset=utf-8".into()),
            "# Page",
        )));
    }

    #[test]
    fn filename_prefers_disposition_and_removes_path_authority() {
        assert_eq!(
            suggested_filename(
                "gemini://example.test/fallback.bin",
                Some("attachment; filename*=UTF-8''field%20notes.txt"),
            ),
            "field notes.txt"
        );
        assert_eq!(
            suggested_filename(
                "https://example.test/file.bin",
                Some("attachment; filename=../outside?.bin"),
            ),
            "outside_.bin"
        );
        assert_eq!(
            suggested_filename("https://example.test/a+b.bin", None),
            "a+b.bin"
        );
    }

    #[test]
    fn store_writes_a_collision_free_copy_and_retrievable_blob() {
        let root = std::env::temp_dir().join(format!(
            "turnstone-download-store-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let session = root.join("session");
        let downloads = root.join("downloads");
        let first = store(
            &session,
            &downloads,
            "gemini://example.test/report.bin",
            None,
            b"exact bytes",
        )
        .unwrap();
        let second = store(
            &session,
            &downloads,
            "gemini://example.test/report.bin",
            None,
            b"exact bytes",
        )
        .unwrap();
        assert_eq!(first.content, second.content);
        assert_ne!(first.destination, second.destination);
        assert_eq!(fs::read(&first.destination).unwrap(), b"exact bytes");
        let backend = muniment::RedbBackend::open(session.join(REPRESENTATIONS_FILE)).unwrap();
        let blobs = muniment::BlobStore::new(backend);
        assert_eq!(
            pollster::block_on(blobs.get(&first.content)).unwrap(),
            Some(b"exact bytes".to_vec())
        );
        fs::remove_dir_all(root).unwrap();
    }
}
