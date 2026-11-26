// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Wasm mod projects in desktop Turnstone (graphshell mod-projects MP5; D1, D2,
//! D5, D6, D12).
//!
//! A project is a folder: `mod.toml`, a component, a starting document and
//! `fixtures/*.toml`. Projects live under `<data_root>/mod-projects/`; the
//! opened set is `installed.json`. Turnstone grants a mod nothing beyond the
//! world's `log` and `document` (MP0), so a component importing `net` is
//! refused by name at check, attach and reload. The browser build does not
//! compile this module.

// Every error here is a plain-words reason, a `String`; the doc says what fails.
#![allow(clippy::missing_errors_doc)]

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use document_host::mod_loader::{ModCapability, ModManifest, RecordEvent};
use document_host::runtime::DocumentScriptRuntime;
use document_host::{
    CapPermission, CargoBuilder, DocumentScript, Grant, Quota, TurnOutcome, World,
};
use serde::{Deserialize, Serialize};

pub use crate::command_packs::Level;

mod doc;
mod files;
mod fixtures_run;
mod manifest;
#[cfg(test)]
mod tests;

/// The directory under the data root that holds the projects.
pub const PROJECTS_DIR: &str = "mod-projects";
const LIST_FILE: &str = "installed.json";
const HISTORY: usize = 8;
const LOG_TAIL: usize = 6;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Installed {
    root: PathBuf,
    level: Level,
}

/// One opened project and what has happened to it.
struct Project {
    root: PathBuf,
    level: Level,
    manifest: ModManifest,
    version: String,
    component: PathBuf,
    document: PathBuf,
    script: Option<DocumentScript>,
    last_trap: Option<String>,
    last_turn: Option<String>,
    fixtures: Vec<String>,
    logs: Vec<String>,
    failure: Option<String>,
    /// The last build's outcome: the record is replaced at every re-admission.
    build_note: Option<String>,
    building: bool,
}

impl Project {
    fn keep_logs(&mut self, logs: &[String]) {
        let from = logs.len().saturating_sub(LOG_TAIL);
        self.logs = logs[from..].to_vec();
    }
}

#[derive(Default)]
struct State {
    projects: BTreeMap<String, Project>,
    refusals: VecDeque<String>,
}

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
}

/// The rows, and project folders in the projects directory not opened yet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectListing {
    pub rows: Vec<ProjectRow>,
    pub available: Vec<PathBuf>,
}

/// The runtime, the builder adapter and the app-owned project list.
pub struct ModProjectHost {
    directory: PathBuf,
    runtime: DocumentScriptRuntime,
    /// Host capabilities Turnstone grants a mod: none (MP0).
    granted: Vec<ModCapability>,
    cargo: PathBuf,
    toolchain: OnceLock<bool>,
    state: Mutex<State>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl ModProjectHost {
    /// A host over `data_root/mod-projects`, reopening the projects its list
    /// names. One that no longer admits is noted, not fatal.
    pub fn open(data_root: &Path) -> Self {
        Self::open_with(data_root, "cargo", Vec::new())
    }

    /// As [`open`](Self::open), naming the cargo the build adapter shells out
    /// to and the host capabilities granted.
    pub fn open_with(
        data_root: &Path,
        cargo: impl Into<PathBuf>,
        granted: Vec<ModCapability>,
    ) -> Self {
        let host = Self {
            directory: data_root.join(PROJECTS_DIR),
            runtime: DocumentScriptRuntime::new(granted.clone()),
            granted,
            cargo: cargo.into(),
            toolchain: OnceLock::new(),
            state: Mutex::new(State::default()),
        };
        host.load_list();
        host
    }

    fn list_path(&self) -> PathBuf {
        self.directory.join(LIST_FILE)
    }

    fn load_list(&self) {
        let Ok(bytes) = std::fs::read(self.list_path()) else {
            return;
        };
        match serde_json::from_slice::<Vec<Installed>>(&bytes) {
            Ok(list) => {
                for project in list {
                    if let Err(error) = self.open_project(&project.root, project.level) {
                        self.note(format!("startup: {error}"));
                    }
                }
            },
            Err(error) => self.note(format!("startup: {LIST_FILE} unreadable: {error}")),
        }
    }

    fn save_list(&self, state: &State) {
        let list: Vec<Installed> = state
            .projects
            .values()
            .map(|p| Installed {
                root: p.root.clone(),
                level: p.level,
            })
            .collect();
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(&self.directory)?;
            std::fs::write(self.list_path(), serde_json::to_vec_pretty(&list)?)
        };
        if let Err(error) = write() {
            tracing::warn!(%error, "could not save the mod project list");
        }
    }

