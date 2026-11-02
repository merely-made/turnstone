// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn collection_id_suffix(
    version: &crate::place::PlaceCollectionVersion,
    choices: &[crate::place::PlaceCollectionChoice],
    name: &str,
) -> String {
    let hex = collection_id_hex(&version.collection);
    for length in (8..=hex.len()).step_by(2) {
        let prefix = &hex[..length];
        if choices
            .iter()
            .filter(|choice| choice.name == name)
            .all(|choice| {
                choice.version.collection == version.collection
                    || !collection_id_hex(&choice.version.collection).starts_with(prefix)
            })
        {
            return prefix.to_string();
        }
    }
    hex
}
