// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Durable node references to explicitly captured source documents.
//!
//! The Fleece annotation owns all capture facts. This facet retains only its
//! manifest identities in append order, so it can select a node's captures
//! after restart without reconstructing an association by scanning Eidetic.

use chartulary::{AcceptAll, FacetError, FacetId};

/// A node-scoped ordered series of Eidetic Fleece annotation manifest ids.
pub const SOURCE_CAPTURE_REFERENCES_FACET: &str = "capture.source-annotations/v1";

/// Read one node's source annotation references in capture order.
pub fn references(facets: &pandect::NodeFacetStore, node: uuid::Uuid) -> Vec<String> {
    facets
        .get(&node, &FacetId::new(SOURCE_CAPTURE_REFERENCES_FACET))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect()
}

/// Append exactly one explicit capture reference. Repeated captures remain
/// repeated entries: vector position is the only copied ordering fact.
pub fn append_reference(
    facets: &mut pandect::NodeFacetStore,
    node: uuid::Uuid,
    annotation_manifest: String,
) -> Result<(), FacetError> {
    let mut series = references(facets, node);
    series.push(annotation_manifest);
    facets.set(
        node,
        FacetId::new(SOURCE_CAPTURE_REFERENCES_FACET),
        serde_json::json!(series),
        &AcceptAll,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_survive_facet_round_trip_in_capture_order() {
        let root = std::env::temp_dir().join(format!(
            "turnstone-source-capture-facet-{}",
            uuid::Uuid::new_v4()
        ));
        let node = uuid::Uuid::new_v4();
        let mut facets = pandect::NodeFacetStore::new();
        append_reference(&mut facets, node, "annotation-a".into()).unwrap();
        append_reference(&mut facets, node, "annotation-b".into()).unwrap();
        crate::session::save_node_facets(&root, &facets);

        let reopened = crate::session::load_node_facets(&root).unwrap();
        assert_eq!(
            references(&reopened, node),
            vec!["annotation-a".to_string(), "annotation-b".to_string()],
            "the facet is a durable ordered reference series"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
