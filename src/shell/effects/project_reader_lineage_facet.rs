// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn project_reader_lineage_facet(
    app: &mut crate::app::App,
    node: uuid::Uuid,
    lineage: Option<&crate::content::ExtractionLineageFacts>,
) {
    let Some(lineage) = lineage else {
        return;
    };
    let value = serde_json::json!({
        "tool": lineage.tool,
        "version": lineage.version,
        "selector": lineage.selector,
        "score": lineage.score,
        "block_count": lineage.block_count,
    });
    if let Err(error) = app.graph_runtimes.edit_facets(|facets| {
        facets.set(
            node,
            chartulary::FacetId::new(crate::content::READER_LINEAGE_FACET),
            value,
            &chartulary::AcceptAll,
        )
    }) {
        tracing::warn!(%node, %error, "could not project reader extraction lineage facet");
    }
}
