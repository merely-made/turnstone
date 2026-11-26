// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The authoring loop (mod projects MP6; D9, D29 to D32). The folder is the
//! protocol: anything may edit it. A pass snapshots the project's text files into
//! `last-good/`; the row diffs the folder against that snapshot and counts the
//! files that moved. Rollback puts `last-good.wasm` back over the component and
//! reloads through the pin; every edited file stays as it is.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use similar::{ChangeTag, TextDiff};

use super::{LAST_GOOD, ModProjectHost, Project, State, lock};

/// The snapshot directory a pass writes beside `last-good.wasm`.
pub(super) const SNAPSHOT: &str = "last-good";
/// Diff lines the row carries; the rest are counted.
const DIFF_LINES: usize = 10;
const ROOT_FILES: [&str; 2] = ["mod.toml", "Cargo.toml"];
const ROOT_DIRS: [&str; 2] = ["src", "fixtures"];

/// What changed in a project folder since its last pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Changes {
    pub files: Vec<String>,
    /// `name: -old` and `name: +new` lines, at most [`DIFF_LINES`].
    pub diff: Vec<String>,
    /// Diff lines past the cap.
    pub more: usize,
    /// Whether the component's bytes differ from `last-good.wasm`.
    pub wasm_differs: bool,
}

impl Changes {
    /// The row's count line (D32); the diff names the files.
    pub fn words(&self) -> String {
        let n = self.files.len();
        format!(
            "Changed since the last pass: {n} {}",
            if n == 1 { "file" } else { "files" }
        )
    }

    /// Whether the component is the one that passed (D29).
    pub fn component_words(&self) -> &'static str {
        if self.wasm_differs {
            "component.wasm differs from last-good.wasm"
        } else {
            "component.wasm matches last-good.wasm"
        }
    }

    pub fn diff_lines(&self) -> Vec<String> {
        let mut lines = self.diff.clone();
        if self.more > 0 {
            lines.push(format!("... {} more lines", self.more));
        }
        lines
    }
}

fn walk(base: &Path, rel: &str, out: &mut BTreeMap<String, String>) {
    let Ok(entries) = std::fs::read_dir(base.join(rel)) else {
        return;
    };
    for entry in entries.flatten() {
        let name = format!("{rel}/{}", entry.file_name().to_string_lossy());
        match entry.file_type() {
            Ok(t) if t.is_dir() => walk(base, &name, out),
            Ok(_) => {
                if let Ok(text) = std::fs::read_to_string(entry.path()) {
                    out.insert(name, normalise(&text));
                }
            },
            Err(_) => {},
        }
    }
}

/// Text with CRLF read as LF, so a line-ending change alone is no edit.
fn normalise(text: &str) -> String {
    text.lines().collect::<Vec<_>>().join("\n")
}

/// The project's text files under `base`, line endings normalised to LF: `mod.toml`, `Cargo.toml`, the
/// document, and everything in `src/` and `fixtures/`.
fn gather(base: &Path, document: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for name in ROOT_FILES.iter().copied().chain([document]) {
        if let Ok(text) = std::fs::read_to_string(base.join(name)) {
            out.insert(name.to_string(), normalise(&text));
        }
    }
    for dir in ROOT_DIRS {
        walk(base, dir, &mut out);
    }
    out
}

/// The document's path relative to the project folder, `/` separated.
fn document_name(p: &Project) -> String {
    p.document.strip_prefix(&p.root).map_or_else(
        |_| "document.txt".into(),
        |r| r.to_string_lossy().replace('\\', "/"),
    )
}

/// Replace `last-good/` with the project's text files as they are now.
pub(super) fn snapshot(p: &Project) {
    let dir = p.root.join(SNAPSHOT);
    let write = || -> std::io::Result<()> {
        match std::fs::remove_dir_all(&dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {},
        }
        std::fs::create_dir_all(&dir)?;
        for (name, text) in gather(&p.root, &document_name(p)) {
            let path: PathBuf = dir.join(&name);
            std::fs::create_dir_all(path.parent().unwrap_or(&dir))?;
            std::fs::write(path, text)?;
        }
        Ok(())
    };
    if let Err(error) = write() {
        tracing::warn!(%error, "could not snapshot the last passing sources");
    }
}

fn marked(name: &str, tag: char, text: &str) -> Vec<String> {
    text.lines()
        .map(|l| format!("{name}: {tag}{}", l.trim_end()))
        .collect()
}

/// The folder against its last pass; `None` before the first pass.
pub(super) fn scan(p: &Project) -> Option<Changes> {
    let snap = p.root.join(SNAPSHOT);
    if !snap.is_dir() {
        return None;
    }
    let document = document_name(p);
    let (old, new) = (gather(&snap, &document), gather(&p.root, &document));
    let mut names: Vec<&String> = old.keys().chain(new.keys()).collect();
    names.sort();
    names.dedup();
    let (mut files, mut lines) = (Vec::new(), Vec::new());
    for name in names {
        let (was, is) = (old.get(name), new.get(name));
        if was == is {
            continue;
        }
        files.push(name.clone());
        match (was, is) {
            (None, Some(is)) => lines.extend(marked(name, '+', is)),
            (Some(was), None) => lines.extend(marked(name, '-', was)),
            (Some(was), Some(is)) => {
                for change in TextDiff::from_lines(was, is).iter_all_changes() {
                    let tag = match change.tag() {
                        ChangeTag::Delete => '-',
                        ChangeTag::Insert => '+',
                        ChangeTag::Equal => continue,
                    };
                    lines.push(format!("{name}: {tag}{}", change.value().trim_end()));
                }
            },
            (None, None) => {},
        }
    }
    let more = lines.len().saturating_sub(DIFF_LINES);
    lines.truncate(DIFF_LINES);
    let wasm_differs = match (
        std::fs::read(p.root.join(LAST_GOOD)),
        std::fs::read(&p.component),
    ) {
        (Ok(good), Ok(now)) => good != now,
        (Ok(_), Err(_)) => true,
        _ => false,
    };
    Some(Changes {
        files,
        diff: lines,
        more,
        wasm_differs,
    })
}

impl ModProjectHost {
    /// Scan every project's folder against its last pass (D32): on pane open and
    /// after each action. Nothing watches the folder.
    pub fn refresh_changes(&self) {
        for p in lock(&self.state).projects.values_mut() {
            p.changes = scan(p);
        }
    }

    /// Put `last-good.wasm` back over the component and reload through the pin
    /// as Reload does; sources stay as edited (D30, D31). A page attachment must
    /// detach first.
    pub fn rollback(&self, id: &str) -> Result<String, String> {
        self.with(id, |p, state| self.rollback_in(id, p, state))
    }

    fn rollback_in(&self, id: &str, p: &mut Project, state: &mut State) -> Result<String, String> {
        if p.page.is_some() {
            return Err(format!("{id} is attached to a page; detach it first"));
        }
        let good = p.root.join(LAST_GOOD);
        if !good.is_file() {
            return Err(format!("{id} has no last passing build to roll back to"));
        }
        std::fs::copy(&good, &p.component)
            .map_err(|e| format!("{id}: could not restore {}: {e}", p.component.display()))?;
        match self.reload_in(id, p, state) {
            Ok(note) => Ok(format!("restored last-good.wasm; {note}")),
            Err(why) => Err(format!("restored last-good.wasm, but {why}")),
        }
    }
}
