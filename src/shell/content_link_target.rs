// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Resolve a session-authored link against the node that owns that content
/// surface before lowering it to `OpenAddress`. Session engines preserve the
/// authored spelling for inspection; the host owns the navigation context.
pub(super) fn content_link_target(app: &App, node: uuid::Uuid, href: &str) -> String {
    app.graph_runtimes
        .graph()
        .get_node_by_id(node)
        .map(|(_, owner)| {
            url::Url::parse(owner.url())
                .and_then(|base| base.join(href))
                .map(|target| target.to_string())
                .unwrap_or_else(|_| genet_documents::resolve_href(owner.url(), href))
        })
        .unwrap_or_else(|| href.to_string())
}
