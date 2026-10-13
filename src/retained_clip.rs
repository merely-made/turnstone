// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Host admission and local presentation of Fleece's selected-content draft.
//! Bytes live in the mere, independently of the session graph's clip reference.
//! The shell checks destination authority before opening this local store.

use eidetic::clip_profile::{ClipSourceV1, PortableClipV1};
use eidetic::{Hash, ManifestId, ProvenanceRecord, Timestamp, TrustEnvelope};
use fleece::portable_clip::{CaptureContext, ClipLimits, ClipPackageV1};
use inker::{Block, EngineDocument, InlineSpan};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const ENGINE_ID: &str = "fleece.retained-clip";
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

pub fn address(id: ManifestId) -> String {
    format!("mere://clip/{}", id.0.to_hex())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipRequest {
    Read(ManifestId),
    Play(ManifestId),
    Stop(ManifestId),
    Export(ManifestId),
}

pub fn request(address: &str) -> Result<Option<ClipRequest>, String> {
    let Ok(url) = url::Url::parse(address) else {
        return Ok(None);
    };
    if url.scheme() != "mere" || url.host_str() != Some("clip") {
        return Ok(None);
    }
    if url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err("Invalid retained clip address".into());
    }
    let parts: Vec<_> = url
        .path_segments()
        .ok_or("Invalid retained clip address")?
        .collect();
    let Some(hex) = parts.first().filter(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }) else {
        return Err("Invalid retained clip identity".into());
    };
    let id = ManifestId::from_hash(Hash::parse(hex).map_err(|e| e.to_string())?);
    match parts.as_slice() {
        [_] => Ok(Some(ClipRequest::Read(id))),
        [_, "play"] => Ok(Some(ClipRequest::Play(id))),
        [_, "stop"] => Ok(Some(ClipRequest::Stop(id))),
        [_, "export"] => Ok(Some(ClipRequest::Export(id))),
        _ => Err("Unsupported retained clip action".into()),
    }
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

pub struct ClipView {
    pub id: ManifestId,
    pub clip: PortableClipV1,
    pub body: ClipBody,
}
pub enum ClipBody {
    Text(String),
    Voice(Vec<u8>),
}

