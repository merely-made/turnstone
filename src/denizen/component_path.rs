// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Where a component participant's `.wasm` lives: `sessions/<id>/denizens/<subject>.wasm`
/// (beside the worlds — a resident's whole substance in one place).
pub fn component_path(session_dir: &Path, file: &str) -> PathBuf {
    session_dir.join("denizens").join(file)
}
