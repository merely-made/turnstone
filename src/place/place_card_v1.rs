// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A place's public calling card: where it is and who founded it.
///
/// Carries no authority whatsoever. It exists so someone who wants in can
/// name the Moot their pre-key belongs to; the invitation stays the only
/// envelope admission reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceCardV1 {
    pub version: u16,
    pub binding: PlaceBindingV1,
    /// The founder's Personae root, hex.
    pub founder_root: String,
    /// The founder's dialable contact hints at export time.
    pub rendezvous: Vec<invite::RendezvousV1>,
}
