// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::App;
use crate::action::Effect;
use crate::research_records::{Binding, Job, JobResult, PendingChoice, SavedResultChoice};
impl App {
    pub(crate) fn structured_completed(&mut self, response: JobResult) -> Vec<Effect> {
        if !self.structured_response_current(&response) {
            return vec![];
        }
        match response {
            JobResult::OpenSource {
                generation,
                source,
                version,
                result,
                ..
            } => {
                let controller = &self.research_search.structured.controller;
                if controller.result_generation != Some(generation)
                    || !controller.result.as_ref().is_some_and(|held| {
                        held.snapshot
                            .sources
                            .iter()
                            .any(|item| item.source == source && item.version == version)
                    })
                {
                    return self.structured_error("Frozen source changed");
                }
                let target = match result {
                    Ok(target) => target,
                    Err(error) => return self.structured_error(error),
                };
                let controller = &mut self.research_search.structured.controller;
                controller.pending = None;
                controller.busy = false;
                return self.structured_open_verified(target);
            },
            JobResult::OpenEvidence {
                generation,
                binding,
                record,
                result,
                ..
            } => {
                let controller = &self.research_search.structured.controller;
                let still_current = match generation {
                    Some(generation) => {
                        controller.result_generation == Some(generation)
                            && controller.frozen_record(&record.id) == Some(&record)
                    },
                    None => controller
                        .inspected
                        .as_ref()
                        .is_some_and(|(held, value)| held == &binding && value == &record),
                };
                if !still_current {
                    return self.structured_error("Property evidence changed");
                }
                let target = match result {
                    Ok(target) => target,
                    Err(error) => return self.structured_error(error),
                };
                let controller = &mut self.research_search.structured.controller;
                controller.pending = None;
                controller.busy = false;
                return self.structured_open_verified(target);
            },
            JobResult::Deposit { request, result } => {
                let manifest = match result {
                    Ok(id) => id,
                    Err(error) => return self.structured_error(error),
                };
                let (binding, assertion) = match self.structured_attach_binding(
                    request.property.graph,
                    request.property.entity,
                    manifest,
                    false,
                    &request.author,
                ) {
                    Ok(value) => value,
                    Err(error) => return self.structured_error(error),
                };
                let scope = match self.structured_after_commit(&request.scope, binding.graph) {
                    Ok(value) => value,
                    Err(error) => return self.structured_error(error),
                };
                self.clear_structured_projection();
                return self.structured_submit(Job::MarkProperty {
                    request,
                    scope,
                    binding,
                    assertion,
                });
            },
            JobResult::Query {
                request,
                result,
                manifest,
            } => {
                let value = match result {
                    Ok(value) => value,
                    Err(error) => return self.structured_error(error),
                };
                let manifest = match manifest {
                    Ok(value) => value,
                    Err(error) => return self.structured_error(error),
                };
                if let (Some(manifest), Some((graph, entity))) = (manifest, request.result_anchor) {
                    let author = match self.structured_author_identity() {
                        Ok(value) => value.asserter_iri(),
                        Err(error) => return self.structured_error(error),
                    };
                    let (binding, assertion) = match self
                        .structured_attach_binding(graph, entity, manifest, true, &author)
                    {
                        Ok(value) => value,
                        Err(error) => return self.structured_error(error),
                    };
                    let scope = match self.structured_after_commit(&request.scope, graph) {
                        Ok(value) => value,
                        Err(error) => return self.structured_error(error),
                    };
                    self.clear_structured_projection();
                    return self.structured_submit(Job::MarkResult {
                        request,
                        scope,
                        binding,
                        assertion,
                        value,
                    });
                }
                let controller = &mut self.research_search.structured.controller;
                if let Err(error) = controller.install_result(value, request.scope.clone()) {
                    return self.structured_error(error);
                }
                controller.accepted = Some(request);
                controller.error = Some(
                    "This result could not be saved because no graph resource is available".into(),
                );
            },
            JobResult::Inspect {
                scope,
                binding,
                result,
                ..
            } => {
                let value = match result {
                    Ok(value) => value,
                    Err(error) => return self.structured_error(error),
                };
                let candidate = self.structured_candidates(false).and_then(|mut a| {
                    a.extend(self.structured_candidates(true)?);
                    Ok(a)
                });
                if !candidate.ok().is_some_and(|all| {
                    all.iter()
                        .any(|item| item.binding == binding && item.author == value.author)
                }) {
                    return self.structured_error("Property attribution changed");
                }
                let controller = &mut self.research_search.structured.controller;
                controller.inspected = Some((binding, value));
                controller.view_scope = Some(scope);
                controller.error = None;
            },
            JobResult::Replay { scope, result, .. } => {
                let value = match result {
                    Ok(value) => value,
                    Err(error) => return self.structured_error(error),
                };
                let controller = &mut self.research_search.structured.controller;
                if let Err(error) = controller.install_result(value, scope) {
                    return self.structured_error(error);
                }
                controller.accepted = None;
                controller.error = None;
            },
            JobResult::ConfirmProperty {
                scope,
                binding,
                result,
                ..
            } => {
                if let Err(error) = result {
                    return self.structured_error(error);
                }
                let controller = &mut self.research_search.structured.controller;
                controller.view_scope = Some(scope);
                controller.error = None;
                controller.inspected = None;
                controller.saved_result = None;
                let _ = binding;
            },
            JobResult::ConfirmResult {
                mut request,
                scope,
                binding,
                value,
                result,
            } => {
                if let Err(error) = result {
                    return self.structured_error(error);
                }
                request.scope = scope.clone();
                let controller = &mut self.research_search.structured.controller;
                if let Err(error) = controller.install_result(value, scope) {
                    return self.structured_error(error);
                }
                controller.accepted = Some(request);
                controller.saved_result = Some(binding.manifest);
                controller.error = None;
            },
            JobResult::MarkProperty {
                request,
                scope,
                binding,
                assertion,
                result,
            } => {
                if let Err(error) = result {
                    return self.structured_error(error);
                }
                if let Err(error) = self.structured_check_assertion_author(&assertion) {
                    return self.structured_error(error);
                }
                if let Err(error) = self.structured_assertion_matches(&binding, &assertion, false) {
                    return self.structured_error(error);
                }
                if let Err(error) = self.structured_save_graph(binding.graph) {
                    return self.structured_error(error);
                }
                if self.structured_scope().ok().as_ref() != Some(&scope) {
                    return self.structured_error("Structured scope changed during save");
                }
                return self.structured_submit(Job::ConfirmProperty {
                    request,
                    scope,
                    binding,
                });
            },
            JobResult::MarkResult {
                request,
                scope,
                binding,
                assertion,
                value,
                result,
            } => {
                if let Err(error) = result {
                    return self.structured_error(error);
                }
                if let Err(error) = self.structured_check_assertion_author(&assertion) {
                    return self.structured_error(error);
                }
                if let Err(error) = self.structured_assertion_matches(&binding, &assertion, true) {
                    return self.structured_error(error);
                }
                if let Err(error) = self.structured_save_graph(binding.graph) {
                    return self.structured_error(error);
                }
                if self.structured_scope().ok().as_ref() != Some(&scope) {
                    return self.structured_error("Structured scope changed during save");
                }
                return self.structured_submit(Job::ConfirmResult {
                    request,
                    scope,
                    binding,
                    value,
                });
            },
            JobResult::PrepareWithdrawal {
                id,
                scope,
                binding,
                nonce,
                result,
            } => {
                if let Err(error) = result {
                    return self.structured_error(error);
                }
                if !self
                    .structured_has_binding(&binding, false)
                    .unwrap_or(false)
                {
                    return self.structured_error("Property binding changed before withdrawal");
                }
                if let Err(error) = self.structured_commit_withdrawal(&binding) {
                    return self.structured_error(error);
                }
                let after = match self.structured_after_commit(&scope, binding.graph) {
                    Ok(value) => value,
                    Err(error) => return self.structured_error(error),
                };
                self.clear_structured_projection();
                return self.structured_submit(Job::ConfirmWithdrawal {
                    id,
                    scope: after,
                    binding,
                    nonce,
                });
            },
            JobResult::ConfirmWithdrawal { scope, result, .. } => {
                if let Err(error) = result {
                    return self.structured_error(error);
                }
                let controller = &mut self.research_search.structured.controller;
                controller.view_scope = Some(scope);
                controller.error = None;
            },
            JobResult::Recovery { scope, result, .. } => {
                let items = match result {
                    Ok(value) => value,
                    Err(error) => return self.structured_error(error),
                };
                let mut choices = Vec::new();
                for item in &items {
                    let (kind, graph, manifest, attached, withdrawal) = match item {
                        crate::trail_memory::record_actor::RecoveryItem::Property(receipt) => (
                            "Property",
                            receipt.request.graph,
                            receipt.manifest,
                            receipt.statement_id.is_some(),
                            receipt.withdrawal.as_ref().is_some_and(|w| !w.saved),
                        ),
                        crate::trail_memory::record_actor::RecoveryItem::Result(receipt) => (
                            "Result",
                            receipt.request.graph,
                            Some(receipt.request.manifest),
                            receipt.statement_id.is_some(),
                            false,
                        ),
                    };
                    let live = manifest.is_some_and(|manifest| {
                        scope
                            .graph_manifests
                            .contains(&(crate::panes::GraphId(graph), manifest))
                    });
                    let orphan = !attached && !live && !withdrawal;
                    let status = if orphan {
                        "Unbound deposit"
                    } else if withdrawal {
                        "Withdrawal awaiting save"
                    } else {
                        "Attachment awaiting save"
                    };
                    choices.push(PendingChoice {
                        nonce: item.nonce(),
                        kind: kind.into(),
                        status: status.into(),
                        orphan,
                    });
                }
                let controller = &mut self.research_search.structured.controller;
                controller.recovery = choices;
                controller.recovery_items = items;
                controller.recovery_scope = Some(scope);
                controller.error = None;
            },
            JobResult::Discover {
                scope,
                bindings,
                history,
                result,
                ..
            } => {
                if let Err(error) = result {
                    return self.structured_error(error);
                }
                let controller = &mut self.research_search.structured.controller;
                if history {
                    controller.history_choices = bindings;
                    controller.history_scope = Some(scope);
                } else {
                    controller.saved_choices = bindings
                        .into_iter()
                        .map(|binding| SavedResultChoice { binding })
                        .collect();
                    controller.discovery_scope = Some(scope);
                }
                controller.error = None;
            },
            JobResult::Reconcile {
                id,
                scope,
                item,
                result,
            } => {
                if let Err(error) = result {
                    return self.structured_error(error);
                }
                let (binding, assertion, withdrawn) = match self.structured_recover_assertion(&item)
                {
                    Ok(value) => value,
                    Err(error) => return self.structured_error(error),
                };
                let withdrawal = matches!(&item, crate::trail_memory::record_actor::RecoveryItem::Property(receipt) if receipt.withdrawal.as_ref().is_some_and(|w| !w.saved));
                if withdrawal {
                    let saved = if withdrawn {
                        self.structured_save_graph(binding.graph)
                    } else {
                        self.structured_commit_withdrawal(&binding)
                    };
                    if let Err(error) = saved {
                        return self.structured_error(error);
                    }
                    let after = match self.structured_after_commit(&scope, binding.graph) {
                        Ok(value) => value,
                        Err(error) => return self.structured_error(error),
                    };
                    self.clear_structured_projection();
                    return self.structured_submit(Job::RecoveryAck {
                        id,
                        scope: after,
                        item,
                        binding,
                    });
                }
                let after = match self.structured_after_commit(&scope, binding.graph) {
                    Ok(value) => value,
                    Err(error) => return self.structured_error(error),
                };
                self.clear_structured_projection();
                return self.structured_submit(Job::RecoveryMark {
                    id,
                    scope: after,
                    item,
                    binding,
                    assertion,
                });
            },
            JobResult::RecoveryMark {
                id,
                scope,
                item,
                binding,
                assertion,
                result,
            } => {
                if let Err(error) = result {
                    return self.structured_error(error);
                }
                if let Err(error) = self.structured_check_assertion_author(&assertion) {
                    return self.structured_error(error);
                }
                let result_kind = matches!(
                    &item,
                    crate::trail_memory::record_actor::RecoveryItem::Result(_)
                );
                if let Err(error) =
                    self.structured_assertion_matches(&binding, &assertion, result_kind)
                {
                    return self.structured_error(error);
                }
                if let Err(error) = self.structured_save_graph(binding.graph) {
                    return self.structured_error(error);
                }
                if self.structured_scope().ok().as_ref() != Some(&scope) {
                    return self.structured_error("Structured scope changed during recovery save");
                }
                return self.structured_submit(Job::RecoveryAck {
                    id,
                    scope,
                    item,
                    binding,
                });
            },
            JobResult::Abandon { result, .. } | JobResult::RecoveryAck { result, .. } => {
                if let Err(error) = result {
                    return self.structured_error(error);
                }
                return self.structured_refresh_recovery();
            },
        }
        let controller = &mut self.research_search.structured.controller;
        controller.pending = None;
        controller.busy = false;
        if self.research_search.structured.indexed_generation != controller.result_generation {
            self.clear_structured_index();
        }
        self.recompute_omnibar_suggestions();
        vec![Effect::Redraw]
    }
}
