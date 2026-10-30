// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn latest_at_ms(run: &DiskRun) -> u64 {
    run.events
        .iter()
        .fold(run.header.started_at_ms, |latest, event| {
            latest.max(match event {
                DiskEvent::Intent { at_ms, .. }
                | DiskEvent::Result { at_ms, .. }
                | DiskEvent::Cancel { at_ms }
                | DiskEvent::Paused { at_ms }
                | DiskEvent::Resumed { at_ms }
                | DiskEvent::Interrupted { at_ms }
                | DiskEvent::BudgetExhausted { at_ms }
                | DiskEvent::Reconciled { at_ms, .. } => *at_ms,
            })
        })
}