fn read_store(
    store: &mut dyn eidetic::Store,
    id: ManifestId,
    policy: ClipLimits,
) -> Result<ClipView, String> {
    let package = pollster::block_on(fleece::portable_clip::export_selected_clip(
        store,
        None,
        id,
        &["body".into()],
        policy,
    ))
    .map_err(|e| e.to_string())?;
    if package.clip.parts.len() != 1 {
        return Err("This clip needs a receiving adapter for its additional parts".into());
    }
    let part = &package.clip.parts[0];
    let bytes = package
        .payloads
        .into_iter()
        .find(|p| p.id == part.manifest)
        .ok_or("Retained clip body is missing")?
        .bytes;
    let body = match part.media_type.as_str() {
        "text/plain" | "text/plain;charset=utf-8" | "text/plain; charset=utf-8" => {
            ClipBody::Text(String::from_utf8(bytes).map_err(|_| "Clip text is not UTF-8")?)
        },
        "audio/wav" | "audio/x-wav"
            if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") =>
        {
            ClipBody::Voice(bytes)
        },
        _ => {
            return Err(format!(
                "This host cannot present clip media {}",
                part.media_type
            ));
        },
    };
    Ok(ClipView {
        id,
        clip: package.clip,
        body,
    })
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

impl ClipView {
    pub fn document(&self) -> EngineDocument {
        let mut blocks = vec![Block::Heading {
            level: 1,
            spans: vec![InlineSpan::Text(self.clip.title.clone())],
        }];
        match &self.body {
            ClipBody::Text(body) => blocks.push(Block::Paragraph {
                spans: vec![InlineSpan::Text(body.clone())],
            }),
            ClipBody::Voice(_) => {
                for (suffix, label) in [("play", "Play voice note"), ("stop", "Stop voice note")] {
                    blocks.push(Block::Paragraph {
                        spans: vec![InlineSpan::Link {
                            url: format!("{}/{suffix}", address(self.id)),
                            title: None,
                            spans: vec![InlineSpan::Text(label.into())],
                            predicate: None,
                        }],
                    });
                }
                blocks.push(Block::MetadataRow {
                    label: "Voice note playback".into(),
                    value: "Ready".into(),
                });
            },
        }
        if self.clip.anchors.iter().any(|a| a.units == "milliseconds") {
            blocks.push(Block::Badge {
                text: "Source recording unavailable in this clip".into(),
            });
        }
        for anchor in &self.clip.anchors {
            blocks.push(Block::MetadataRow {
                label: format!("Source anchor ({})", anchor.units),
                value: anchor.source.clone(),
            });
            if anchor.units == "milliseconds" {
                if let Some(offset) = anchor.selector.get("offset_ms").and_then(|v| v.as_u64()) {
                    blocks.push(Block::MetadataRow {
                        label: "Original note position".into(),
                        value: format!("{}.{:03} seconds", offset / 1000, offset % 1000),
                    });
                }
            } else if anchor.units == "utf8-bytes" {
                if let Some(quote) = anchor.selector.get("exact_quote").and_then(|v| v.as_str()) {
                    blocks.push(Block::MetadataRow {
                        label: "Original selection".into(),
                        value: quote.into(),
                    });
                }
            } else {
                blocks.push(Block::Badge {
                    text: "Source selector preserved; this view does not interpret its format"
                        .into(),
                });
            }
        }
        for tag in &self.clip.tags {
            blocks.push(Block::MetadataRow {
                label: format!("Tag: {}", tag.label),
                value: format!(
                    "{}; {}",
                    tag.owner,
                    tag.asserter.as_deref().unwrap_or("author unknown")
                ),
            });
        }
        if let Some(tags) = self
            .clip
            .source_evidence
            .get("source_tags")
            .and_then(|v| v.as_array())
        {
            let labels: Vec<_> = tags.iter().filter_map(|v| v.as_str()).collect();
            if !labels.is_empty() {
                blocks.push(Block::MetadataRow {
                    label: "Source tags (author unknown)".into(),
                    value: labels.join(", "),
                });
            }
        }
        blocks.push(Block::Paragraph {
            spans: vec![InlineSpan::Link {
                url: format!("{}/export", address(self.id)),
                title: None,
                spans: vec![InlineSpan::Text("Export selected clip".into())],
                predicate: None,
            }],
        });
        EngineDocument {
            address: address(self.id),
            title: Some(self.clip.title.clone()),
            content_type: "application/vnd.mere.clip+json".into(),
            lang: None,
            provenance: inker::DocumentProvenance::for_engine(ENGINE_ID, &address(self.id)),
            trust: inker::DocumentTrustState::Unknown,
            diagnostics: vec![],
            navigation: Default::default(),
            blocks,
        }
    }

    /// A verified, rebuildable decoder cache. Durable authority stays with the
    /// retained bytes; incoming packages cannot choose a filesystem path.
    pub fn playback_path(&self, root: &Path) -> Result<PathBuf, String> {
        let ClipBody::Voice(bytes) = &self.body else {
            return Err("This clip has no voice-note body".into());
        };
        let folder = root.join(STORE_DIR).join("playback");
        let path = folder.join(format!("{}.wav", Hash::of(bytes).to_hex()));
        if path.exists() {
            let mut held = Vec::new();
            File::open(&path)
                .map_err(|e| e.to_string())?
                .take(bytes.len() as u64 + 1)
                .read_to_end(&mut held)
                .map_err(|e| e.to_string())?;
            if held != *bytes {
                return Err(
                    "Retained audio decoder cache differs from its content identity".into(),
                );
            }
        } else {
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            let mut temporary =
                tempfile::NamedTempFile::new_in(&folder).map_err(|e| e.to_string())?;
            temporary.write_all(bytes).map_err(|e| e.to_string())?;
            temporary.as_file().sync_all().map_err(|e| e.to_string())?;
            temporary
                .persist_noclobber(&path)
                .map_err(|e| e.to_string())?;
        }
        Ok(path)
    }
}

pub struct ClipSessionEngine;
impl inker::SessionEngine<netrender::Scene> for ClipSessionEngine {
    fn engine_id(&self) -> &str {
        ENGINE_ID
    }
    fn spawn(
        &self,
        request: &inker::SessionSpawnRequest,
    ) -> Result<Box<dyn inker::DocumentSession<netrender::Scene>>, inker::SessionError> {
        let body = request.body.as_deref().ok_or_else(|| {
            inker::SessionError::Unsupported(
                "A retained clip requires host-resolved local content".into(),
            )
        })?;
        let document: EngineDocument = serde_json::from_str(body)
            .map_err(|e| inker::SessionError::SpawnFailed(e.to_string()))?;
        if document.address != request.address
            || document.provenance.source_kind.as_deref() != Some(ENGINE_ID)
        {
            return Err(inker::SessionError::SpawnFailed(
                "Retained clip presentation identity differs".into(),
            ));
        }
        let doc = mere_document_lanes::SmolwebDocument::from_document_with_theme(
            document,
            mere_document_lanes::SmolwebTheme::System,
        );
        Ok(Box::new(mere_document_lanes::SmolwebDocumentSession::new(
            doc,
            request.viewport,
        )))
    }
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
