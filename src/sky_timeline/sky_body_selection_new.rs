// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyBodySelection {
    pub fn new(body: ApparentBody, rise_set_policy: ConventionalRiseSetPolicy) -> Self {
        Self {
            body,
            rise_set_policy,
        }
    }

    pub fn body(self) -> ApparentBody {
        self.body
    }

    pub fn rise_set_policy(self) -> ConventionalRiseSetPolicy {
        self.rise_set_policy
    }
}
