// SPDX-License-Identifier: MPL-2.0
//! Public and member journals never exchange unwrapped hints.
use super::*;
pub(super) const MODES: [CatalogueVisibility; 2] =
    [CatalogueVisibility::Public, CatalogueVisibility::Members];
fn path(open: &OpenPlace, mode: CatalogueVisibility) -> PathBuf {
    open.directory.join(match mode {
        CatalogueVisibility::Public => "catalogue-hints.cbor",
        CatalogueVisibility::Members => "protected-catalogue-hints.cbor",
    })
}
pub(super) fn load(
    open: &OpenPlace,
    mode: CatalogueVisibility,
) -> Result<CatalogueDirectory, String> {
    let path = path(open, mode);
    if !path.exists() && path.with_extension("cbor.previous").exists() {
        return Err("catalogue journal needs recovery from its previous copy".into());
    }
    use std::io::Read;
    let bytes = match std::fs::File::open(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CatalogueDirectory::default());
        },
        Err(e) => return Err(e.to_string()),
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(protected::MAX_CACHE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            bytes
        },
    };
    if bytes.len() > protected::MAX_CACHE_BYTES {
        return Err("catalogue journal byte ceiling".into());
    }
    match mode {
        CatalogueVisibility::Public => {
            let d = CatalogueDirectory::decode(&bytes, MAX_ENTRIES)?;
            d.require_audience(None)?;
            Ok(d)
        },
        CatalogueVisibility::Members => protected::open_cache(
            open.binding.moot.0,
            &open.group.keys.current(),
            &bytes,
            MAX_ENTRIES,
        )
        .map_err(|_| "protected catalogue journal could not be opened".to_string()),
    }
}
pub(super) fn save(
    open: &OpenPlace,
    mode: CatalogueVisibility,
    directory: &CatalogueDirectory,
) -> Result<(), String> {
    let bytes = match mode {
        CatalogueVisibility::Public => {
            directory.require_audience(None)?;
            directory.encode()?
        },
        CatalogueVisibility::Members => {
            protected::seal_cache(open.binding.moot.0, &open.group.keys.current(), directory)?
        },
    };
    muniment::atomic_file::write_bytes_with_backup(&path(open, mode), &bytes)
        .map_err(|e| e.to_string())
}
pub(super) fn wire_bytes(
    open: &OpenPlace,
    hint: &SignedCatalogueAdvertisement,
) -> Result<Vec<u8>, String> {
    if hint.advertisement.audience.is_some() {
        protected::seal_advertisement(open.binding.moot.0, &open.group.keys.current(), hint)
    } else {
        hint.encode()
    }
}
pub(super) fn retain(
    open: &OpenPlace,
    directory: &mut CatalogueDirectory,
    hint: &SignedCatalogueAdvertisement,
    now: u64,
) -> Result<(), String> {
    let bytes = wire_bytes(open, hint)?;
    if hint.advertisement.audience.is_some() {
        pollster::block_on(directory.receive_protected(
            &open.moot,
            open.subject,
            &bytes,
            &open.group.keys.current(),
            now,
            MAX_LIFETIME_MS,
            MAX_ENTRIES,
        ))?;
    } else {
        pollster::block_on(directory.receive(
            &open.moot,
            open.subject,
            &bytes,
            now,
            MAX_LIFETIME_MS,
            MAX_ENTRIES,
        ))?;
    }
    Ok(())
}
pub(super) fn outgoing(
    open: &OpenPlace,
    provider: [u8; 32],
    now: u64,
) -> Result<Vec<SignedCatalogueAdvertisement>, String> {
    let mut hints = Vec::new();
    for mode in MODES {
        hints.extend(load(open, mode)?.outgoing(provider, now));
    }
    Ok(hints)
}
pub(crate) fn refresh(open: &OpenPlace, now: u64) -> Result<Vec<DiscoveredSite>, String> {
    let result = refresh_inner(open, now);
    if result.is_err() {
        if let Some(lanes) = &open.lanes {
            lanes
                .catalogue_clock
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }
    }
    result
}
fn refresh_inner(open: &OpenPlace, now: u64) -> Result<Vec<DiscoveredSite>, String> {
    let mut public = load(open, CatalogueVisibility::Public)?;
    let mut members = load(open, CatalogueVisibility::Members)?;
    let (mut public_changed, mut members_changed) = (false, false);
    if let Some(lanes) = &open.lanes {
        for bytes in lanes.catalogue.drain() {
            let member = protected::is_protected(&bytes);
            let outcome = if member {
                pollster::block_on(members.receive_protected(
                    &open.moot,
                    open.subject,
                    &bytes,
                    &open.group.keys.current(),
                    now,
                    MAX_LIFETIME_MS,
                    MAX_ENTRIES,
                ))
            } else {
                pollster::block_on(public.receive(
                    &open.moot,
                    open.subject,
                    &bytes,
                    now,
                    MAX_LIFETIME_MS,
                    MAX_ENTRIES,
                ))
            };
            match outcome {
                Ok(changed) => {
                    if member {
                        members_changed |= changed
                    } else {
                        public_changed |= changed
                    }
                },
                Err(_) if member => tracing::debug!("protected catalogue hint refused"),
                Err(e) => tracing::debug!(%e,"catalogue hint refused"),
            }
        }
        for hint in public
            .outgoing(open.subject, now)
            .into_iter()
            .chain(members.outgoing(open.subject, now))
        {
            if pollster::block_on(open.moot.validate_catalogue_advertisement(
                open.subject,
                &hint,
                now,
                MAX_LIFETIME_MS,
            ))
            .is_ok()
            {
                // Protected intent is resealed to the current epoch on every retry.
                // An old recipient cannot open later rotations, and no plaintext
                // is sent when a key is missing or stale.
                lanes.publish_catalogue(wire_bytes(open, &hint)?)?;
            }
        }
    }
    if public_changed {
        save(open, CatalogueVisibility::Public, &public)?;
    }
    if members_changed {
        save(open, CatalogueVisibility::Members, &members)?;
    }
    let mut rows = Vec::new();
    for (mode, directory) in [
        (CatalogueVisibility::Public, &public),
        (CatalogueVisibility::Members, &members),
    ] {
        for hint in
            pollster::block_on(directory.visible(&open.moot, open.subject, now, MAX_LIFETIME_MS))
        {
            let digest = CatalogueDirectory::digest(&hint)?;
            let ad = hint.advertisement;
            let d = ad.description.ok_or("visible description missing")?;
            rows.push(DiscoveredSite {
                visibility: mode,
                digest,
                provider: ad.provider,
                collection: ad.collection.collection.collection_id.0,
                sequence: ad.sequence,
                address: d.address,
                title: d.title,
                abstract_text: d.abstract_text,
                scroll_subject: d.scroll_subject,
                expires_at_ms: ad.expires_at_ms,
            });
        }
    }
    if let Some(lanes) = &open.lanes {
        let active = !rows.is_empty()
            || !public.outgoing(open.subject, now).is_empty()
            || !members.outgoing(open.subject, now).is_empty();
        lanes
            .catalogue_clock
            .store(active, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(rows)
}
