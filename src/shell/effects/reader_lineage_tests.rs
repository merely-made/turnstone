// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::action::Action;

#[test]
fn reader_lineage_projects_and_reextract_updates_the_node_facet() {
    let mut app = crate::app::App::test_stub();
    app.update(Action::OpenAddress(
        "https://example.test/story".to_string(),
    ));
    let node = app.graph_runtimes.focused_member().unwrap();
    let mut lineage = crate::content::ExtractionLineageFacts {
        tool: "fleece".to_string(),
        version: "0.1.0".to_string(),
        selector: "main".to_string(),
        score: None,
        block_count: 3,
    };
    project_reader_lineage_facet(&mut app, node, Some(&lineage));
    let facet = chartulary::FacetId::new(crate::content::READER_LINEAGE_FACET);
    assert_eq!(
        app.graph_runtimes.facets().get(&node, &facet).unwrap()["version"],
        "0.1.0"
    );
    lineage.version = "0.2.0".to_string();
    lineage.block_count = 4;
    project_reader_lineage_facet(&mut app, node, Some(&lineage));
    let updated = app.graph_runtimes.facets().get(&node, &facet).unwrap();
    assert_eq!(updated["version"], "0.2.0");
    assert_eq!(updated["block_count"], 4);
}
