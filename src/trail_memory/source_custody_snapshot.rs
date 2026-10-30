// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SourceCustody {
    pub(super) fn snapshot(&self) -> Option<u64> {
        (self.epoch)()
    }

    pub(super) fn remains_current(&self, epoch: Option<u64>) -> bool {
        epoch.is_some() && self.snapshot() == epoch
    }
}
