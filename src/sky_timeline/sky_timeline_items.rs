// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyTimeline {
    pub fn items(&self) -> &[SkyTimelineItem] {
        &self.items
    }

    pub fn receipt(&self) -> &SkyDerivationReceipt {
        &self.receipt
    }

    pub fn local_time(&self, item: &SkyTimelineItem) -> Result<Zoned, SkyLocalTimeError> {
        self.receipt.day.local_time(item.instant())
    }
}
