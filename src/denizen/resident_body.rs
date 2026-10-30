// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn resident_body(
    facets: &pandect::NodeFacetStore,
    member: Uuid,
    session_dir: &Path,
) -> Option<Vec<u8>> {
    let source = facets
        .get(&member, &chartulary::FacetId::new(SCENARIO_SOURCE_FACET))
        .and_then(|value| value.as_str())
        .map(|source| source.as_bytes().to_vec());
    if source.is_some() {
        return source;
    }
    let component = facets
        .get(&member, &chartulary::FacetId::new(COMPONENT_FACET))
        .and_then(|value| value.as_str())?;
    std::fs::read(component_path(session_dir, component)).ok()
}
