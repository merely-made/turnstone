// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl CommandCatalog {
    /// The id the command shown as `label` registered under.
    pub(crate) fn id_of(&self, label: &str) -> Option<&str> {
        self.ids.get(label).map(String::as_str)
    }

    pub(in super::super) fn action(&self, id: &str) -> Option<&Action> {
        self.actions.get(id)
    }
}
