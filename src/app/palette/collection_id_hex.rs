// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn collection_id_hex(collection: &crate::place::PlaceCollectionId) -> String {
    collection
        .0
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
