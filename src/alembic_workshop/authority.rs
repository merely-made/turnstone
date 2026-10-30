// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn authority(
    owner: &Owner,
    installation: [u8; 16],
    human: [u8; 32],
    passages: &[Passage],
    now_ms: u64,
) -> servitor::GrantTable {
    fixture_authority(
        owner,
        installation,
        human,
        &passages
            .iter()
            .map(|p| p.source.clone())
            .collect::<Vec<_>>(),
        None,
        now_ms,
    )
}
