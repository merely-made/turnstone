// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Control {
    pub(super) fn intent(self) -> &'static str {
        match self {
            Self::Pause => wire::PAUSE_INTENT,
            Self::Resume => wire::RESUME_INTENT,
            Self::Cancel => wire::CANCEL_INTENT,
        }
    }
}
