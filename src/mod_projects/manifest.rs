// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A project's `mod.toml`: what the folder names and what it declares.

use std::path::{Component, Path, PathBuf};

use document_host::mod_loader::{ModCapability, ModManifest, ModType};
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
}

pub(super) struct Spec {
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
        manifest,
        version: disk.version,
        component: inside(root, "component", &disk.component)?,
        document: inside(root, "document", &disk.document)?,
    })
}
