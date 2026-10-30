// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Page {
    pub(super) fn record(&self, call: impl Into<String>) -> Result<(), SurfaceError> {
        self.calls.lock().unwrap().push(call.into());
        Ok(())
    }
}
