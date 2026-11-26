// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! What the settings section shows: rows, unopened folders and refusals.

use std::path::PathBuf;

use document_host::mod_loader::capability_name;

use super::{Level, ModProjectHost, doc, lock, manifest};

/// One project as the settings section shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectRow {
    pub id: String,
    pub level: Level,
    pub status: String,
    pub check: String,
    pub build: String,
    pub trap: String,
    pub document: Vec<String>,
    pub last_turn: Option<String>,
    pub fixtures: Vec<String>,
    pub logs: Vec<String>,
    pub failure: Option<String>,
    pub attached: bool,
    pub can_build: bool,
    /// Declared capabilities and whether each is granted.
    pub capabilities: Vec<(String, bool)>,
}

/// The rows, and project folders in the projects directory not opened yet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectListing {
    pub rows: Vec<ProjectRow>,
    pub available: Vec<PathBuf>,
}

impl ModProjectHost {
    /// Whether the build adapter finds a toolchain (asked once).
    pub fn toolchain_present(&self) -> bool {
        *self.toolchain.get_or_init(|| {
            std::process::Command::new(&self.cargo)
                .arg("--version")
                .output()
                .is_ok_and(|out| out.status.success())
        })
    }

    /// Recent refusals, newest first, then each project's last failure.
    pub fn status_lines(&self) -> Vec<String> {
        let state = lock(&self.state);
        let mut lines: Vec<String> = state.refusals.iter().cloned().collect();
        lines.extend(state.projects.iter().filter_map(|(id, p)| {
            p.failure
                .as_ref()
                .map(|why| format!("last failure of {id}: {why}"))
        }));
        if lines.is_empty() {
            lines.push("No mod project refusals".into());
        }
        lines
    }

    /// Opened projects, and project folders in the directory not opened yet.
    pub fn listing(&self) -> ProjectListing {
        let state = lock(&self.state);
        let open: Vec<&PathBuf> = state.projects.values().map(|p| &p.root).collect();
        let mut available: Vec<PathBuf> = std::fs::read_dir(&self.directory)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.join(manifest::MANIFEST).is_file() && !open.contains(&path))
            .collect();
        available.sort();
        let toolchain = self.toolchain_present();
        let rows = state
            .projects
            .iter()
            .map(|(id, p)| ProjectRow {
                id: id.clone(),
                level: p.level,
                status: p.script.as_ref().map_or_else(
                    || "Not attached".into(),
                    |s| format!("Attached, revision {}", s.revision()),
                ),
                check: p.check_note.clone().unwrap_or_else(|| "not checked".into()),
                build: p.build_note.clone().unwrap_or_else(|| "not built".into()),
                trap: p.last_trap.clone().unwrap_or_else(|| "none".into()),
                document: p.script.as_ref().map(|s| doc::outline(s.dom())).unwrap_or_default(),
                last_turn: p.last_turn.clone(),
                fixtures: p.fixtures.clone(),
                logs: p.logs.clone(),
                failure: p.failure.clone(),
                attached: p.script.is_some(),
                can_build: toolchain && p.root.join("Cargo.toml").is_file(),
                capabilities: p
                    .manifest
                    .capabilities
                    .iter()
                    .map(|c| (capability_name(*c).to_string(), p.granted.contains(c)))
                    .collect(),
            })
            .collect();
        ProjectListing { rows, available }
    }
}
