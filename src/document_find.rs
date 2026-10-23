// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use crate::action::{Action, DocumentFindDirection, DocumentFindModel};

/// What the mounted find field reported, as the Action it lowers to. Only the
/// field's own events arrive here; the reading toolbar's text controls mean
/// nothing to document find.
pub fn find_action(event: cambium::ReadEvent) -> Option<Action> {
    use cambium::ReadCommand as C;
    match event {
        cambium::ReadEvent::Query(input) => Some(Action::EditDocumentFind(input)),
        cambium::ReadEvent::Command(C::Next) => {
            Some(Action::StepDocumentFind(DocumentFindDirection::Next))
        },
        cambium::ReadEvent::Command(C::Previous) => {
            Some(Action::StepDocumentFind(DocumentFindDirection::Previous))
        },
        cambium::ReadEvent::Command(C::CloseFind) => Some(Action::CloseDocumentFind),
        cambium::ReadEvent::Command(C::Find) => Some(Action::OpenDocumentFind),
        cambium::ReadEvent::Command(
            C::Smaller | C::Larger | C::Narrower | C::Wider | C::Reset,
        ) => None,
    }
}

/// App-owned retained state for find in the active document. The target is
/// captured when the field opens; later focus changes cannot redirect a query.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocumentFindState {
    pub open: bool,
    pub target: Option<uuid::Uuid>,
    /// The field as the person edits it: text, caret, selection, preedit.
    pub input: cambium::TextInput,
    pub request: u64,
    pub pending: bool,
    pub model: DocumentFindModel,
    pub error: Option<String>,
}

impl DocumentFindState {
    pub fn query(&self) -> &str {
        self.input.text()
    }

    pub fn status(&self) -> String {
        if let Some(error) = &self.error {
            return cambium::find_failed(error);
        }
        let count = self.model.count;
        let searching = self.pending || !self.model.complete;
        let mut status =
            cambium::find_status(self.query().is_empty(), searching, count, self.model.current);
        if let Some(item) = self.model.current.and_then(|i| self.model.matches.get(i)) {
            if let Some(role) = &item.role {
                status.push_str(" · ");
                status.push_str(role);
            }
            if !item.label.is_empty() && item.label != self.query() {
                status.push_str(": ");
                status.push_str(&item.label);
            }
        }
        status
    }
}
