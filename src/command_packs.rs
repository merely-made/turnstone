// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Lua command packs in desktop Turnstone (command packs CP4; D11, D13 to D15).
//!
//! One registry holds every installed pack. The action catalog reads the
//! commands scoped to the `"app"` target; a pack's run yields requests that
//! the app dispatches through its ordinary action path, never effects of its
//! own. The browser build does not compile this module.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cambium::{Command, CommandSet, Invocation};
use command_packs::{CommandContext, PackRunner};
use document_host::mod_loader::{AdmissionLevel, Contribution, ModRegistry};
use serde::{Deserialize, Serialize};

/// The target name of the action catalog (`src/app/palette.rs`).
pub const APP_TARGET: &str = "app";
/// The command category of a pack row in the catalog.
pub const PACK_CATEGORY: &str = "pack";
/// The schema of the invocations a pack's run collects.
const SCHEMA: &str = "turnstone.pack/v1";
const INSTALLED_FILE: &str = "installed.json";
/// The directory under the data root that holds the pack list.
pub const PACKS_DIR: &str = "command-packs";
const HISTORY: usize = 8;

/// A pack's admission level, as the manager names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    Conformant,
    Dev,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Self::Conformant => "Conformant",
            Self::Dev => "Dev mode",
        }
    }

    fn admission(self) -> AdmissionLevel {
        match self {
            Self::Conformant => AdmissionLevel::Conformant,
            Self::Dev => AdmissionLevel::DevMode,
        }
    }

    pub fn other(self) -> Self {
        match self {
            Self::Conformant => Self::Dev,
            Self::Dev => Self::Conformant,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Installed {
    root: PathBuf,
    level: Level,
}

/// One pack as the settings section shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackRow {
    pub id: String,
    pub level: Level,
    pub status: String,
    pub commands: Vec<String>,
    pub grants: String,
    pub last_failure: Option<String>,
}

#[derive(Default)]
struct State {
    installed: BTreeMap<String, Installed>,
    failures: BTreeMap<String, String>,
    refusals: VecDeque<String>,
}

/// What the settings section shows: installed packs and the pack folders in
/// the packs directory that are not installed yet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackListing {
    pub rows: Vec<PackRow>,
    pub available: Vec<PathBuf>,
}

/// The registry, its runner and the app-owned pack list.
pub struct PackHost {
    directory: PathBuf,
    commands: Arc<Mutex<CommandSet>>,
    runner: Arc<PackRunner>,
    registry: Mutex<ModRegistry>,
    state: Mutex<State>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl PackHost {
    /// A host over `data_root/command-packs`, reinstalling the packs its list
    /// names. A pack that no longer admits is noted, not fatal.
    pub fn open(data_root: &Path) -> Self {
        let commands = Arc::new(Mutex::new(CommandSet::new()));
        let runner = Arc::new(PackRunner::new(commands.clone(), SCHEMA));
        let registry = ModRegistry::new().with_lua_runtime(runner.clone());
        let host = Self {
            directory: data_root.join(PACKS_DIR),
            commands,
            runner,
            registry: Mutex::new(registry),
            state: Mutex::new(State::default()),
        };
        host.load_list();
        host
    }

    fn list_path(&self) -> PathBuf {
        self.directory.join(INSTALLED_FILE)
    }

    fn load_list(&self) {
        let Ok(bytes) = std::fs::read(self.list_path()) else {
            return;
        };
        match serde_json::from_slice::<Vec<Installed>>(&bytes) {
            Ok(list) => {
                for pack in list {
                    if let Err(error) = self.install(&pack.root, pack.level) {
                        self.note(format!("startup: {error}"));
                    }
                }
            },
            Err(error) => self.note(format!("startup: {INSTALLED_FILE} unreadable: {error}")),
        }
    }

    fn save_list(&self, state: &State) {
        let list: Vec<&Installed> = state.installed.values().collect();
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(&self.directory)?;
            std::fs::write(self.list_path(), serde_json::to_vec_pretty(&list)?)
        };
        if let Err(error) = write() {
            tracing::warn!(%error, "could not save the command pack list");
        }
    }

    fn note(&self, line: String) {
        let mut state = lock(&self.state);
        state.refusals.push_front(line);
        state.refusals.truncate(HISTORY);
    }

