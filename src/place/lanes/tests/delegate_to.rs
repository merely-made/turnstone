// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Issue a capability delegation on the host's retained delegation lane.
pub(super) fn delegate_to(
    host: &Path,
    founder: &InMemoryProvider,
    moot: [u8; 32],
    subject: [u8; 32],
) {
    let rules = place_rules(founder.master_public_key().to_bytes(), moot);
    let moot_file = pollster::block_on(gemot::moot::MootFile::open_existing(
        place_store_dir(host).join("gemot"),
        gemot::moot::MootId(moot),
        settings().retention,
    ))
    .unwrap();
    pollster::block_on(moot_file.delegation_store().author_issue(
        &founder_signing_key(founder, moot),
        &rules,
        place_delegation(founder, moot, subject),
    ))
    .unwrap();
}
