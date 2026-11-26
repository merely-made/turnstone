// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Making a project folder: the embedded starter, or a copy of a fixture.

use std::path::{Path, PathBuf};

const STARTER_ID: &str = "notes-mod";

macro_rules! starter {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mod-projects/starter/", $name
        )) as &[u8])),*]
    };
}

/// The starter project: a note-editing document mod with its source and one
/// fixture. Its component imports no capability.
const STARTER: &[(&str, &[u8])] = starter![
    "mod.toml",
    "document.txt",
    "component.wasm",
    "fixtures/set.toml",
    "Cargo.toml",
    "src/lib.rs",
    "wit/world.wit",
];

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let name = entry.file_name();
        let target = to.join(&name);
        if entry.file_type()?.is_dir() {
            if name != "target" {
                copy_dir(&entry.path(), &target)?;
            }
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Make a project folder in `directory`, named by its mod id; its path.
pub(super) fn create(directory: &Path, from: Option<&Path>) -> Result<PathBuf, String> {
    let id = match from {
        Some(src) => super::manifest::read(src)?.manifest.mod_id,
        None => STARTER_ID.to_string(),
    };
    let root = directory.join(&id);
    if root.exists() {
        return Err(format!("{} already exists", root.display()));
    }
    let made = match from {
        Some(src) => copy_dir(src, &root),
        None => STARTER.iter().try_for_each(|(name, bytes)| {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap_or(&root))?;
            std::fs::write(path, bytes)
        }),
    };
    made.map_err(|e| format!("{}: {e}", root.display()))?;
    Ok(root)
}
