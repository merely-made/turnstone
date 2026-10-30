// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn scoped_reads(
    table: &servitor::WatchTable,
) -> HashMap<Subject, Vec<(servitor::Cap, Mode)>> {
    let mut reads = HashMap::new();
    for watch in table.watches() {
        reads
            .entry(watch.subject)
            .or_insert_with(Vec::new)
            .push((servitor::Cap::Scope(watch.scope.clone()), Mode::Read));
    }
    reads
}
