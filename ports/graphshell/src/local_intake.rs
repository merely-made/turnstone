// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Durable local intake, independent of a file chooser or browser form.
//!
//! Creation is journaled by the existing Mere host. A failed durable write
//! leaves that member pending in the same app; retry persists it without
//! creating another object.
use mere::canvas::Canvas;
use mere::kernel::graph::Graph;
use muniment::Backend;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::app::GraphshellApp;
use crate::local_edit::{NodeMetadata, metadata};
use crate::product::LocalFileMetadata;

pub fn create_address<B: Backend>(
    app: &mut GraphshellApp<B>,
    address: &str,
    title: &str,
) -> Result<Uuid, String> {
    let address = address.trim();
    if address.is_empty() {
        return Err("Enter an address before adding an object".into());
    }
    app.host
        .create_address(address, title.trim())
        .map_err(|error| error.to_string())
}

/// Reduce the chosen bytes to existing portable metadata. Empty files are
/// valid; the supplied filename must identify the chosen object.
pub fn file_metadata(
    name: &str,
    media_type: Option<&str>,
    last_modified_ms: Option<u64>,
    bytes: &[u8],
) -> Result<LocalFileMetadata, String> {
    if name.trim().is_empty() {
        return Err("The chosen file has no name".into());
    }
    let digest = Sha256::digest(bytes);
    Ok(LocalFileMetadata {
        content_hash: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
        name: name.to_string(),
        media_type: media_type.unwrap_or_default().to_string(),
        byte_len: bytes.len() as u64,
        last_modified_ms: last_modified_ms.unwrap_or_default(),
    })
}

pub fn create_file<B: Backend>(
    app: &mut GraphshellApp<B>,
    name: &str,
    media_type: Option<&str>,
    last_modified_ms: Option<u64>,
    bytes: &[u8],
) -> Result<Uuid, String> {
    let metadata = file_metadata(name, media_type, last_modified_ms, bytes)?;
    app.host
        .create_file_metadata(metadata)
        .map_err(|error| error.to_string())
}

/// Refresh the portable projection before writing, so a projection failure
/// cannot be reported as a save failure after storage has acknowledged it.
/// This also serves a retry over the same pending member.
pub async fn persist_intake<B: Backend>(
    app: &mut GraphshellApp<B>,
    member: Uuid,
    saved_at_secs: u64,
) -> Result<NodeMetadata, String> {
    let saved = metadata(app, member)?;
    app.mount_local().map_err(|error| error.to_string())?;
    app.host
        .persist(saved_at_secs)
        .await
        .map_err(|error| error.to_string())?;
    Ok(saved)
}

/// Reconcile the owner's acknowledged graph through the structural ingest
/// seam. Existing body positions, camera, roles, representations and physics
/// settings remain in the canvas; intake changes only the selected member.
/// Matching existing keys matters because canvas view state is keyed by them.
pub fn sync_canvas_intake(canvas: &mut Canvas, owner: &Graph, member: Uuid) -> Result<(), String> {
    if owner.get_node_by_id(member).is_none() {
        return Err("the saved object is no longer in the local graph".into());
    }
    for (key, node) in canvas.graph().nodes() {
        if owner.get_node_key_by_id(node.id) != Some(key) {
            return Err("the canvas no longer matches the local graph".into());
        }
    }
    canvas.ingest_graph(|graph| {
        *graph = owner.clone();
        true
    });
    canvas.set_selected_members(&[member]);
    Ok(())
}

#[cfg(test)]
mod tests;
