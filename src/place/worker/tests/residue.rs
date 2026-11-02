// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// (Gemot store, sealed secrets, admitted binding).
pub(super) fn residue(directory: &Path) -> (bool, bool, bool) {
    (
        place_store_dir(directory).exists(),
        place_secrets_dir(directory).exists(),
        crate::session::place_binding_path(directory).exists(),
    )
}
