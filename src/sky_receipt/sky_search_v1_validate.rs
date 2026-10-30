// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkySearchV1 {
    pub(super) fn validate(&self) -> Result<(), SkyReceiptJsonError> {
        SkySearchControls::new(
            Duration::from_nanos(self.sample_step_nanoseconds),
            Duration::from_nanos(self.tolerance_nanoseconds),
        )
        .map(|_| ())
        .map_err(|source| invalid(format!("invalid Sky search controls: {source}")))
    }
}