    /// Install the pack at `root` at `level`; the new pack's id.
    pub fn install(&self, root: &Path, level: Level) -> Result<String, String> {
        let result = lock(&self.registry).install_lua_pack(root, level.admission());
        match result {
            Ok(id) => {
                let mut state = lock(&self.state);
                state.failures.remove(&id);
                state.installed.insert(
                    id.clone(),
                    Installed {
                        root: root.to_path_buf(),
                        level,
                    },
                );
                self.save_list(&state);
                Ok(id)
            },
            Err(error) => {
                let line = format!("install {}: {error}", root.display());
                self.note(line.clone());
                Err(line)
            },
        }
    }

    /// Re-admit and swap in pack `id`; a failure leaves the last good version
    /// active and is kept as the pack's last failed reload.
    pub fn reload(&self, id: &str) -> Result<(), String> {
        let result = lock(&self.registry).reload_lua_pack(id);
        let mut state = lock(&self.state);
        match result {
            Ok(()) => {
                state.failures.remove(id);
                Ok(())
            },
            Err(error) => {
                let text = error.to_string();
                state.failures.insert(id.to_string(), text.clone());
                Err(format!("reload {id}: {text}"))
            },
        }
    }

    pub fn unload(&self, id: &str) -> Result<(), String> {
        lock(&self.registry)
            .unload_lua_pack(id)
            .map_err(|error| format!("unload {id}: {error}"))?;
        let mut state = lock(&self.state);
        state.installed.remove(id);
        state.failures.remove(id);
        self.save_list(&state);
        Ok(())
    }

    /// Move pack `id` to `level`: unload and install again from its root.
    /// A refusal at the new level puts the old one back.
    pub fn set_level(&self, id: &str, level: Level) -> Result<(), String> {
        let Some(old) = lock(&self.state).installed.get(id).cloned() else {
            return Err(format!("no pack {id}"));
        };
        if old.level == level {
            return Ok(());
        }
        self.unload(id)?;
        match self.install(&old.root, level) {
            Ok(_) => Ok(()),
            Err(error) => {
                let _ = self.install(&old.root, old.level);
                lock(&self.state)
                    .failures
                    .insert(id.to_string(), error.clone());
                Err(error)
            },
        }
    }

    /// The pack commands listed on `target` (D13, D14).
    pub fn commands_for(&self, target: &str) -> Vec<Command> {
        self.runner.commands_for(target)
    }

    /// Offer the host's commands to packs: a pack may only request a command
    /// the host declares, and its effect class is the host's word.
    pub fn offer(&self, host: &CommandSet) {
        let pack_ids: Vec<String> = {
            let registry = lock(&self.registry);
            lock(&self.state)
                .installed
                .keys()
                .filter_map(|id| registry.lua_pack(id))
                .flat_map(|pack| pack.manifest.contributes.iter())
                .filter_map(|c| match c {
                    Contribution::Command { id, .. } => Some(id.clone()),
                    _ => None,
                })
                .collect()
        };
        let mut set = lock(&self.commands);
        for command in host.commands() {
            if !pack_ids.contains(&command.id) {
                set.register(command.clone());
            }
        }
    }

    /// Run pack command `id`; a refusal is kept for the status view.
    pub fn run(&self, id: &str, context: &CommandContext) -> Result<Vec<Invocation>, String> {
        self.runner.run(id, context).map_err(|refusal| {
            let line = format!("{id} refused: {refusal}");
            self.note(line.clone());
            line
        })
    }

    /// Record a refusal found while dispatching what a pack asked for.
    pub fn note_refusal(&self, line: String) {
        self.note(line);
    }

    /// What the pack status view shows: recent refusals, newest first, then
    /// each pack's last failed reload.
    pub fn status_lines(&self) -> Vec<String> {
        let state = lock(&self.state);
        let mut lines: Vec<String> = state.refusals.iter().cloned().collect();
        lines.extend(
            state
                .failures
                .iter()
                .map(|(id, why)| format!("last reload of {id} failed: {why}")),
        );
        if lines.is_empty() {
            lines.push("No command pack refusals".into());
        }
        lines
    }

