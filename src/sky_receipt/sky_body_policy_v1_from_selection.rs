// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyBodyPolicyV1 {
    pub(super) fn from_selection(selection: SkyBodySelection) -> Self {
        Self {
            body: SkyBodyV1::from_body(selection.body()),
            rise_set: SkyRiseSetPolicyV1::from_policy(selection.rise_set_policy()),
        }
    }

    pub(super) fn validate(&self, index: usize) -> Result<(), SkyReceiptJsonError> {
        self.rise_set.validate(&format!("Sky body policy {index}"))
    }
}
