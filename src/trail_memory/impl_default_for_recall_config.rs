// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for RecallConfig {
    fn default() -> Self {
        Self {
            frecency_weight: DEFAULT_FRECENCY_WEIGHT,
        }
    }
}
