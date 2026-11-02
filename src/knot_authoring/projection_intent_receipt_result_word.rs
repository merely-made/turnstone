// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ProjectionIntentReceipt {
    pub(super) fn result_word(&self) -> &'static str {
        match &self.result {
            IntentResult::Accepted => "accepted",
            IntentResult::Rejected { .. } => "rejected",
            IntentResult::Stale { .. } => "stale",
        }
    }

    pub(super) fn line(&self) -> String {
        format!(
            "Projection intent {}: {} {}",
            self.target.0,
            self.intent,
            self.result_word()
        )
    }
}