    /// Installed packs, and pack folders in the packs directory not yet installed.
    pub fn listing(&self) -> PackListing {
        let installed: Vec<PathBuf> = lock(&self.state)
            .installed
            .values()
            .map(|pack| pack.root.clone())
            .collect();
        let mut available: Vec<PathBuf> = std::fs::read_dir(&self.directory)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.join("mod.toml").is_file() && !installed.contains(path))
            .collect();
        available.sort();
        PackListing {
            rows: self.rows(),
            available,
        }
    }

    /// The manager's rows, one per installed pack.
    pub fn rows(&self) -> Vec<PackRow> {
        let registry = lock(&self.registry);
        let state = lock(&self.state);
        state
            .installed
            .iter()
            .filter_map(|(id, installed)| {
                let pack = registry.lua_pack(id)?;
                let grants = &pack.manifest.grants;
                let mut parts: Vec<String> =
                    grants.classes.iter().map(|c| format!("{c:?}")).collect();
                parts.extend(grants.commands.iter().cloned());
                Some(PackRow {
                    id: id.clone(),
                    level: installed.level,
                    status: registry
                        .get_status(id)
                        .map_or_else(|| "unknown".into(), |s| format!("{s:?}")),
                    commands: pack
                        .manifest
                        .contributes
                        .iter()
                        .filter_map(|c| match c {
                            Contribution::Command { id, label, .. } => {
                                Some(format!("{label} ({id})"))
                            },
                            _ => None,
                        })
                        .collect(),
                    grants: if parts.is_empty() {
                        "none".into()
                    } else {
                        parts.join(", ")
                    },
                    last_failure: state.failures.get(id).cloned(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn copy_fixture(to: &Path) -> PathBuf {
        let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/command-packs/demo-pack");
        let root = to.join("demo-pack");
        std::fs::create_dir_all(&root).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            std::fs::copy(entry.path(), root.join(entry.file_name())).unwrap();
        }
        root
    }

    fn host_set() -> CommandSet {
        let mut set = CommandSet::new();
        for (id, effect) in [
            ("view:fit", cambium::EffectClass::Curation),
            ("nav:reload", cambium::EffectClass::ExternalEffect),
        ] {
            set.register(Command::new(id, id, "turnstone", effect));
        }
        set
    }

    #[test]
    fn a_pack_installs_runs_is_refused_and_reloads_to_changed_behaviour() {
        let dir = tempfile::tempdir().unwrap();
        let root = copy_fixture(dir.path());
        let host = PackHost::open(dir.path());
        host.offer(&host_set());
        assert_eq!(host.install(&root, Level::Conformant).unwrap(), "demo-pack");
        let ids: Vec<_> = host.commands_for(APP_TARGET).into_iter().map(|c| c.id).collect();
        assert_eq!(ids, ["demo-pack:fit", "demo-pack:reload"]);
        let context = CommandContext::new();
        assert_eq!(host.run("demo-pack:fit", &context).unwrap()[0].id, "view:fit");
        let refused = host.run("demo-pack:reload", &context).unwrap_err();
        assert!(refused.contains("not granted nav:reload"), "{refused}");
        assert!(host.status_lines()[0].contains("not granted"));

        let manifest = root.join("mod.toml");
        let text = std::fs::read_to_string(&manifest).unwrap().replace("\r\n", "\n");
        let edited = text.replace("[\"Curation\"]", "[\"Curation\", \"ExternalEffect\"]");
        std::fs::write(&manifest, edited).unwrap();
        host.reload("demo-pack").unwrap();
        assert_eq!(host.run("demo-pack:reload", &context).unwrap()[0].id, "nav:reload");
    }

    #[test]
    fn a_failed_reload_is_kept_and_the_last_good_pack_stays_active() {
        let dir = tempfile::tempdir().unwrap();
        let root = copy_fixture(dir.path());
        let host = PackHost::open(dir.path());
        host.offer(&host_set());
        host.install(&root, Level::Conformant).unwrap();
        std::fs::write(root.join("mod.toml"), "not = [toml").unwrap();
        assert!(host.reload("demo-pack").is_err());
        assert!(host.rows()[0].last_failure.is_some());
        assert!(host.run("demo-pack:fit", &CommandContext::new()).is_ok());
    }

    #[test]
    fn the_list_survives_a_restart_and_levels_change_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let root = copy_fixture(&dir.path().join(PACKS_DIR));
        let host = PackHost::open(dir.path());
        host.install(&root, Level::Conformant).unwrap();
        assert!(host.listing().available.is_empty());
        host.set_level("demo-pack", Level::Dev).unwrap();
        assert_eq!(host.rows()[0].level, Level::Dev);
        let again = PackHost::open(dir.path());
        assert_eq!(again.rows()[0].level, Level::Dev);
        again.unload("demo-pack").unwrap();
        assert!(again.rows().is_empty());
        assert_eq!(again.listing().available, [root]);
    }
}
