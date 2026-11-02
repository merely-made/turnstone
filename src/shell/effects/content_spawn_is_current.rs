// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn content_spawn_is_current(app: &crate::app::App, node: uuid::Uuid, url: &str) -> bool {
    app.graph_runtimes
        .graph_containing_member(node)
        .and_then(|graph| app.graph_runtimes.canvas(graph))
        .is_some_and(|canvas| browse::still_current(canvas, node, url))
}
