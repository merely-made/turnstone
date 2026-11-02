// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn authority_for_destination(
    owner: &Owner,
    installation: [u8; 16],
    human: [u8; 32],
    destination: &alembic::workshop::Destination,
    sources: &[alembic::workshop::Source],
    now_ms: u64,
) -> servitor::GrantTable {
    fixture_authority(
        owner,
        installation,
        human,
        sources,
        Some(destination),
        now_ms,
    )
}
