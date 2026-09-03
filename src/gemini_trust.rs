// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Durable Gemini server-certificate trust.
//!
//! The protocol layer performs the TLS comparison; this host-owned store keeps
//! only capsule targets and SHA-256 fingerprints under Turnstone's data root.
//! A changed certificate cannot replace its pin through [`fetch::SmolwebTofuStore`]:
//! replacement requires the explicit, stale-safe [`GeminiTrustStore::accept_change`]
//! path used by the shell after a human confirmation.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

const FORMAT_VERSION: u16 = 1;
const FILE_NAME: &str = "gemini_trust.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TrustFile {
    #[serde(default = "format_version")]
    version: u16,
    #[serde(default)]
    pins: BTreeMap<String, String>,
}

impl Default for TrustFile {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            pins: BTreeMap::new(),
        }
    }
}

fn format_version() -> u16 {
    FORMAT_VERSION
}

/// Process-wide trust state backed by one profile file.
#[derive(Debug)]
pub struct GeminiTrustStore {
    path: PathBuf,
    state: Mutex<TrustFile>,
}

impl GeminiTrustStore {
    pub fn load(data_root: &Path) -> io::Result<Self> {
        let path = data_root.join(FILE_NAME);
        let state = match std::fs::read(&path) {
            Ok(bytes) => decode_file(&bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let backup = backup_path(&path);
                match std::fs::read(&backup) {
                    Ok(bytes) => {
                        let state = decode_file(&bytes)?;
                        std::fs::rename(&backup, &path)?;
                        state
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => TrustFile::default(),
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        };
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    /// Replace one changed pin only if the prompt still names current trust.
    /// This prevents a stale decision from overwriting a pin changed elsewhere.
    pub fn accept_change(&self, target: &str, pinned: &str, seen: &str) -> io::Result<()> {
        let pinned_bytes = decode_fingerprint(pinned).map_err(io::Error::other)?;
        let seen_bytes = decode_fingerprint(seen).map_err(io::Error::other)?;
        let mut state = self.state.lock().unwrap();
        let current = state
            .pins
            .get(target)
            .and_then(|fingerprint| decode_fingerprint(fingerprint).ok());
        if current != Some(pinned_bytes) {
            return Err(io::Error::other(format!(
                "Gemini trust for {target} changed while the decision was open"
            )));
        }
        self.persist_pin(&mut state, target, seen_bytes)
    }

    fn persist_pin(
        &self,
        state: &mut TrustFile,
        target: &str,
        fingerprint: [u8; 32],
    ) -> io::Result<()> {
        let previous = state
            .pins
            .insert(target.to_string(), encode_fingerprint(&fingerprint));
        if let Err(error) = save_file(&self.path, state) {
            match previous {
                Some(previous) => {
                    state.pins.insert(target.to_string(), previous);
                }
                None => {
                    state.pins.remove(target);
                }
            }
            return Err(error);
        }
        Ok(())
    }
}

impl fetch::SmolwebTofuStore for GeminiTrustStore {
    fn fingerprint(&self, target: &str) -> Option<[u8; 32]> {
        self.state
            .lock()
            .unwrap()
            .pins
            .get(target)
            .and_then(|fingerprint| decode_fingerprint(fingerprint).ok())
    }

    fn pin(&self, target: &str, fingerprint: [u8; 32]) {
        if let Err(error) = self.try_pin(target, fingerprint) {
            tracing::error!(%error, target, "failed to persist Gemini trust pin");
        }
    }

    fn try_pin(&self, target: &str, fingerprint: [u8; 32]) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        self.persist_pin(&mut state, target, fingerprint)
            .map_err(|error| error.to_string())
    }
}

fn save_file(path: &Path, state: &TrustFile) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Gemini trust path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    let backup = backup_path(path);
    let bytes = serde_json::to_vec_pretty(state).map_err(io::Error::other)?;
    if let Err(error) = (|| -> io::Result<()> {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        if path.exists() {
            if backup.exists() {
                std::fs::remove_file(&backup)?;
            }
            std::fs::rename(path, &backup)?;
        }
        if let Err(error) = std::fs::rename(&temporary, path) {
            if backup.exists() {
                let _ = std::fs::rename(&backup, path);
            }
            return Err(error);
        }
        let _ = std::fs::remove_file(&backup);
        Ok(())
    })() {
        let _ = std::fs::remove_file(temporary);
        return Err(error);
    }
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn decode_file(bytes: &[u8]) -> io::Result<TrustFile> {
    let state: TrustFile = serde_json::from_slice(bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    if state.version != FORMAT_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported Gemini trust version {}", state.version),
        ));
    }
    for (target, fingerprint) in &state.pins {
        decode_fingerprint(fingerprint).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid Gemini trust pin for {target}: {error}"),
            )
        })?;
    }
    Ok(state)
}

fn encode_fingerprint(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_fingerprint(hex: &str) -> Result<[u8; 32], String> {
    if hex.len() != 64 || !hex.is_ascii() {
        return Err("fingerprint must be 64 hexadecimal characters".to_string());
    }
    let mut bytes = [0u8; 32];
    for (index, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk).map_err(|error| error.to_string())?;
        bytes[index] = u8::from_str_radix(text, 16)
            .map_err(|_| "fingerprint contains a non-hexadecimal character".to_string())?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fetch::SmolwebTofuStore;

    #[test]
    fn first_contact_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let trust = GeminiTrustStore::load(dir.path()).unwrap();
        trust.try_pin("capsule.example", [0x11; 32]).unwrap();
        drop(trust);

        let reopened = GeminiTrustStore::load(dir.path()).unwrap();
        assert_eq!(reopened.fingerprint("capsule.example"), Some([0x11; 32]));
        assert_eq!(reopened.fingerprint("capsule.example:1966"), None);
    }

    #[test]
    fn a_change_requires_the_current_pin_and_persists_the_decision() {
        let dir = tempfile::tempdir().unwrap();
        let trust = GeminiTrustStore::load(dir.path()).unwrap();
        trust.try_pin("capsule.example", [0x11; 32]).unwrap();
        assert!(
            trust
                .accept_change(
                    "capsule.example",
                    &encode_fingerprint(&[0x33; 32]),
                    &encode_fingerprint(&[0x22; 32]),
                )
                .is_err()
        );
        trust
            .accept_change(
                "capsule.example",
                &encode_fingerprint(&[0x11; 32]),
                &encode_fingerprint(&[0x22; 32]),
            )
            .unwrap();
        drop(trust);

        let reopened = GeminiTrustStore::load(dir.path()).unwrap();
        assert_eq!(reopened.fingerprint("capsule.example"), Some([0x22; 32]));
    }

    #[test]
    fn an_interrupted_replace_recovers_the_previous_pin() {
        let dir = tempfile::tempdir().unwrap();
        let trust = GeminiTrustStore::load(dir.path()).unwrap();
        trust.try_pin("capsule.example", [0x11; 32]).unwrap();
        drop(trust);

        let path = dir.path().join(FILE_NAME);
        let backup = backup_path(&path);
        std::fs::rename(&path, &backup).unwrap();
        std::fs::write(path.with_extension("json.tmp"), b"partial").unwrap();

        let recovered = GeminiTrustStore::load(dir.path()).unwrap();
        assert_eq!(recovered.fingerprint("capsule.example"), Some([0x11; 32]));
        assert!(path.exists());
        assert!(!backup.exists());
    }
}