    fn note(&self, line: String) {
        Self::note_in(&mut lock(&self.state), line);
    }

    fn note_in(state: &mut State, line: String) {
        state.refusals.push_front(line);
        state.refusals.truncate(HISTORY);
    }

    /// Whether the build adapter finds a toolchain (asked once).
    pub fn toolchain_present(&self) -> bool {
        *self.toolchain.get_or_init(|| {
            std::process::Command::new(&self.cargo)
                .arg("--version")
                .output()
                .is_ok_and(|out| out.status.success())
        })
    }

    /// Open the project folder at `root` at `level`, admitting its component;
    /// the project's id.
    pub fn open_project(&self, root: &Path, level: Level) -> Result<String, String> {
        let result = self.try_open(root, level);
        if let Err(line) = &result {
            self.note(line.clone());
        }
        result
    }

    fn try_open(&self, root: &Path, level: Level) -> Result<String, String> {
        let spec = manifest::read(root).map_err(|e| format!("open {}: {e}", root.display()))?;
        self.runtime
            .admit(
                &spec.manifest,
                &spec.version,
                &spec.component,
                level.admission(),
                true,
            )
            .map_err(|e| format!("open {}: {}", spec.manifest.mod_id, e.words()))?;
        let id = spec.manifest.mod_id.clone();
        let mut state = lock(&self.state);
        state.projects.insert(
            id.clone(),
            Project {
                root: root.to_path_buf(),
                level,
                manifest: spec.manifest,
                version: spec.version,
                component: spec.component,
                document: spec.document,
                script: None,
                last_trap: None,
                last_turn: None,
                fixtures: Vec::new(),
                logs: Vec::new(),
                failure: None,
                build_note: None,
                building: false,
            },
        );
        self.save_list(&state);
        Ok(id)
    }

    /// Copy `from` (the starter when `None`) into the projects directory under
    /// its mod id and open it.
    pub fn create(&self, from: Option<&Path>) -> Result<String, String> {
        let result = files::create(&self.directory, from)
            .and_then(|root| self.try_open(&root, Level::Conformant));
        if let Err(line) = &result {
            self.note(format!("create: {line}"));
        }
        result
    }

    /// The grant a project's component links under: `log` and `document`, and
    /// `net` only for a conformant mod that declares and was granted it (D2).
    fn grant(&self, project: &Project) -> Grant {
        let net = project.level == Level::Conformant
            && project.manifest.capabilities.contains(&ModCapability::Network)
            && self.granted.contains(&ModCapability::Network);
        Grant {
            log: CapPermission::Allow,
            document: CapPermission::Allow,
            net: if net {
                CapPermission::Allow
            } else {
                CapPermission::Deny
            },
        }
    }

    fn with<T>(
        &self,
        id: &str,
        run: impl FnOnce(&mut Project, &mut State) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = lock(&self.state);
        let state = &mut *guard;
        let mut project = state
            .projects
            .remove(id)
            .ok_or_else(|| format!("no mod project {id}"))?;
        let result = run(&mut project, state);
        state.projects.insert(id.to_string(), project);
        result
    }

    /// Check the component against its grants and the world (D1); every
    /// mismatch is named.
    pub fn check(&self, id: &str) -> Result<(), String> {
        self.with(id, |p, state| self.check_in(id, p, state))
    }

