// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Graphshell's host-owned Forme preferences and Canvas projection adapter.
use mere::kernel::graph::Graph;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

pub type FormeWorkspace = platen::workspace::FormeWorkspace<GraphshellFormePreferences>;
pub type GraphshellFormeSession = platen::workspace::FormeSession<GraphshellFormePreferences>;
pub type GraphshellSaveKind = platen::workspace::SaveKind<GraphshellFormePreferences>;
pub type GraphshellPreparedSave = platen::workspace::PreparedSave<GraphshellFormePreferences>;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GraphshellFormePreferences {
    #[serde(default)]
    pub inspector_source: crate::pane_source::InspectorSource,
    #[serde(default)]
    pub focused_reading: Option<Uuid>,
    #[serde(default)]
    pub reading_preferences: tabard::ReadingProjections<Uuid>,
    #[serde(default)]
    pub reading_anchors: BTreeMap<Uuid, tabard::ReadingAnchor>,
    #[serde(default)]
    pub reading_trail: tabard::ReadingTrail<Uuid>,
    #[serde(default)]
    pub scope_navigation: crate::scope_policy::ScopeNavigation<tabard::ReadingAnchor>,
}
impl platen::workspace::WorkspacePreferences for GraphshellFormePreferences {
    fn focused_reading(&self) -> Option<Uuid> {
        self.focused_reading
    }
    fn set_focused_reading(&mut self, member: Option<Uuid>) {
        self.focused_reading = member;
    }
    fn visit_reading(&mut self, member: Uuid) {
        self.reading_trail.visit(member);
    }
    fn reading_after_close(&self, members: &[Uuid], stack: &[Uuid], closed: Uuid) -> Option<Uuid> {
        cambium::scope_navigation::reading_after_close_in_stack(
            members,
            stack,
            self.focused_reading.as_ref(),
            &closed,
        )
    }
    fn validate(&self, graph_id: Uuid, members: &[Uuid]) -> Result<(), String> {
        self.reading_trail.validate()?;
        self.scope_navigation.validate(
            |place| {
                place.valid()
                    && place.graph == graph_id
                    && place.parents().iter().all(|p| p.graph == graph_id)
            },
            |anchor| anchor.validate().is_ok(),
        )?;
        if self.reading_anchors.len() > 1024 {
            return Err("Too many reading anchors".into());
        }
        for (access, anchor) in &self.reading_anchors {
            if !members.contains(access) {
                return Err("Reading anchor has no projection".into());
            }
            anchor.validate()?;
        }
        self.reading_preferences.validate()?;
        if self
            .reading_preferences
            .keys()
            .any(|access| !members.contains(access))
        {
            return Err("Reading preferences have no projection".into());
        }
        Ok(())
    }
}

pub fn slot(graph: Uuid) -> String {
    format!("graphshell.forme-workspace/v1/{graph}")
}

pub(crate) fn canvas_region(
    region: Option<platen::workspace::WorkspaceRegion>,
) -> Option<mere::canvas::FormeRegion> {
    region.map(|region| mere::canvas::FormeRegion {
        id: region.id,
        bounds: region.bounds,
        cells: region
            .cells
            .into_iter()
            .map(|cell| mere::canvas::FormeCell {
                member: cell.member,
                bounds: cell.bounds,
            })
            .collect(),
        locked: region.locked,
        visible: region.visible,
    })
}

pub fn project_region(
    workspace: &FormeWorkspace,
    owner: Uuid,
    graph: &Graph,
) -> Result<Option<mere::canvas::FormeRegion>, String> {
    workspace.project_region(owner, graph).map(canvas_region)
}

#[cfg(test)]
#[path = "forme_session_lifecycle_tests.rs"]
mod lifecycle_tests;
