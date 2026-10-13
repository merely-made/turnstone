// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Host admission and local presentation of Fleece's selected-content draft.
//! Bytes live in the mere, independently of the session graph's clip reference.
//! The shell checks destination authority before opening this local store.

use eidetic::clip_profile::ClipSourceV1;
use eidetic::{Hash, ManifestId, ProvenanceRecord, Timestamp, TrustEnvelope};
use fleece::portable_clip::{CaptureContext, ClipLimits, ClipPackageV1};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub use mere_document_lanes::retained_clip::{
    ENGINE_ID, ClipBody, ClipRequest, ClipSessionEngine, ClipView, address, request,
};

fn read_store(store: &mut dyn eidetic::Store, id: ManifestId, policy: ClipLimits) -> Result<ClipView, String> {
    pollster::block_on(mere_document_lanes::retained_clip::read_store(store, None, id, policy))
}
#[cfg(test)]
mod tests;
const STORE_DIR: &str = "retained-clips";

/// Prototype settings are explicit environment overrides. Invalid settings
/// refuse the operation rather than widening its budget.
pub fn limits() -> Result<ClipLimits, String> {
    fn setting(name: &str, default: u64) -> Result<u64, String> {
        match std::env::var(name) {
            Ok(value) => value
                .parse::<u64>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| format!("Invalid {name}")),
            Err(std::env::VarError::NotPresent) => Ok(default),
            Err(_) => Err(format!("Invalid {name}")),
        }
    }
    Ok(ClipLimits {
        max_parts: usize::try_from(setting("TURNSTONE_CLIP_MAX_PARTS", 64)?)
            .map_err(|e| e.to_string())?,
        max_bytes: setting("TURNSTONE_CLIP_MAX_BYTES", 16 * 1024 * 1024)?,
        max_record_bytes: usize::try_from(setting("TURNSTONE_CLIP_MAX_RECORD_BYTES", 1024 * 1024)?)
            .map_err(|e| e.to_string())?,
    })
}

fn open(root: &Path, create: bool) -> Result<muniment::FjallStore, String> {
    let path = root.join(STORE_DIR);
    if !create && !path.is_dir() {
        return Err("This mere has no retained clip store".into());
    }
    muniment::FjallStore::open(path).map_err(|e| e.to_string())
}

pub fn import(
    root: &Path,
    file: &Path,
    policy: ClipLimits,
    now_ms: u64,
) -> Result<ManifestId, String> {
    let wire_limit = policy
        .max_bytes
        // Pretty JSON can use indentation and decimal digits for each byte.
        // Bound wire expansion separately from the decoded inventory budget.
        .checked_mul(16)
        .and_then(|n| n.checked_add(policy.max_record_bytes as u64))
        .ok_or("Clip wire budget overflow")?;
    let mut bytes = Vec::new();
    File::open(file)
        .map_err(|e| e.to_string())?
        .take(
            wire_limit
                .checked_add(1)
                .ok_or("Clip wire budget overflow")?,
        )
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > wire_limit {
        return Err("Clip package exceeds the configured wire budget".into());
    }
    let package: ClipPackageV1 =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid clip package: {e}"))?;
    // Verify and lower in memory first: a refused file creates no disk store.
    let context = CaptureContext {
        source: ClipSourceV1 {
            owner: Some("turnstone".into()),
            entity: "selected-package-import".into(),
            revision: None,
        },
        capture_id: uuid::Uuid::new_v4().to_string(),
        provenance: ProvenanceRecord::self_imported("explicit selected clip package"),
        trust: TrustEnvelope::self_asserted(),
        captured_at: Timestamp(now_ms),
    };
    let mut staging = eidetic::MemoryBackend::new();
    let id = pollster::block_on(fleece::portable_clip::import_clip_package(
        &mut staging,
        None,
        &package,
        &context,
        policy,
    ))
    .map_err(|e| e.to_string())?;
    // Validate the receiving capability before durable admission.
    read_store(&mut staging, id, policy)?;
    let mut store = open(root, true)?;
    let received = pollster::block_on(fleece::portable_clip::import_clip_package(
        &mut store, None, &package, &context, policy,
    ))
    .map_err(|e| e.to_string())?;
    // Fjall's normal handle drop owns close/reopen durability; this function
    // never reports application graph/session admission as a store transaction.
    Ok(received)
}


pub fn read(root: &Path, id: ManifestId, policy: ClipLimits) -> Result<ClipView, String> {
    read_store(&mut open(root, false)?, id, policy)
}

/// Explicit selected-body disclosure. Fleece validates the held inventory and
/// hashes with NoFetcher; custody manifests and private backing stay local.
/// Finish validation before creating output, then atomically publish the file.
pub fn export(
    root: &Path,
    id: ManifestId,
    file: &Path,
    policy: ClipLimits,
    replace: bool,
) -> Result<(), String> {
    let package = pollster::block_on(fleece::portable_clip::export_selected_clip(
        &mut open(root, false)?,
        None,
        id,
        &["body".into()],
        policy,
    ))
    .map_err(|e| e.to_string())?;
    let parent = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    serde_json::to_writer(temporary.as_file_mut(), &package).map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    if replace {
        temporary.persist(file).map_err(|e| e.to_string())?;
    } else {
        temporary
            .persist_noclobber(file)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}


impl crate::app::App {
    pub(crate) fn clip_export_target(&self) -> Result<ManifestId, String> {
        self.clip_destination()?;
        let (_, url) = self
            .browser_command_target()
            .ok_or("No retained clip is selected")?;
        match request(&url)? {
            Some(ClipRequest::Read(id)) => Ok(id),
            _ => Err("The current content is not a retained clip".into()),
        }
    }

    pub(crate) fn choose_retained_clip_export(&mut self) -> Vec<crate::action::Effect> {
        match self.clip_export_target() {
            Ok(clip) => vec![crate::action::Effect::ChooseClipExportFile { clip }],
            Err(error) => {
                self.note_retained_clip_refusal(error);
                vec![crate::action::Effect::Redraw]
            },
        }
    }

    pub(crate) fn clip_destination(&self) -> Result<PathBuf, String> {
        if self.identity.is_none() {
            return Err(crate::identity::PENDING.into());
        }
        self.graph_runtimes
            .active_persistence()
            .placement_for(&self.session_dir())
            .map_err(|e| e.to_string())?;
        Ok(self.data_root.clone())
    }

    pub(crate) fn retained_clip_control(
        &mut self,
        address: &str,
    ) -> Option<Vec<crate::action::Effect>> {
        use crate::action::Effect;
        match request(address) {
            Ok(Some(control @ (ClipRequest::Play(_) | ClipRequest::Stop(_)))) => {
                Some(vec![Effect::ControlRetainedClip(control), Effect::Redraw])
            },
            Ok(Some(ClipRequest::Export(clip))) => match self.clip_destination() {
                Ok(_) => Some(vec![Effect::ChooseClipExportFile { clip }, Effect::Redraw]),
                Err(error) => {
                    self.note_retained_clip_refusal(error);
                    Some(vec![Effect::Redraw])
                },
            },
            Err(error) => {
                self.note_retained_clip_refusal(error);
                Some(vec![Effect::Redraw])
            },
            _ => None,
        }
    }
}
