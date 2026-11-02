// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Browser-owned address admission includes public NomadNet destination/path
/// identifiers without registering a made-up URL scheme in shared fetch.
pub(crate) fn is_fetchable(address: &str) -> bool {
    fetch::is_fetchable(address) || crate::nomadnet::parse_address(address).is_some()
}
