// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Codicil open and graph reconciliation around the canvas's saved scene
//! (`Canvas::capture_scene` and `Canvas::restore_scene`, shared with Turnstone).
use mere::canvas::Canvas;
use mere::kernel::graph::Graph;
use pandect::SavedSceneV3;

/// A successful import is authority even if its optional view cannot restore.
pub struct OpenedCodicil {
    pub receipt: crate::product::ImportReceipt,
    pub owner: uuid::Uuid,
    pub scene: Option<SavedSceneV3>,
    pub view_error: Option<String>,
}
/// Restore a view of an already accepted graph. Every return leaves Canvas
/// attached to that graph; no old graph-local keys survive a restoration error.
pub(crate) fn install_accepted_codicil(
    canvas: &mut Canvas,
    owner: &Graph,
    scene: Option<&SavedSceneV3>,
    viewport: (u32, u32),
) -> Option<String> {
    if let Some(scene) = scene {
        if let Err(error) = canvas.restore_scene(owner, scene, viewport) {
            canvas.set_graph(owner.clone());
            return Some(error);
        }
    } else {
        canvas.set_graph(owner.clone());
    }
    None
}
/// The retained Scene Open operation, shared with native lifecycle tests.
pub fn open_codicil<B: muniment::Backend + Clone>(
    host: &mut crate::mere_host::MereHost<B>,
    canvas: &mut Canvas,
    bytes: &[u8],
    viewport: (u32, u32),
) -> Result<OpenedCodicil, String> {
    let (receipt, scene) = host
        .replace_with_product_codicil(bytes)
        .map_err(|e| e.to_string())?;
    let owner = host.graph_session().id().0;
    let view_error = install_accepted_codicil(canvas, host.graph(), scene.as_ref(), viewport);
    Ok(OpenedCodicil {
        receipt,
        owner,
        scene,
        view_error,
    })
}

/// Install the returned App graph before Forme reconciliation, including a
/// retained in-memory edit whose persistence failed.
/// Same-owner stable keys retain view and physics state. Reused keys or a new
/// actual owner use the graph replacement path, which releases old local holds.
pub fn reconcile_completed_graph(
    canvas: &mut Canvas,
    previous_owner: Option<uuid::Uuid>,
    actual_owner: uuid::Uuid,
    graph: &Graph,
) {
    let keys_match = previous_owner == Some(actual_owner)
        && canvas.graph().nodes().all(|(key, node)| {
            graph
                .get_node_by_id(node.id)
                .is_none_or(|(next, _)| next == key)
                && graph.get_node(key).is_none_or(|next| next.id == node.id)
        });
    if !keys_match {
        canvas.set_graph(graph.clone());
    } else if canvas.refresh_same_topology_graph(graph).is_err() {
        canvas.ingest_graph(|current| {
            *current = graph.clone();
            true
        });
    }
}
