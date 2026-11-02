// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl DiskRun {
    pub(super) fn reducer(&self) -> Result<RunReducer, String> {
        let mut r = RunReducer::new(self.header.to_header(RunId(self.id))?)
            .map_err(|e| format!("resident run header refused: {e:?}"))?;
        for event in self.events.iter().copied() {
            r.apply(event.to_event()?)
                .map_err(|e| format!("resident run event is corrupt: {e:?}"))?;
        }
        Ok(r)
    }
}
