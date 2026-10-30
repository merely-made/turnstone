// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Validate the global identity law: each pane spec and station appears once
/// across all live spaces, and fixed identity Formes name their actual graph.
pub fn validate_spaces(spaces: &[SpaceBlueprint]) -> Vec<BlueprintViolation> {
    let mut violations = Vec::new();
    let mut space_ids = HashSet::new();
    let mut specs: HashMap<PaneId, SpaceId> = HashMap::new();
    let mut station_counts: HashMap<PaneId, usize> = HashMap::new();

    for space in spaces {
        if !space_ids.insert(space.id.clone()) {
            violations.push(BlueprintViolation::DuplicateSpaceId(space.id.clone()));
        }
        let local_specs: HashSet<_> = space.panes.iter().map(|pane| pane.id).collect();
        for spec in &space.panes {
            if specs.insert(spec.id, space.id.clone()).is_some() {
                violations.push(BlueprintViolation::DuplicatePaneSpec(spec.id));
            }
            if let PaneSource::Fixed(SourceRef::Forme {
                graph,
                forme: FormeRef::Identity(identity_graph),
            }) = spec.source
                && graph.0 != identity_graph
            {
                violations.push(BlueprintViolation::FormeGraphMismatch(spec.id));
            }
        }
        for pane in space.station_ids() {
            if !local_specs.contains(&pane) {
                violations.push(BlueprintViolation::MissingPaneSpec {
                    space: space.id.clone(),
                    pane,
                });
            }
            *station_counts.entry(pane).or_default() += 1;
        }
    }

    for (pane, space) in specs {
        match station_counts.get(&pane).copied().unwrap_or(0) {
            0 => violations.push(BlueprintViolation::UnplacedPane { space, pane }),
            1 => {},
            _ => violations.push(BlueprintViolation::DuplicatePaneStation(pane)),
        }
    }
    violations
}
