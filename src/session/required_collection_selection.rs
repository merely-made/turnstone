// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

// An explicit null clears the choice; an omitted field is corrupt, not a
// request to broaden the scope. Serde otherwise defaults missing Options.
pub(super) fn required_collection_selection<'de, D>(
    deserializer: D,
) -> Result<Option<crate::place::PlaceCollectionVersion>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(deserializer)
}
