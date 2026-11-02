// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Missing inputs are fresh; corrupt or unreadable inputs refuse persistence.
/// Read canonical facets before any migration write or image deposition.
pub(crate) fn try_load_session_graph(
    data_root: &Path,
) -> Result<Option<LoadedSessionGraph>, String> {
    let _writer = crate::session_persistence::SessionWriteLease::acquire(data_root)
        .map_err(|error| format!("session writer lease refused: {error}"))?;
    try_load_session_graph_locked(data_root)
}
