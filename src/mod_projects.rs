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
//! opened set and each project's granted capabilities are `installed.json`.
//! A mod links only the interfaces its manifest declares and the user granted
//! with the row's toggle, at either level, so a component importing `net` is
//! refused by name at check, attach and reload until `network` is granted. The
//! browser build does not compile this module.

// Every error here is a plain-words reason, a `String`; the doc says what fails.
#![allow(clippy::missing_errors_doc)]

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use document_host::mod_loader::{ModCapability, ModManifest, admit_wasm, capability_name};
use document_host::runtime::DocumentScriptRuntime;
use document_host::{CapPermission, DocumentScript, Grant, Quota, TurnOutcome, World};
use serde::{Deserialize, Serialize};

pub use crate::command_packs::Level;
pub use listing::{ProjectListing, ProjectRow};

mod build;
mod doc;
mod files;
mod fixtures_run;
mod listing;
mod manifest;
#[cfg(test)]
mod tests;

/// The directory under the data root that holds the projects.
pub const PROJECTS_DIR: &str = "mod-projects";
const LIST_FILE: &str = "installed.json";
const HISTORY: usize = 8;
const LOG_TAIL: usize = 6;
/// The last component that passed, kept in the project folder for rollback.
const LAST_GOOD: &str = "last-good.wasm";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Installed {
    root: PathBuf,
    level: Level,
    /// Granted capability names (`network`, ...); only declared ones count.
    #[serde(default)]
    granted: Vec<String>,
}

/// One opened project and what has happened to it.
struct Project {
    root: PathBuf,
    level: Level,
    manifest: ModManifest,
    version: String,
    component: PathBuf,
    document: PathBuf,
    granted: Vec<ModCapability>,
    script: Option<DocumentScript>,
    check_note: Option<String>,
    build_note: Option<String>,
    last_trap: Option<String>,
    last_turn: Option<String>,
    fixtures: Vec<String>,
    logs: Vec<String>,
    failure: Option<String>,
    building: bool,
}

impl Project {
    fn keep_logs(&mut self, logs: &[String]) {
        let from = logs.len().saturating_sub(LOG_TAIL);
        self.logs = logs[from..].to_vec();
    }

    /// The grant the component links under: `log` and `document`, and `net` only
    /// when the manifest declares `network` and the user granted it. The same at
    /// both levels.
    fn grant(&self) -> Grant {
        let net = self.manifest.capabilities.contains(&ModCapability::Network)
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

    /// Keep the grants the manifest still declares.
    fn prune_grants(&mut self) {
        let declared = self.manifest.capabilities.clone();
        self.granted.retain(|c| declared.contains(c));
    }
}

#[derive(Default)]
struct State {
    projects: BTreeMap<String, Project>,
    refusals: VecDeque<String>,
}

/// The runtime, the builder adapter and the app-owned project list.
pub struct ModProjectHost {
    directory: PathBuf,
    runtime: DocumentScriptRuntime,
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
        Self::open_with(data_root, "cargo")
    }

