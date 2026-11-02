// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The lexical index beside a memory store, on eidetic-recall's `<db>.index`
/// convention. Derived state: it is re-minted from the corpus, never repaired.
pub(super) fn index_dir(memory: &Path) -> PathBuf {
    let mut dir = memory.as_os_str().to_os_string();
    dir.push(".index");
    PathBuf::from(dir)
}
