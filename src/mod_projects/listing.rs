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
    /// The manifest's display name: a viewer's pane kind is titled with it (MP7c, D63).
    pub title: String,
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
    /// A last passing build exists to roll back to (D30).
    pub can_rollback: bool,
    /// Files changed since the last pass, and the component against it (D32).
    pub changes: String,
    /// Whether the component is the last passing one (empty before a pass).
    pub component: String,
    /// The line diff against the last pass's snapshot, capped (D29).
    pub diff: Vec<String>,
    /// Declared capabilities and whether each is granted.
    pub capabilities: Vec<(String, bool)>,
    /// `document`, `parser` or `viewer` (MP7).
    pub kind: String,
    /// A parser's declared media types: whether a built-in also claims it, and
    /// whether the user handed it to this parser (D39). Unclaimed types are the
    /// parser's by default.
    pub media_types: Vec<(String, bool, bool)>,
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
                title: p.manifest.display_name.clone(),
                level: p.level,
                status: if let Some(page) = &p.page {
                    format!("Attached to a page, revision {}", page.script.revision())
                } else if let Some(s) = &p.script {
                    format!("Attached, revision {}", s.revision())
                } else if let Some(why) = &p.detached {
                    format!("Not attached ({why})")
                } else {
                    "Not attached".into()
                },
                check: p.check_note.clone().unwrap_or_else(|| "not checked".into()),
                build: p.build_note.clone().unwrap_or_else(|| "not built".into()),
                trap: p.last_trap.clone().unwrap_or_else(|| "none".into()),
                document: match (&p.page, &p.script) {
                    (Some(page), _) => Self::page_outline(page),
                    (None, Some(s)) => doc::outline(s.dom()),
                    _ => Vec::new(),
                },
                last_turn: p.last_turn.clone(),
                fixtures: p.fixtures.clone(),
                logs: p.logs.clone(),
                failure: p.failure.clone(),
                attached: p.script.is_some() || p.page.is_some(),
                can_build: toolchain && p.root.join("Cargo.toml").is_file(),
                can_rollback: p.root.join(super::LAST_GOOD).is_file(),
                changes: p.changes.as_ref().map_or_else(
                    || "No passing build yet, so nothing to compare against".into(),
                    super::authoring::Changes::words,
                ),
                component: p.changes.as_ref().map(|c| c.component_words().to_string()).unwrap_or_default(),
                diff: p.changes.as_ref().map(super::authoring::Changes::diff_lines).unwrap_or_default(),
                capabilities: p
                    .manifest
                    .capabilities
                    .iter()
                    .map(|c| (capability_name(*c).to_string(), p.granted.contains(c)))
                    .collect(),
                kind: p.kind.as_str().to_string(),
                media_types: p
                    .media_types
                    .iter()
                    .map(|m| (m.clone(), crate::mod_parser::builtin_claims(m), p.claimed_on.contains(m)))
                    .collect(),
            })
            .collect();
        ProjectListing { rows, available }
    }
}