    /// As [`open`](Self::open), naming the cargo the build adapter shells out to.
    pub fn open_with(data_root: &Path, cargo: impl Into<PathBuf>) -> Self {
        let host = Self {
            directory: data_root.join(PROJECTS_DIR),
            // Grants are per project (`Project::grant`); the runtime holds records.
            runtime: DocumentScriptRuntime::new(Vec::new()),
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
                    let granted = project.granted;
                    if let Err(error) = self.try_open(&project.root, project.level, &granted) {
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
                granted: p.granted.iter().map(|c| capability_name(*c).into()).collect(),
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

    /// Open the project folder at `root` at `level`, admitting its component;
    /// the project's id.
    pub fn open_project(&self, root: &Path, level: Level) -> Result<String, String> {
        let result = self.try_open(root, level, &[]);
        if let Err(line) = &result {
            self.note(line.clone());
        }
        result
    }

    fn try_open(&self, root: &Path, level: Level, granted: &[String]) -> Result<String, String> {
        let spec = manifest::read(root).map_err(|e| format!("open {}: {e}", root.display()))?;
        self.runtime
            .admit(&spec.manifest, &spec.version, &spec.component, level.admission(), true)
            .map_err(|e| format!("open {}: {}", spec.manifest.mod_id, e.words()))?;
        let id = spec.manifest.mod_id.clone();
        let mut project = Project {
            root: root.to_path_buf(),
            level,
            granted: granted.iter().filter_map(|n| manifest::capability(n).ok()).collect(),
            manifest: spec.manifest,
            version: spec.version,
            component: spec.component,
            document: spec.document,
            script: None,
            check_note: None,
            build_note: None,
            last_trap: None,
            last_turn: None,
            fixtures: Vec::new(),
            logs: Vec::new(),
            failure: None,
            building: false,
        };
        project.prune_grants();
        let mut state = lock(&self.state);
        state.projects.insert(id.clone(), project);
        self.save_list(&state);
        Ok(id)
    }

    /// Copy `from` (the starter when `None`) into the projects directory under
    /// its mod id and open it.
    pub fn create(&self, from: Option<&Path>) -> Result<String, String> {
        let result = files::create(&self.directory, from)
            .and_then(|root| self.try_open(&root, Level::Conformant, &[]));
        if let Err(line) = &result {
            self.note(format!("create: {line}"));
        }
        result
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

    /// Grant or revoke a capability the manifest declares. It takes effect at
    /// the next attach or reload; an attached instance keeps what it linked.
    pub fn set_grant(&self, id: &str, capability: &str, granted: bool) -> Result<(), String> {
        let cap = manifest::capability(capability)?;
        self.with(id, |p, _| {
            if !p.manifest.capabilities.contains(&cap) {
                return Err(format!("{id} does not declare {capability} in mod.toml"));
            }
            p.granted.retain(|c| *c != cap);
            if granted {
                p.granted.push(cap);
            }
            Ok(())
        })?;
        self.save_list(&lock(&self.state));
        Ok(())
    }

    /// Check the component against its grants and the world (D1); every
    /// mismatch is named. With no fixtures to run, a pass makes it the last
    /// good component.
    pub fn check(&self, id: &str) -> Result<(), String> {
        self.with(id, |p, state| {
            self.check_in(id, p, state)?;
            if fixture_files(p).is_empty() {
                promote(p);
            }
            Ok(())
        })
    }

    fn check_in(&self, id: &str, p: &mut Project, state: &mut State) -> Result<(), String> {
        let report = document_host::check(&p.component, &p.grant(), &World::document_core())
            .map_err(|e| e.words());
        let words = match report {
            Ok(report) => {
                p.check_note = Some(report.words());
                if report.ok() {
                    return Ok(());
                }
                report.words()
            },
            Err(why) => {
                p.check_note = Some(why.clone());
                why
            },
        };
        let line = format!("{id}: {words}");
        Self::note_in(state, line.clone());
        p.failure = Some(words);
        Err(line)
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
                &p.grant(),
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

    /// Read `mod.toml` again, check the edited component, and swap it in on the
    /// same document; a failure leaves the old instance attached (D3, D12). The
    /// new component is admitted only after the swap succeeds, so the pin always
    /// names the running component.
    pub fn reload(&self, id: &str) -> Result<String, String> {
        self.with(id, |p, state| {
            let spec = manifest::read(&p.root).map_err(|e| format!("{id}: {e}"))?;
            p.manifest = spec.manifest;
            p.version = spec.version;
            p.component = spec.component;
            p.document = spec.document;
            p.prune_grants();
            self.check_in(id, p, state)?;
            let admit = |p: &Project| {
                self.runtime
                    .admit(&p.manifest, &p.version, &p.component, p.level.admission(), true)
                    .map_err(|e| format!("{id}: {}", e.words()))
            };
            let Some(mut script) = p.script.take() else {
                admit(p)?;
                p.failure = None;
                return Ok("re-admitted; nothing attached".into());
            };
            let mut record =
                admit_wasm(&p.manifest, &p.version, &p.component, p.level.admission(), &[], None)
                    .map_err(|e| format!("{id}: {}", e.words()));
            let result = match &mut record {
                Ok(record) => script
                    .reload(&p.component, &p.grant(), Quota::default(), None, Vec::new(), Some(record))
                    .map_err(|e| format!("reload: {}", e.words())),
                Err(why) => Err(why.clone()),
            };
            let logs = script.guest_logs().to_vec();
            p.keep_logs(&logs);
            p.script = Some(script);
            match result {
                Ok(()) => {
                    admit(p)?;
                    p.failure = None;
                    Ok("reloaded on the same document".into())
                },
                Err(why) => {
                    if let Some(trap) = record.ok().and_then(|r| r.last_trap) {
                        p.last_trap = Some(trap.note);
                    }
                    p.failure = Some(why.clone());
                    Self::note_in(state, format!("{id}: {why}"));
                    Err(format!("{id}: {why}; the old instance stays attached"))
                },
            }
        })
    }

    /// Move the project to `level`: admit its component again at that level.
    pub fn set_level(&self, id: &str, level: Level) -> Result<(), String> {
        self.with(id, |p, _| {
            self.runtime
                .admit(&p.manifest, &p.version, &p.component, level.admission(), true)
                .map_err(|e| format!("{id}: {}", e.words()))?;
            p.level = level;
            Ok(())
        })?;
        self.save_list(&lock(&self.state));
        Ok(())
    }

    /// The hash the install record pins for `id`.
    #[cfg(test)]
    fn pinned(&self, id: &str) -> Option<String> {
        self.runtime.record(id).map(|r| r.source_hash)
    }
}

/// The project's `fixtures/*.toml`, sorted.
fn fixture_files(p: &Project) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(p.root.join("fixtures"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|path| path.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();
    files
}

/// Keep the current component as `last-good.wasm`: the last one that passed its
/// check and every fixture, or its check alone where the project has no
/// fixtures (MP6 rolls back to it).
fn promote(p: &Project) {
    if let Err(error) = std::fs::copy(&p.component, p.root.join(LAST_GOOD)) {
        tracing::warn!(%error, "could not keep the last good component");
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
