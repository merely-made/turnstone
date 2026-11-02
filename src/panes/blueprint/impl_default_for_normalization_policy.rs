// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for NormalizationPolicy {
    fn default() -> Self {
        Self {
            collapse_single_child: true,
            join_same_axis_splits: true,
            prune_unknown_panes: true,
        }
    }
}
