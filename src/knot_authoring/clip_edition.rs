// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use gemot_site::clip::{ClipPayloadV1, clip_body};
use gemot_site::editions::{EditionOperation, EditionWriter};

/// Provenance `source` for editions this engine signs.
const CLIP_TOOL: &str = "turnstone";

/// A clip as the first edition of a fresh log, with its payload bytes.
///
/// The edition names the payload by digest and records the producing engine
/// and time; the payload keeps the source URL, title and selector, so the
/// clip's provenance travels with every fork. The caller supplies the writer:
/// which key signs (and where it is held) is the release-log key-tier
/// decision, not this engine's.
pub fn clip_edition(
    source_url: &str,
    title: Option<&str>,
    selector: Option<&str>,
    knot_body: &str,
    writer: &mut EditionWriter,
    created_ms: u64,
) -> Result<(EditionOperation, Vec<u8>), String> {
    let payload = ClipPayloadV1::new(source_url, title, selector, knot_body);
    let body = clip_body(&payload, CLIP_TOOL, created_ms).map_err(|e| e.to_string())?;
    let operation = writer.append(&body).map_err(|e| e.to_string())?;
    Ok((operation, payload.encode().map_err(|e| e.to_string())?))
}
