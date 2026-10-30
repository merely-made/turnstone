// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn open_cached_place(
    directory: &Path,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    settings: &PlaceWorkerSettings,
) -> Result<(OpenPlace, OfflinePlaceSnapshot), String> {
    binding
        .validate()
        .map_err(|error| format!("place binding: {error}"))?;
    let stores = place_store_dir(directory);
    // Not migrated: an old place's peers still speak the plaintext graph lane,
    // so opening it encrypted would show an empty graph that never fills.
    if stores.join(LEGACY_GRAPH_STORE).exists() && !stores.join(GRAPH_STORE).exists() {
        return Err(LEGACY_GRAPH_REFUSAL.to_string());
    }
    let group = PlaceGroup::open(directory, identity, binding.moot.0)?;

    let moot = pollster::block_on(MootFile::open_existing(
        stores.join("gemot"),
        MootId(binding.moot.0),
        settings.retention.clone(),
    ))
    .map_err(|error| format!("open Gemot cache: {error}"))?;

    let graph_backend = RedbBackend::open(stores.join(GRAPH_STORE))
        .map_err(|error| format!("open Commons graph cache: {error}"))?;
    let graph =
        EncryptedReplica::for_identity(graph_backend, binding.root.0, identity, group.keys.clone())
            .map_err(|error| format!("bind Commons graph writer: {error}"))?;

    let chat_backend = RedbBackend::open(stores.join("commons-chat.redb"))
        .map_err(|error| format!("open Commons chat cache: {error}"))?;
    let chat =
        ChatReplica::for_identity(chat_backend, binding.chat.0, identity, group.keys.clone())
            .map_err(|error| format!("bind Commons chat writer: {error}"))?;

    let mut open = OpenPlace {
        directory: directory.to_path_buf(),
        lanes: None,
        binding: binding.clone(),
        subject: identity.master_public_key().to_bytes(),
        capture_directory: crate::trail_memory::memory_dir(directory),
        collection_selection: crate::session::load_place_collection(directory)?,
        moot,
        graph,
        chat,
        group,
        readmitted: 0,
    };
    open.follow_group_keys(identity)?;
    let snapshot = place_snapshot(&open, binding.moot.0, settings)?;
    Ok((open, snapshot))
}
