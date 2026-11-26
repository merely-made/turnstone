// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A project's `mod.toml`: what the folder names and what it declares.

use std::path::{Component, Path, PathBuf};

use document_host::mod_loader::{ModCapability, ModManifest, ModType};
use mod_host::{ModKind, normalize_media_type};
use serde::Deserialize;

pub(super) const MANIFEST: &str = "mod.toml";

fn version() -> String {
    "0.1.0".into()
}
fn component() -> String {
    "component.wasm".into()
}
fn document() -> String {
    "document.txt".into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Disk {
    mod_id: String,
    display_name: Option<String>,
    #[serde(default = "version")]
    version: String,
    #[serde(default = "component")]
    component: String,
    #[serde(default = "document")]
    document: String,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    provides: Vec<String>,
    #[serde(default)]
    document_script_origins: Vec<String>,
    /// `document` (the default), `parser` or `viewer` (mod projects MP7).
    #[serde(default)]
    kind: Option<String>,
    /// The media types a parser takes; the guest's `describe` must report the same set.
    #[serde(default)]
    media_types: Vec<String>,
}

pub(super) struct Spec {
    pub kind: ModKind,
    /// Normalized, sorted, unique; empty unless the project is a parser.
    pub media_types: Vec<String>,
    pub manifest: ModManifest,
    pub version: String,
    pub component: PathBuf,
    pub document: PathBuf,
}

pub(super) fn capability(name: &str) -> Result<ModCapability, String> {
    match name.trim().to_ascii_lowercase().as_str() {
        "network" => Ok(ModCapability::Network),
        "filesystem" | "fs" => Ok(ModCapability::Filesystem),
        "identity" => Ok(ModCapability::Identity),
        "clipboard" => Ok(ModCapability::Clipboard),
        "exec" => Ok(ModCapability::Exec),
        other => Err(format!("{MANIFEST}: unknown capability '{other}'")),
    }
}

/// A path the folder owns: relative, with no `..`.
fn inside(root: &Path, field: &str, name: &str) -> Result<PathBuf, String> {
    let path = Path::new(name);
    if path.components().all(|c| matches!(c, Component::Normal(_))) {
        Ok(root.join(path))
    } else {
        Err(format!("{MANIFEST}: {field} '{name}' leaves the project folder"))
    }
}

pub(super) fn read(root: &Path) -> Result<Spec, String> {
    let path = root.join(MANIFEST);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let disk: Disk = toml::from_str(&text).map_err(|e| format!("{MANIFEST}: {e}"))?;
    let kind = disk
        .kind
        .as_deref()
        .map_or(Ok(ModKind::Document), ModKind::parse)
        .map_err(|e| format!("{MANIFEST}: {e}"))?;
    let mut media_types = Vec::new();
    for raw in &disk.media_types {
        media_types.push(
            normalize_media_type(raw)
                .ok_or_else(|| format!("{MANIFEST}: '{raw}' is not a media type"))?,
        );
    }
    media_types.sort();
    media_types.dedup();
    match (kind, media_types.is_empty()) {
        (ModKind::Parser, true) => {
            return Err(format!("{MANIFEST}: a parser declares its media_types"));
        },
        (ModKind::Parser, false) => {},
        (_, false) => {
            return Err(format!("{MANIFEST}: media_types belong to a parser, not a {}", kind.as_str()));
        },
        _ => {},
    }
    let capabilities = disk
        .capabilities
        .iter()
        .map(|c| capability(c))
        .collect::<Result<Vec<_>, _>>()?;
    let mut manifest = ModManifest::new(
        disk.mod_id.clone(),
        disk.display_name.unwrap_or_else(|| disk.mod_id.clone()),
        ModType::Wasm,
        disk.provides,
        Vec::new(),
        capabilities,
    );
    manifest.document_script_origins = disk.document_script_origins;
    Ok(Spec {
        kind,
        media_types,
        manifest,
        version: disk.version,
        component: inside(root, "component", &disk.component)?,
        document: inside(root, "document", &disk.document)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_toml(text: &str) -> Result<Spec, String> {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(MANIFEST), text).unwrap();
        read(dir.path())
    }

    #[test]
    fn a_manifest_names_its_kind_and_a_parsers_media_types_are_normalized() {
        let doc = read_toml("mod_id = \"a\"\n").unwrap();
        assert_eq!((doc.kind, doc.media_types.len()), (ModKind::Document, 0));
        let parser = read_toml(
            "mod_id = \"p\"\nkind = \"parser\"\nmedia_types = [\"Text/X-One; charset=utf-8\", \"text/x-one\", \"text/x-two\"]\n",
        )
        .unwrap();
        assert_eq!(parser.kind, ModKind::Parser);
        assert_eq!(parser.media_types, ["text/x-one", "text/x-two"]);
        assert_eq!(read_toml("mod_id = \"v\"\nkind = \"viewer\"\n").unwrap().kind, ModKind::Viewer);
    }

    #[test]
    fn a_manifest_that_disagrees_with_its_kind_is_refused_by_name() {
        let refuse = |text: &str| read_toml(text).err().expect("refused");
        assert!(refuse("mod_id = \"p\"\nkind = \"parser\"\n").contains("declares its media_types"));
        assert!(refuse("mod_id = \"p\"\nkind = \"viewer\"\nmedia_types = [\"a/b\"]\n").contains("belong to a parser"));
        assert!(refuse("mod_id = \"p\"\nkind = \"lens\"\n").contains("document, parser or viewer"));
        assert!(refuse("mod_id = \"p\"\nkind = \"parser\"\nmedia_types = [\"nope\"]\n").contains("not a media type"));
    }
}
