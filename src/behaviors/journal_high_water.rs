// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn journal_high_water(app: &App) -> u64 {
    match app.journal.lock() {
        Ok(journal) => journal.high_water(),
        Err(poisoned) => poisoned.into_inner().high_water(),
    }
}
