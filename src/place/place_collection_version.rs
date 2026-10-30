// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// App-owned form of one exact Gemot collection version.
///
/// The worker converts this at the Gemot boundary. Keeping it here prevents
/// durable app state and surfaces from acquiring domain-crate types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceCollectionVersion {
    pub moot: PlaceId,
    pub collection: PlaceCollectionId,
    pub frontier: Vec<[u8; 32]>,
    pub membership_commitment: [u8; 32],
}
