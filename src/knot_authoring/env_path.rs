// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A path setting, with an empty or blank value read as unset.
///
/// A child process inherits every name its parent exported, including the ones
/// the parent left blank, and `var_os` answers `Some("")` for those. Treating
/// that as a path sends the vault open at the current directory and fails, so a
/// profile that was never given a vault is told it has none.
pub(crate) fn env_path(name: &str) -> Option<PathBuf> {
    let value = std::env::var_os(name)?;
    if value.to_string_lossy().trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}
