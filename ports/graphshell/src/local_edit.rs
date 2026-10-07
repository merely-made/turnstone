// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Typed local detail edits, independent of a browser form or presenter.
use mere::canvas::Canvas;
use muniment::Backend;
use uuid::Uuid;

use crate::app::GraphshellApp;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeMetadata {
    pub member: Uuid,
    pub address: String,
    pub title: String,
    pub tags: Vec<String>,
}

pub fn metadata<B: Backend>(app: &GraphshellApp<B>, member: Uuid) -> Result<NodeMetadata, String> {
    let (_, node) = app
        .host
        .graph()
        .get_node_by_id(member)
        .ok_or_else(|| format!("selected object {member} no longer exists"))?;
    let mut tags = node.tags.iter().cloned().collect::<Vec<_>>();
    tags.sort();
    Ok(NodeMetadata {
        member,
        address: node.url().to_string(),
        title: node.title.clone(),
        tags,
    })
}

/// Save the selected object's metadata. Errors leave pending in-memory edits
/// available for retry; callers must not describe a failed write as saved.
pub async fn save_metadata<B: Backend>(
    app: &mut GraphshellApp<B>,
    member: Uuid,
    title: &str,
    tags: &str,
    saved_at_secs: u64,
) -> Result<NodeMetadata, String> {
    metadata(app, member)?;
    app.host
        .edit_node(
            member,
            title,
            tags.split(',').map(str::trim).map(str::to_owned),
        )
        .map_err(|error| error.to_string())?;
    // Refresh the portable projection before the durable write. A projection
    // failure must not become a misleading save failure after bytes committed.
    app.mount_local().map_err(|error| error.to_string())?;
    let saved = metadata(app, member)?;
    app.host
        .persist(saved_at_secs)
        .await
        .map_err(|error| error.to_string())?;
    Ok(saved)
}

/// Stamp the saved metadata into the canvas projection through its existing
/// metadata seams. Geometry, camera, selection and live physics remain owned
/// by this canvas; this does not switch it to a different graph.
pub fn sync_canvas_metadata(canvas: &mut Canvas, saved: &NodeMetadata) -> Result<(), String> {
    let old = canvas
        .graph()
        .get_node_by_id(saved.member)
        .ok_or_else(|| "saved object is no longer in the canvas".to_string())?
        .1
        .tags
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    canvas.set_node_title_for(saved.member, saved.title.clone());
    for tag in old.iter().filter(|tag| !saved.tags.contains(tag)) {
        canvas.untag_node(saved.member, tag);
    }
    for tag in &saved.tags {
        canvas.tag_node(saved.member, tag);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
