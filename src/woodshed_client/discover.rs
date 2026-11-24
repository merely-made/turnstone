// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0


//! Where a native Woodshed executable is found, in launch priority order.

use super::ClientError;
use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

/// Discovers supported native Woodshed executables in launch priority order.
/// A co-located app wins over alternate locations, so an installed Turnstone
/// pairs with its installed Woodshed sibling even if developer builds are on
/// `PATH`.
pub fn discover_executables() -> Vec<PathBuf> {
    if let Some(path) = colocated_executable() {
        return vec![path];
    }
    if let Some(path) = configured_executable() {
        return vec![path];
    }
    if let Some(root) = env::var_os("WOODSHED_REPO_DIR") {
        let root = PathBuf::from(root);
        let mut candidates = Vec::new();
        let profile = env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .and_then(|path| path.file_name().map(OsStr::to_os_string));
        if let Some(profile) = profile {
            let candidate = root.join("target").join(profile).join(executable_name());
            push_existing(&mut candidates, candidate);
        }
        if candidates.is_empty() {
            for profile in ["debug", "release"] {
                push_existing(
                    &mut candidates,
                    root.join("target").join(profile).join(executable_name()),
                );
            }
        }
        let candidates = deduplicate_paths(candidates);
        if !candidates.is_empty() {
            return candidates;
        }
    }
    let mut candidates = Vec::new();
    for directory in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        push_existing(&mut candidates, directory.join(executable_name()));
        #[cfg(not(windows))]
        push_existing(&mut candidates, directory.join("woodshed-genet"));
    }
    let candidates = deduplicate_paths(candidates);
    if !candidates.is_empty() {
        return candidates;
    }
    development_sibling_executable(&env::current_exe().ok().unwrap_or_default())
        .into_iter()
        .collect()
}

fn colocated_executable() -> Option<PathBuf> {
    let candidate = env::current_exe().ok()?.parent()?.join(executable_name());
    candidate.is_file().then_some(candidate)
}

pub(super) fn development_sibling_executable(turnstone_exe: &Path) -> Option<PathBuf> {
    let profile_directory = turnstone_exe.parent()?;
    let profile = profile_directory.file_name()?.to_str()?;
    if !matches!(profile, "debug" | "release")
        || turnstone_exe.file_name()?.to_string_lossy() != "turnstone.exe"
    {
        return None;
    }
    let shared_target_parent = profile_directory.parent()?.parent()?;
    let candidate = shared_target_parent
        .join("woodshed")
        .join(profile)
        .join(executable_name());
    candidate.is_file().then_some(candidate)
}

fn configured_executable() -> Option<PathBuf> {
    let configured = env::var_os("WOODSHED_EXE")?;
    let path = PathBuf::from(configured);
    path.is_file().then_some(path)
}

pub(super) fn executable_name() -> &'static str {
    if cfg!(windows) {
        "woodshed-genet.exe"
    } else {
        "woodshed-genet"
    }
}

fn push_existing(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if path.is_file() {
        paths.push(path);
    }
}

fn deduplicate_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths {
        let comparable = fs::canonicalize(&path).unwrap_or(path.clone());
        if !unique.iter().any(|existing: &PathBuf| {
            fs::canonicalize(existing).unwrap_or_else(|_| existing.clone()) == comparable
        }) {
            unique.push(path);
        }
    }
    unique
}

pub(super) fn validate_executable(path: PathBuf) -> Result<PathBuf, ClientError> {
    if !path.is_file() {
        return Err(ClientError::ExecutablePathNotFound(path));
    }
    fs::canonicalize(path).map_err(Into::into)
}
