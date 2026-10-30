// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ScopeBookmark {
    pub fn validate(&self) -> bool {
        self.reading.as_ref().is_none_or(|a| a.validate().is_ok())
    }
}
