// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use crate::{
    peer_edition::Replay, research_storage::SourceReadError, source_capture::CaptureRequest,
};
use eidetic::{ManifestId, PayloadSealer, Store, capture::sealed, models::OpaqueBlob};
use fetch::peer::PeerAssets;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Serialize, Deserialize)]
struct Manifest {
    acquired_at_ms: u64,
    raw: String,
    document_url: String,
    assets: BTreeMap<String, ManifestId>,
}

fn index(raw: &str, url: &str, at: u64) -> Result<String, SourceReadError> {
    let identity = serde_json::to_vec(&(raw, url, at)).map_err(|e| e.to_string())?;
    Ok(format!(
        "turnstone/peer-edition/v1/{}",
        eidetic::Hash::of(&identity)
    ))
}

pub(super) fn keep(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    request: &CaptureRequest,
    bundle: Option<&PeerAssets>,
) -> Result<(), SourceReadError> {
    let source = request
        .source
        .as_ref()
        .ok_or("missing edition acquisition")?;
    let url = source
        .effective_url
        .as_deref()
        .ok_or("missing immutable edition address")?;
    let key = index(&source.hash, url, source.acquired_at_ms)?;
    let Some(bundle) = bundle else {
        let existing = load_manifest(store, sealer, &source.hash, url, source.acquired_at_ms)?;
        for blob in existing.assets.values() {
            let _: OpaqueBlob = pollster::block_on(sealed::load(store, sealer, *blob))?;
        }
        return Ok(());
    };
    if bundle.document_url != url {
        return Err("edition acquisition differs from the displayed page".into());
    }
    let mut assets = BTreeMap::new();
    for (url, bytes) in &bundle.resources {
        let id = pollster::block_on(sealed::save(
            store,
            sealer,
            &OpaqueBlob(bytes.clone()),
            source.acquired_at_ms,
        ))?;
        assets.insert(url.clone(), id);
    }
    let manifest = Manifest {
        acquired_at_ms: source.acquired_at_ms,
        raw: source.hash.clone(),
        document_url: bundle.document_url.clone(),
        assets,
    };
    let bytes = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
    let id = pollster::block_on(sealed::save(
        store,
        sealer,
        &OpaqueBlob(bytes),
        source.acquired_at_ms,
    ))?;
    // Publish only after the complete inventory is durable. The trail actor serializes writes.
    pollster::block_on(store.put(&key, id.to_string().as_bytes()))?;
    Ok(())
}

fn load_manifest(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    raw: &str,
    url: &str,
    at: u64,
) -> Result<Manifest, SourceReadError> {
    let reference = pollster::block_on(store.get(&index(raw, url, at)?))?
        .ok_or("edition inventory is unavailable")?;
    let id = sealed::parse_id(std::str::from_utf8(&reference).map_err(|e| e.to_string())?)?;
    let OpaqueBlob(bytes) = pollster::block_on(sealed::load(store, sealer, id))?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if manifest.document_url != url || manifest.acquired_at_ms != at || manifest.raw != raw {
        return Err("edition inventory belongs to another capture".into());
    }
    Ok(manifest)
}

pub(super) fn read(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    raw_id: &str,
    url: &str,
    at: u64,
    raw: Vec<u8>,
) -> Result<Replay, SourceReadError> {
    let manifest = load_manifest(store, sealer, raw_id, url, at)?;
    let body = String::from_utf8(raw).map_err(|e| e.to_string())?;
    let mut resources = BTreeMap::new();
    for (url, id) in manifest.assets {
        let OpaqueBlob(bytes) = pollster::block_on(sealed::load(store, sealer, id))?;
        resources.insert(url, bytes);
    }
    Ok(Replay {
        body,
        assets: Arc::new(PeerAssets {
            document_url: manifest.document_url,
            resources,
        }),
    })
}
