// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Read-only compatibility for consumers that need a graph without adopting
/// session persistence authority. The application uses the fallible loader.
pub fn load_session_graph(data_root: &Path) -> Option<Graph> {
    match try_load_session_graph(data_root) {
        Ok(input) => input.map(|input| input.graph),
        Err(error) => {
            tracing::warn!(%error, "session graph load refused");
            None
        },
    }
}
