// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The one rule that withholds local file locations from an export.
//!
//! The typed address decides. A file and a directory both name a path on this
//! device, so both are local. The rule reaches every place a snapshot holds a
//! node's address: its primary address and legacy url, the resource the
//! address names, and the address and referring address of each access
//! record. A node that loses its address keeps its identity, title, tags,
//! content and surface relations: the empty address is the form a snapshot
//! already reads as "no address". A relation the node held through the
//! withheld resource goes with it, since that resource is the location.

use std::collections::HashSet;

use mere::kernel::address::{Address, address_from_url};
use mere::kernel::persistence::{GraphSnapshot, PersistedAddress};
use serde_json::Value;

use crate::access::{AccessHistory, AccessRecord};

/// Whether `address` names a file or directory on this device.
pub fn is_local_file_location(address: &str) -> bool {
    matches!(
        address_from_url(address),
        Address::File(_) | Address::Directory(_)
    )
}

/// Withhold the local locations a snapshot holds: each node's address and
/// legacy url are emptied, and a resource whose canonical iri is local goes
/// with the surfaces' link to it and the resource edges that touch it.
pub fn withhold_snapshot_locations(snapshot: &mut GraphSnapshot) {
    for node in &mut snapshot.nodes {
        if is_local_file_location(node.address.as_url_str()) {
            node.address = PersistedAddress::default();
        }
        if is_local_file_location(&node.url) {
            node.url.clear();
        }
    }
    let withheld: HashSet<String> = snapshot
        .resources
        .iter()
        .filter(|record| is_local_file_location(&record.canonical_iri))
        .map(|record| chartulary::resource_id_from_canonical_iri(&record.canonical_iri).to_string())
        .collect();
    snapshot
        .resources
        .retain(|record| !is_local_file_location(&record.canonical_iri));
    snapshot
        .shown_resources
        .retain(|shown| !withheld.contains(&shown.resource_id));
    snapshot.resource_edges.retain(|edge| {
        !withheld.contains(&edge.from_node_id) && !withheld.contains(&edge.to_node_id)
    });
}

/// The record without its local locations: none when the record is of a local
/// file, and its referring address cleared when that one is local.
pub fn withhold_access_record(mut record: AccessRecord) -> Option<AccessRecord> {
    if is_local_file_location(&record.address) {
        return None;
    }
    if record
        .referring_address
        .as_deref()
        .is_some_and(is_local_file_location)
    {
        record.referring_address = None;
    }
    Some(record)
}

/// A node's access-history facet without its local locations; none when no
/// record remains. A value that is not the current shape (the legacy one names
/// no addresses) is kept as it is.
pub fn withhold_access_history(value: &Value) -> Option<Value> {
    let Ok(history) = serde_json::from_value::<AccessHistory>(value.clone()) else {
        return Some(value.clone());
    };
    let records: Vec<_> = history
        .records
        .into_iter()
        .filter_map(withhold_access_record)
        .collect();
    (!records.is_empty())
        .then(|| serde_json::to_value(AccessHistory { records }).expect("history serializes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_and_directories_are_local_and_the_rest_are_not() {
        for local in [
            "file:///home/mark/notes.md",
            "FILE:///C:/Users/mark/notes.md",
            "file:///home/mark/notes/",
        ] {
            assert!(is_local_file_location(local), "{local}");
        }
        for held in [
            "https://example.test/file:///x",
            "data:text/plain,file:///x",
            "ni:///sha-256;abc",
            "mere://scene/one",
            "",
        ] {
            assert!(!is_local_file_location(held), "{held}");
        }
    }
}