    fn check_in(&self, id: &str, p: &mut Project, state: &mut State) -> Result<(), String> {
        let report = self
            .runtime
            .check(&p.manifest, &p.component, &World::document_core())
            .map_err(|e| e.words());
        let words = match report {
            Ok(report) if report.ok() => return Ok(()),
            Ok(report) => report.words(),
            Err(why) => why,
        };
        let line = format!("{id}: {words}");
        Self::note_in(state, line.clone());
        p.failure = Some(words);
        Err(line)
    }

    /// Note the check again: a re-admission starts a fresh record.
    fn recheck(&self, p: &Project) {
        let _ = self.runtime.check(&p.manifest, &p.component, &World::document_core());
    }

    /// Build the project with the toolchain adapter and put the result in
    /// place as its component. Blocks; the app calls [`build`](Self::build).
    pub fn build_blocking(&self, id: &str) -> Result<String, String> {
        let root = self.with(id, |p, _| {
            if p.building {
                return Err(format!("{id} is already building"));
            }
            p.building = true;
            Ok(p.root.clone())
        })?;
        let builder = CargoBuilder::new()
            .with_cargo(self.cargo.clone())
            .with_target_dir(root.join("target"));
        let built = self.runtime.build_project(id, &builder, &root);
        self.with(id, |p, state| {
            p.building = false;
            let outcome = built.map_err(|e| e.words()).and_then(|b| {
                std::fs::copy(&b.path, &p.component)
                    .map(|bytes| format!("built {} ({bytes} bytes)", p.component.display()))
                    .map_err(|e| format!("could not place {}: {e}", b.path.display()))
            });
            p.build_note = Some(match &outcome {
                Ok(note) => note.clone(),
                Err(why) => why.clone(),
            });
            outcome.map_err(|why| {
                let line = format!("{id}: build: {why}");
                Self::note_in(state, line.clone());
                p.failure = Some(format!("build: {why}"));
                line
            })
        })
    }

    /// Build on a worker thread; the listing shows the outcome when it lands.
    pub fn build(self: &Arc<Self>, id: &str) -> Result<(), String> {
        if !self.toolchain_present() {
            let line = format!("{id}: build: no toolchain ({} was not found)", self.cargo.display());
            self.note(line.clone());
            return Err(line);
        }
        let (host, id) = (self.clone(), id.to_string());
        std::thread::spawn(move || {
            let _ = host.build_blocking(&id);
        });
        Ok(())
    }

    /// Attach the checked component to the project's document (D5).
    pub fn attach(&self, id: &str) -> Result<(), String> {
        self.with(id, |p, state| {
            if p.script.is_some() {
                return Err(format!("{id} is already attached; reload or detach it"));
            }
            self.check_in(id, p, state)?;
            let record = self
                .runtime
                .record(id)
                .ok_or_else(|| format!("{id} is not admitted"))?;
            let tree = match std::fs::read_to_string(&p.document) {
                Ok(text) => text,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => doc::DEFAULT_DOCUMENT.into(),
                Err(e) => return Err(format!("{}: {e}", p.document.display())),
            };
            let dom = doc::parse(&tree)?;
            let attached = DocumentScript::attach_pinned(
                &p.component,
                &record,
                dom,
                &self.grant(p),
                Quota::default(),
                None,
                Vec::new(),
            );
            match attached {
                Ok(script) => {
                    p.keep_logs(script.guest_logs());
                    p.script = Some(script);
                    p.failure = None;
                    Ok(())
                },
                Err(e) => {
                    let why = format!("attach: {e:#}");
                    p.last_trap = Some(why.clone());
                    p.failure = Some(why.clone());
                    Self::note_in(state, format!("{id}: {why}"));
                    Err(format!("{id}: {why}"))
                },
            }
        })
    }

    pub fn detach(&self, id: &str) -> Result<(), String> {
        self.with(id, |p, _| {
            let script = p
                .script
                .take()
                .ok_or_else(|| format!("{id} is not attached"))?;
            let _ = script.detach();
            Ok(())
        })
    }

