// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn fixture_authority(
    owner: &Owner,
    installation: [u8; 16],
    human: [u8; 32],
    sources: &[alembic::workshop::Source],
    destination: Option<&alembic::workshop::Destination>,
    now_ms: u64,
) -> servitor::GrantTable {
    use servitor::{AuthorityProvider, Grant, GrantTable, Mode, Subject};
    let helper = helper_subject(owner);
    let mut table = GrantTable::new();
    for source in sources {
        table.grant(Grant::new(
            helper,
            athanor::workshop::read_cap(source),
            Mode::Read,
        ));
    }
    table.grant(Grant::new(
        helper,
        athanor::workshop::proposal_cap(owner, installation),
        Mode::Write,
    ));
    if let Some(destination) = destination {
        table.grant(Grant::new(
            Subject(human),
            athanor::workshop::destination_cap(destination),
            Mode::Write,
        ));
    }
    table.set_now(now_ms);
    table
}
