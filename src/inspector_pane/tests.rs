// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::action::Action;
use layout_dom_api::LayoutDom;
fn content_lines(pane: &InspectorPane) -> Vec<String> {
    pane.runner
        .state()
        .sections
        .iter()
        .filter(|section| section.title == "Content")
        .flat_map(|section| {
            section
                .rows
                .iter()
                .map(|(key, value)| format!("{key}: {value}"))
        })
        .collect()
}

struct AddedDocumentEngine;

impl inker::SessionEngine<netrender::Scene> for AddedDocumentEngine {
    fn engine_id(&self) -> &str {
        "example.document"
    }

    fn spawn(
        &self,
        _: &inker::SessionSpawnRequest,
    ) -> Result<Box<dyn inker::DocumentSession<netrender::Scene>>, inker::SessionError> {
        Err(inker::SessionError::SpawnFailed(
            "inventory fixture does not spawn".into(),
        ))
    }
}

#[path = "tests/engine_inventory_saved_unknown_is_selected_disabled_and_cases.rs"]
mod engine_inventory_saved_unknown_is_selected_disabled_and_cases;
#[path = "tests/synced_sections_reach_the_dom_cases.rs"]
mod synced_sections_reach_the_dom_cases;
