// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The public, durable binding between one Turnstone session and one governed
/// shared place.
///
/// These ids remain distinct because the live domain APIs address different
/// replicated spaces with them. Knot documents are addressed nodes inside the
/// shared graph and therefore need no fourth root here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceBindingV1 {
    pub version: u16,
    pub moot: PlaceId,
    pub root: SharedContainerId,
    pub chat: ChatSpaceId,
    pub default_channel: String,
}