    /// Deliver one event to the attached script; a trap detaches it and is the
    /// project's last trap.
    pub fn turn(&self, id: &str, kind: &str, payload: &str) -> Result<String, String> {
        self.with(id, |p, _| {
            let script = p
                .script
                .as_mut()
                .ok_or_else(|| format!("{id} is not attached"))?;
            match script.deliver_event(kind, payload) {
                Ok(outcome) => {
                    let line = format!("turn {kind}: {}", outcome_words(&outcome));
                    let logs = script.guest_logs().to_vec();
                    p.keep_logs(&logs);
                    p.last_turn = Some(line.clone());
                    Ok(line)
                },
                Err(e) => {
                    let why = format!("turn '{kind}' trapped: {e:#}");
                    p.script = None;
                    p.last_trap = Some(why.clone());
                    p.last_turn = Some(format!("turn {kind}: trapped, detached"));
                    Err(format!("{id}: {why}"))
                },
            }
        })
    }

    /// Re-admit the edited component and swap it in on the same document; a
    /// failure leaves the old instance attached (D3, D12).
    pub fn reload(&self, id: &str) -> Result<String, String> {
        self.with(id, |p, state| {
            self.check_in(id, p, state)?;
            self.runtime
                .admit(&p.manifest, &p.version, &p.component, p.level.admission(), true)
                .map_err(|e| format!("{id}: {}", e.words()))?;
            self.recheck(p);
            let Some(mut script) = p.script.take() else {
                p.failure = None;
                return Ok("re-admitted; nothing attached".into());
            };
            let mut record = self.runtime.record(id);
            let result = script.reload(
                &p.component,
                &self.grant(p),
                Quota::default(),
                None,
                Vec::new(),
                record.as_mut(),
            );
            let logs = script.guest_logs().to_vec();
            p.keep_logs(&logs);
            p.script = Some(script);
            result.map(|()| {
                p.failure = None;
                "reloaded on the same document".to_string()
            })
            .map_err(|e| {
                let why = format!("reload: {}", e.words());
                if let Some(trap) = record.and_then(|r| r.last_trap) {
                    p.last_trap = Some(trap.note);
                }
                p.failure = Some(why.clone());
                Self::note_in(state, format!("{id}: {why}"));
                format!("{id}: {why}; the old instance stays attached")
            })
        })
    }

    /// Move the project to `level`: admit its component again at that level.
    pub fn set_level(&self, id: &str, level: Level) -> Result<(), String> {
        self.with(id, |p, _| {
            self.runtime
                .admit(&p.manifest, &p.version, &p.component, level.admission(), true)
                .map_err(|e| format!("{id}: {}", e.words()))?;
            self.recheck(p);
            p.level = level;
            Ok(())
        })?;
        self.save_list(&lock(&self.state));
        Ok(())
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
        let note = |e: Option<RecordEvent>, none: &str| e.map_or_else(|| none.to_string(), |e| e.note);
        let rows = state
            .projects
            .iter()
            .map(|(id, p)| {
                let record = self.runtime.record(id);
                ProjectRow {
                    id: id.clone(),
                    level: p.level,
                    status: p.script.as_ref().map_or_else(
                        || "Not attached".into(),
                        |s| format!("Attached, revision {}", s.revision()),
                    ),
                    check: note(record.as_ref().and_then(|r| r.last_check.clone()), "not checked"),
                    build: p.build_note.clone().unwrap_or_else(|| "not built".into()),
                    trap: p.last_trap.clone().unwrap_or_else(|| "none".into()),
                    document: p.script.as_ref().map(|s| doc::outline(s.dom())).unwrap_or_default(),
                    last_turn: p.last_turn.clone(),
                    fixtures: p.fixtures.clone(),
                    logs: p.logs.clone(),
                    failure: p.failure.clone(),
                    attached: p.script.is_some(),
                    can_build: toolchain && p.root.join("Cargo.toml").is_file(),
                }
            })
            .collect();
        ProjectListing { rows, available }
    }
}

fn outcome_words(outcome: &TurnOutcome) -> String {
    match outcome {
        TurnOutcome::Applied(rev) => format!("applied, rev {rev}"),
        TurnOutcome::Conflict(rev) => format!("conflict, current rev {rev}"),
        TurnOutcome::UnknownNode(id) => format!("unknown node {id}"),
        TurnOutcome::Refused(why) => format!("refused ({why})"),
    }
}
