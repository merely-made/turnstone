// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for FloatSizeConstraints {
    fn default() -> Self {
        Self {
            min_width: 0.0,
            min_height: 0.0,
            max_width: None,
            max_height: None,
        }
    }
}
