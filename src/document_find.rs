// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use crate::action::DocumentFindModel;

/// App-owned retained state for find in the active document. The target is
/// captured when the field opens; later focus changes cannot redirect a query.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocumentFindState {
    pub open: bool,
    pub target: Option<uuid::Uuid>,
    pub query: String,
    pub request: u64,
    pub pending: bool,
    pub model: DocumentFindModel,
    pub error: Option<String>,
}

impl DocumentFindState {
    pub fn status(&self) -> String {
        if let Some(error) = &self.error {
            return format!("Find failed: {error}");
        }
        if self.query.is_empty() {
            return "Type to find".into();
        }
        let count = self.model.count;
        if count == 0 {
            return if self.pending || !self.model.complete {
                "Searching".into()
            } else {
                "0 matches".into()
            };
        }
        let Some(current) = self.model.current else {
            return format!("{count} matches");
        };
        let ordinal = current + 1;
        let mut status = format!("{ordinal} of {count}");
        if let Some(item) = self.model.matches.get(current) {
            if let Some(role) = &item.role {
                status.push_str(" · ");
                status.push_str(role);
            }
            if !item.label.is_empty() && item.label != self.query {
                status.push_str(": ");
                status.push_str(&item.label);
            }
        }
        status
    }
}
