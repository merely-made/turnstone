// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
use super::*;
use mere::query_view::{QueryProjection, QueryViewSpec};
impl SavedProduct {
    pub(in super::super) fn save_query(
        &mut self,
        scope: &(Uuid, String),
        owner: Option<Uuid>,
        expected: Option<&QueryViewSpec>,
        spec: &QueryViewSpec,
    ) -> Result<(Uuid, Uuid), String> {
        if self.selection_locked() || self.retained_scope().as_ref() != Some(scope) {
            return Err("Query draft belongs to another owner or a pending save".into());
        }
        let app = self.app.as_mut().ok_or("Source unavailable")?;
        let id = graphshell::query_views::save(app, owner, expected, Uuid::new_v4(), spec)?;
        let graph = app.host.graph();
        let iri = graph
            .resource(id)
            .ok_or("Saved view Resource missing")?
            .canonical_iri()
            .to_owned();
        let access = graph.surface_ids_showing_resource(id).first().copied();
        let access = match access {
            Some(id) => id,
            None => create_address(app, &iri, &spec.title)?,
        };
        self.operation_success = Some("Query view saved".into());
        self.persist_operation();
        Ok((id, access))
    }
    pub(in super::super) fn freeze_query(
        &mut self,
        scope: &(Uuid, String),
        projection: &QueryProjection,
    ) -> Result<Uuid, String> {
        if self.selection_locked() || self.retained_scope().as_ref() != Some(scope) {
            return Err("Query view belongs to another owner or a pending save".into());
        }
        let app = self.app.as_mut().ok_or("Source unavailable")?;
        if !projection.is_current(app.host.graph()) {
            return Err("Refresh the query view before freezing".into());
        }
        let frozen = graphshell::query_views::freeze(app, projection.owner, Uuid::new_v4())?;
        self.operation_success = Some("Query selection frozen as a Resource".into());
        self.persist_operation();
        Ok(frozen)
    }
}
