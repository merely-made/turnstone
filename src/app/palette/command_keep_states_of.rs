// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl CommandKeepStates {
    pub(crate) fn of(&self, label: &str) -> Option<bool> {
        self.registered
            .contains(label)
            .then(|| self.kept.contains(label))
    }
}
