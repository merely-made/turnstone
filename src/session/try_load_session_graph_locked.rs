// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn try_load_session_graph_locked(
    data_root: &Path,
) -> Result<Option<LoadedSessionGraph>, String> {
    // Pandect's compatibility reader uses exists(); distinguish inspection
    // failure here before it can silently treat an unreadable path as absent.
    crate::session_persistence::require_complete_file(&pandect::node_facets_path(data_root))
        .map_err(|error| format!("canonical facets could not be inspected: {error}"))?;
    let canonical = pandect::load_node_facets(data_root)
        .map_err(|error| format!("canonical facets could not be read: {error}"))?;
    let graph_file = data_root.join(session_graph_store::GRAPH_FILE);
    crate::session_persistence::require_complete_file(&graph_file)
        .map_err(|error| format!("session graph replacement is incomplete: {error}"))?;
    let Some(mut input) = crate::session_graph_file::load_profiled_snapshot(&graph_file)
        .map_err(|error| format!("session graph could not be read: {error}"))?
    else {
        return Ok(None);
    };
    let placement = input.placement;
    let snapshot = &mut input.snapshot;
    // Validate the untouched source before stripping old controls or depositing blobs.
    pandect::graph_placement::materialize_snapshot(snapshot, placement)?;
    let legacy_node_facets = snapshot.legacy_node_facet_count();
    let legacy_controls = crate::surface_controls::extract_legacy(snapshot);
    let controls_migrated = !legacy_controls.is_empty();
    let migrated = externalize_legacy_images(snapshot, data_root)
        .map_err(|error| format!("session imagery could not be migrated: {error}"))?;
    let mut graph = pandect::graph_placement::materialize_snapshot(snapshot, placement)?;
    match canonical {
        Some(canonical) if legacy_node_facets == 0 => {
            // A modern sidecar is the complete authority, including absent
            // facets. Reconstruction defaults must not invent a new revision.
            // Explicit old control labels still migrate; canonical false wins.
            let mut facets = legacy_controls;
            facets.overlay(canonical);
            graph.install_facets(facets).map_err(|error| {
                format!("canonical graph facets could not be restored: {error}")
            })?;
        },
        canonical => {
            // Genuine legacy columns and missing-sidecar inputs retain their
            // imported metadata until the existing migration persists it.
            graph.overlay_facets(legacy_controls).map_err(|error| {
                format!("legacy surface controls could not be restored: {error}")
            })?;
            if let Some(canonical) = canonical {
                graph.overlay_facets(canonical).map_err(|error| {
                    format!("canonical graph facets could not be restored: {error}")
                })?;
            }
        },
    }
    let before_migration = graph.clone();
    if legacy_node_facets > 0 || controls_migrated || migrated > 0 {
        // Evidence first, and propagate failure so the display runtime cannot
        // overwrite the source later through an automatic ordinary save.
        pandect::save_node_facets(data_root, graph.facets())
            .map_err(|error| format!("migration facets could not be persisted: {error}"))?;
        athanor::workshop_destination::preserve_graph_operations(&before_migration, &graph)
            .map_err(|error| format!("operation ledger retention refused migration: {error}"))?;
        crate::session_graph_file::save_profiled(&graph_file, &graph, placement)
            .map_err(|error| format!("migrated graph could not be persisted: {error}"))?;
    }
    Ok(Some(LoadedSessionGraph { graph, placement }))
}
