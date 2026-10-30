// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Preserve an existing typed role exactly. The trail actor serializes these
/// checks with subsequent writes; no transactional backend guarantee is assumed.
pub(super) fn preflight_source_artifact(
    store: &mut dyn eidetic::Store,
    id: eidetic::ManifestId,
    schema: eidetic::SchemaRef,
    privacy: eidetic::PrivacyClass,
    expected: Option<&[u8]>,
) -> Result<bool, String> {
    let manifest = pollster::block_on(eidetic::manifest::load_manifest(store, id))
        .map_err(|error| format!("source artifact preflight manifest: {error}"))?;
    if let Some(manifest) = &manifest {
        if manifest.id != id
            || manifest.content_hash != id.0
            || manifest.schema != schema
            || manifest.privacy != privacy
        {
            return Err(format!(
                "source artifact preflight: incompatible existing manifest role at {id}"
            ));
        }
    }
    let key = format!("blob:{}", id.0.to_hex());
    let bytes = pollster::block_on(store.get(&key))
        .map_err(|error| format!("source artifact preflight payload: {error}"))?;
    if let Some(bytes) = &bytes {
        if eidetic::Hash::of(bytes) != id.0 || expected.is_some_and(|expected| expected != bytes) {
            return Err(format!(
                "source artifact preflight: payload integrity mismatch at {id}"
            ));
        }
    }
    if let Some(manifest) = &manifest {
        let bytes = bytes
            .as_ref()
            .ok_or_else(|| format!("source artifact preflight: missing local payload at {id}"))?;
        if manifest.byte_size != bytes.len() as u64
            || !manifest.sources.iter().any(|source| matches!(source, eidetic::BlobSource::Local { key: local } if local == &key))
        {
            return Err(format!("source artifact preflight: inconsistent local payload at {id}"));
        }
    }
    Ok(manifest.is_some())
}
