// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// A place-held document's address: `knot://<holder root hex>/<path>`.
///
/// The host slot is what makes the form decidable without a lookup. A local
/// vault discloses `knot://vault/<name>` and directory mode discloses
/// `file:///<absolute path>`; both name a document only this profile can
/// reach, so neither can be shared as-is. Sixty-four hex characters in the
/// host say WHO holds it, and the rest is the path inside the vault that
/// holder serves. `is_knot_address` matches it already, so the ordinary open
/// route carries it without a second scheme.
pub fn place_held_address(holder_root: &[u8; 32], path: &str) -> String {
    format!(
        "knot://{}/{}",
        crate::place::hex32(holder_root),
        path.trim_start_matches('/')
    )
}
