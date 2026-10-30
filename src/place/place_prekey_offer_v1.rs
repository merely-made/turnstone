// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One profile's published group pre-key for one Moot.
///
/// Also authority-free: the bundle carries its own Personae attestation, and
/// the inviter re-derives the root from it rather than trusting `root` here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacePrekeyOfferV1 {
    pub version: u16,
    pub moot: PlaceId,
    /// The offering profile's Personae root, hex.
    pub root: String,
    /// The published pre-key bundle, base64.
    pub prekey: String,
}
