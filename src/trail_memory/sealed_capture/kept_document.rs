// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use eidetic::models::OpaqueBlob;
use eidetic::{ManifestId, Store};

use super::{CaptureRequest, FetchedDocument, HostError, PayloadSealer, StoredSourceDocument};
use super::{sealed, verify};
use crate::source_capture::SourceIdentity;

const MAX_HTML_BYTES: u64 = 2 * 1024 * 1024;

/// Reopen the exact response of an app-admitted saved capture under the trail
/// actor's current custody. Neither a live fetch nor the canonical prose from
/// the annotation can substitute for the original decoded HTML.
pub(in super::super) fn read_kept_document(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    request: &CaptureRequest,
    held: &StoredSourceDocument,
) -> Result<FetchedDocument, HostError> {
    if !request.keep {
        return Err("Keep this source before validating it".into());
    }
    if !held.annotation_manifest.starts_with(sealed::SEALED_TAG) {
        return Err(
            "Validation needs a source saved under the current sealed capture format".into(),
        );
    }
    let identity = request
        .source
        .as_ref()
        .ok_or("Saved source has no response identity")?;
    let raw = sealed::parse_id(&identity.hash)?;
    let body = sealed::parse_id(&identity.body_hash)?;

    // Check the declared response sizes before the existing verifier or the
    // source read opens the sealed blobs. The reader verifies those same
    // declarations, their typed roles and digests after decryption.
    pollster::block_on(check_html_size(store, raw))?;
    if raw != body {
        pollster::block_on(check_html_size(store, body))?;
    }
    verify(store, sealer, request, held)?;
    let OpaqueBlob(bytes) = pollster::block_on(sealed::load(store, sealer, raw))?;
    let body_bytes = if body == raw {
        bytes.clone()
    } else {
        let OpaqueBlob(bytes) = pollster::block_on(sealed::load(store, sealer, body))?;
        bytes
    };
    let body = String::from_utf8(body_bytes).map_err(|_| "Saved HTML body is not UTF-8")?;
    let document = FetchedDocument {
        bytes,
        body,
        effective_url: identity.effective_url.clone(),
        content_type: identity.content_type.clone(),
        acquired_at_ms: identity.acquired_at_ms,
    };
    if SourceIdentity::of(&document) != *identity {
        return Err("Saved HTML response differs from the capture identity".into());
    }
    Ok(document)
}

async fn check_html_size(store: &mut dyn Store, id: ManifestId) -> Result<(), HostError> {
    let manifest = eidetic::manifest::load_manifest(store, id)
        .await
        .map_err(|error| HostError::from(error).context("saved source HTML manifest"))?
        .ok_or("Saved source HTML is missing")?;
    if manifest.byte_size > MAX_HTML_BYTES {
        return Err("Saved source exceeds the 2 MiB HTML limit".into());
    }
    Ok(())
}
