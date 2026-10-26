// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
use super::*;
use graphshell::query_views::QueryFreezeCorrelation;
use mere::query_view::QueryProjection;
use mere::query_view::editor::{Completion, RequestId, SaveRequest, SaveResult, Scope};

/// The owner keeps this across an unconfirmed store write. Retry persists the
/// same operation; it never repeats query creation or allocates another access.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum PendingQuery {
    Save {
        request: SaveRequest,
        owner: Uuid,
        access: Uuid,
    },
    Freeze {
        correlation: QueryFreezeCorrelation,
    },
}
pub(in super::super) enum QueryCompletion {
    Save(Completion),
    Freeze {
        correlation: QueryFreezeCorrelation,
        result: Result<(), String>,
    },
}
impl SavedProduct {
    pub(in super::super) fn take_query_completion(&mut self) -> Option<QueryCompletion> {
        self.query_completed.pop_front()
    }
    pub(in super::super) fn query_completion_ready(&self) -> bool {
        !self.query_completed.is_empty()
    }
    pub(in super::super) fn pending_query_id(&self) -> Option<RequestId> {
        match &self.pending_operation {
            Some(PendingOperation::Query(PendingQuery::Save { request, .. })) => Some(request.id),
            _ => None,
        }
    }
    pub(in super::super) fn pending_query_freeze(&self) -> Option<QueryFreezeCorrelation> {
        match &self.pending_operation {
            Some(PendingOperation::Query(PendingQuery::Freeze { correlation })) => {
                Some(correlation.clone())
            },
            _ => None,
        }
    }
    pub(in super::super) fn save_query(&mut self, request: SaveRequest) -> Result<(), String> {
        if self.selection_locked() || self.retained_scope().as_ref() != Some(&request.scope.pair())
        {
            return Err("Query draft belongs to another owner or a pending save".into());
        }
        let app = self.app.as_mut().ok_or("Source unavailable")?;
        let (id, access) = graphshell::query_views::save_with_access(
            app,
            request.owner,
            request.expected.as_ref(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            &request.spec,
        )?;
        self.pending_operation = Some(PendingOperation::Query(PendingQuery::Save {
            request,
            owner: id,
            access,
        }));
        self.operation_success = Some("Query view saved".into());
        self.persist_operation();
        Ok(())
    }
    pub(in super::super) fn retry_query(
        &mut self,
        id: RequestId,
        scope: &Scope,
    ) -> Result<(), String> {
        if self.saving || !self.operation_pending {
            return Err("Query save is not ready to retry".into());
        }
        if !matches!(&self.pending_operation, Some(PendingOperation::Query(PendingQuery::Save { request, .. })) if request.id.same_operation(id) && request.id.attempt.checked_add(1) == Some(id.attempt) && &request.scope == scope)
            || self.retained_scope().as_ref() != Some(&scope.pair())
        {
            return Err("Pending query save belongs to another operation or owner".into());
        }
        if let Some(PendingOperation::Query(PendingQuery::Save { request, .. })) =
            &mut self.pending_operation
        {
            request.id = id;
        }
        self.persist_operation();
        Ok(())
    }

    pub(in super::super) fn freeze_query(
        &mut self,
        scope: &(Uuid, String),
        projection: &QueryProjection,
    ) -> Result<QueryFreezeCorrelation, String> {
        if self.selection_locked() || self.retained_scope().as_ref() != Some(scope) {
            return Err("Query view belongs to another owner or a pending save".into());
        }
        let app = self.app.as_mut().ok_or("Source unavailable")?;
        if !projection.is_current(app.host.graph()) {
            return Err("Refresh the query view before freezing".into());
        }
        let frozen = graphshell::query_views::freeze(app, projection.owner, Uuid::new_v4())?;
        let correlation = QueryFreezeCorrelation {
            scope: scope.clone(),
            frozen,
            attempt: 0,
        };
        self.pending_operation = Some(PendingOperation::Query(PendingQuery::Freeze {
            correlation: correlation.clone(),
        }));
        self.operation_success = Some("Query selection frozen as a Resource".into());
        self.persist_operation();
        Ok(correlation)
    }
    /// Called once for the actual persistence completion, after restoring its
    /// owner. No component infers success from presentation strings or flags.
    pub(super) fn complete_query(&mut self, pending: PendingQuery, result: &Result<(), String>) {
        if !matches!(
            &self.pending_operation,
            Some(PendingOperation::Query(current)) if current == &pending
        ) {
            return;
        }
        match pending {
            PendingQuery::Save {
                request,
                owner,
                access,
            } => {
                let same_owner = self.retained_scope().as_ref() == Some(&request.scope.pair());
                let outcome = match result {
                    Err(reason) => SaveResult::Unconfirmed {
                        reason: reason.clone(),
                    },
                    Ok(())
                        if same_owner
                            && self.source_graph().is_ok_and(|graph| {
                                mere::query_view::read_spec(graph, owner).as_ref()
                                    == Ok(&request.spec)
                            }) =>
                    {
                        SaveResult::Committed {
                            owner,
                            access: Some(access),
                        }
                    },
                    Ok(()) => SaveResult::Stale,
                };
                self.query_completed
                    .push_back(QueryCompletion::Save(Completion {
                        id: request.id,
                        scope: request.scope,
                        result: outcome,
                    }));
            },
            PendingQuery::Freeze { correlation } => {
                let outcome = result.clone().and_then(|()| {
                    if self.retained_scope().as_ref() != Some(&correlation.scope) {
                        return Err("Frozen selection completion belongs to another owner".into());
                    }
                    self.source_graph()?
                        .open_frozen_selection(correlation.frozen)
                        .map(|_| ())
                });
                self.query_completed.push_back(QueryCompletion::Freeze {
                    correlation,
                    result: outcome,
                });
            },
        }
        if result.is_ok() && matches!(&self.pending_operation, Some(PendingOperation::Query(_))) {
            self.pending_operation = None;
        }
    }
}
