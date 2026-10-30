// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Sorted: the shared graph is a set, not an insertion order.
pub(super) fn addresses(snapshot: &crate::place::OfflinePlaceSnapshot) -> Vec<String> {
    let mut addresses: Vec<String> = snapshot.shared.addresses().map(str::to_string).collect();
    addresses.sort();
    addresses
}
