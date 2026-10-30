// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::App;
use crate::research_records::{Job, JobResult};
impl App {
    pub(super) fn structured_response_current(&self, response: &JobResult) -> bool {
        let (job, scope) = match response {
            JobResult::Query { request, .. } => (Job::Query(request.clone()), &request.scope),
            JobResult::Deposit { request, .. } => (Job::Deposit(request.clone()), &request.scope),
            JobResult::Inspect {
                id, scope, binding, ..
            } => (
                Job::Inspect {
                    id: *id,
                    scope: scope.clone(),
                    binding: binding.clone(),
                },
                scope,
            ),
            JobResult::Replay { id, scope, .. } => {
                let Some(Job::Replay { result, .. }) =
                    self.research_search.structured.controller.pending.as_ref()
                else {
                    return false;
                };
                (
                    Job::Replay {
                        id: *id,
                        scope: scope.clone(),
                        result: *result,
                    },
                    scope,
                )
            },
            JobResult::ConfirmProperty {
                request,
                scope,
                binding,
                ..
            } => (
                Job::ConfirmProperty {
                    request: request.clone(),
                    scope: scope.clone(),
                    binding: binding.clone(),
                },
                scope,
            ),
            JobResult::ConfirmResult {
                request,
                scope,
                binding,
                value,
                ..
            } => (
                Job::ConfirmResult {
                    request: request.clone(),
                    scope: scope.clone(),
                    binding: binding.clone(),
                    value: value.clone(),
                },
                scope,
            ),
            JobResult::MarkProperty {
                request,
                scope,
                binding,
                assertion,
                ..
            } => (
                Job::MarkProperty {
                    request: request.clone(),
                    scope: scope.clone(),
                    binding: binding.clone(),
                    assertion: assertion.clone(),
                },
                scope,
            ),
            JobResult::MarkResult {
                request,
                scope,
                binding,
                assertion,
                value,
                ..
            } => (
                Job::MarkResult {
                    request: request.clone(),
                    scope: scope.clone(),
                    binding: binding.clone(),
                    assertion: assertion.clone(),
                    value: value.clone(),
                },
                scope,
            ),
            JobResult::PrepareWithdrawal {
                id,
                scope,
                binding,
                nonce,
                ..
            } => (
                Job::PrepareWithdrawal {
                    id: *id,
                    scope: scope.clone(),
                    binding: binding.clone(),
                    nonce: *nonce,
                },
                scope,
            ),
            JobResult::ConfirmWithdrawal {
                id,
                scope,
                binding,
                nonce,
                ..
            } => (
                Job::ConfirmWithdrawal {
                    id: *id,
                    scope: scope.clone(),
                    binding: binding.clone(),
                    nonce: *nonce,
                },
                scope,
            ),
            JobResult::Recovery { id, scope, .. } => (
                Job::Recovery {
                    id: *id,
                    scope: scope.clone(),
                },
                scope,
            ),
            JobResult::Reconcile {
                id, scope, item, ..
            } => (
                Job::Reconcile {
                    id: *id,
                    scope: scope.clone(),
                    item: item.clone(),
                },
                scope,
            ),
            JobResult::RecoveryMark {
                id,
                scope,
                item,
                binding,
                assertion,
                ..
            } => (
                Job::RecoveryMark {
                    id: *id,
                    scope: scope.clone(),
                    item: item.clone(),
                    binding: binding.clone(),
                    assertion: assertion.clone(),
                },
                scope,
            ),
            JobResult::RecoveryAck {
                id,
                scope,
                item,
                binding,
                ..
            } => (
                Job::RecoveryAck {
                    id: *id,
                    scope: scope.clone(),
                    item: item.clone(),
                    binding: binding.clone(),
                },
                scope,
            ),
            JobResult::Abandon {
                id, scope, nonce, ..
            } => (
                Job::Abandon {
                    id: *id,
                    scope: scope.clone(),
                    nonce: *nonce,
                },
                scope,
            ),
            JobResult::Discover {
                id,
                scope,
                bindings,
                history,
                ..
            } => (
                Job::Discover {
                    id: *id,
                    scope: scope.clone(),
                    bindings: bindings.clone(),
                    history: *history,
                },
                scope,
            ),
            JobResult::OpenSource {
                id,
                scope,
                generation,
                source,
                version,
                ..
            } => (
                Job::OpenSource {
                    id: *id,
                    scope: scope.clone(),
                    generation: *generation,
                    source: source.clone(),
                    version: version.clone(),
                },
                scope,
            ),
            JobResult::OpenEvidence {
                id,
                scope,
                generation,
                binding,
                record,
                ..
            } => (
                Job::OpenEvidence {
                    id: *id,
                    scope: scope.clone(),
                    generation: *generation,
                    binding: binding.clone(),
                    record: record.clone(),
                },
                scope,
            ),
        };
        self.research_search.structured.controller.pending.as_ref() == Some(&job)
            && self.structured_scope_is_current(scope)
    }
}
