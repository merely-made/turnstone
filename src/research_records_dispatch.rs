// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
pub(crate) fn execute(
    bridge: &research::Bridge,
    trail: &armillary::ActorHandle<TrailCommand>,
    job: Job,
) -> JobResult {
    match job {
        Job::Query(request) => {
            let result = query(bridge, trail, &request);
            let manifest = if request.result_anchor.is_some() {
                match &result {
                    Ok(value) => (|| {
                        use eidetic::TypedPayload;
                        let id = eidetic::ManifestId::of_blob(
                            &value.serialize_to_bytes().map_err(|e| e.to_string())?,
                        );
                        let saved = result_request(&request, id)?;
                        command(trail, &request.scope, |reply| {
                            record_actor::Kind::SaveResult(saved, value.clone(), reply)
                        })
                        .map(Some)
                    })(),
                    Err(_) => Ok(None),
                }
            } else {
                Ok(None)
            };
            JobResult::Query {
                request,
                result,
                manifest,
            }
        },
        Job::Deposit(request) => {
            let result = (|| {
                let draft_bytes = author_input_size(&request)?;
                let deposit = record_request(&request)?;
                let draft_bytes = draft_bytes.max(bounded_json_size(
                    &(&deposit.definition, &deposit.record),
                    request.scope.settings.max_input_bytes,
                )?);
                let inputs = sources(
                    bridge,
                    trail,
                    &request.scope,
                    request.selected.clone(),
                    request.scope.settings.max_sources,
                    request.scope.settings.max_input_bytes - draft_bytes,
                    None,
                )?;
                let source = inputs
                    .sources
                    .iter()
                    .find(|s| s.retained.source == *deposit.record.evidence.source())
                    .ok_or("Passage is not in the selected admitted sources")?;
                validate_evidence(&deposit.record, source)?;
                current(bridge, &request.scope)?;
                let manifest = command(trail, &request.scope, |reply| {
                    record_actor::Kind::Deposit(deposit, reply)
                })?;
                current(bridge, &request.scope)?;
                Ok(manifest)
            })();
            JobResult::Deposit { request, result }
        },
        Job::Inspect { id, scope, binding } => {
            let result = (|| {
                let inputs = sources(
                    bridge,
                    trail,
                    &scope,
                    scope.selected.clone(),
                    scope.settings.max_sources,
                    scope.settings.max_input_bytes,
                    None,
                )?;
                let (record, _) = command(trail, &scope, |reply| {
                    record_actor::Kind::ReadRecord(binding.manifest, reply)
                })?;
                if record.entity != binding.entity.to_string() {
                    return Err("Property entity mismatch".into());
                }
                let source = inputs
                    .sources
                    .iter()
                    .find(|s| s.retained.source == *record.evidence.source())
                    .ok_or("Property evidence is not admitted")?;
                validate_evidence(&record, source)?;
                current(bridge, &scope)?;
                Ok(record)
            })();
            JobResult::Inspect {
                id,
                scope,
                binding,
                result,
            }
        },
        Job::Replay { id, scope, result } => {
            let loaded = (|| {
                let inputs = sources(
                    bridge,
                    trail,
                    &scope,
                    scope.selected.clone(),
                    scope.settings.max_sources,
                    scope.settings.max_input_bytes,
                    None,
                )?;
                let value = command(trail, &scope, |reply| {
                    record_actor::Kind::ReadResult(result, reply)
                })?;
                validate_saved_inputs(trail, &scope, &inputs, &value)?;
                current(bridge, &scope)?;
                Ok(value)
            })();
            JobResult::Replay {
                id,
                scope,
                result: loaded,
            }
        },
        Job::ConfirmProperty {
            request,
            scope,
            binding,
        } => {
            let result = (|| {
                current(bridge, &scope)?;
                let deposit = record_request(&request)?;
                command(trail, &scope, |reply| {
                    record_actor::Kind::Acknowledge(
                        deposit,
                        binding.manifest,
                        binding.statement_id.clone(),
                        reply,
                    )
                })?;
                current(bridge, &scope)
            })();
            JobResult::ConfirmProperty {
                request,
                scope,
                binding,
                result,
            }
        },
        Job::ConfirmResult {
            request,
            scope,
            binding,
            value,
        } => {
            let result = (|| {
                current(bridge, &scope)?;
                let saved = result_request(&request, binding.manifest)?;
                command(trail, &scope, |reply| {
                    record_actor::Kind::AcknowledgeResult(
                        saved,
                        binding.statement_id.clone(),
                        reply,
                    )
                })?;
                current(bridge, &scope)
            })();
            JobResult::ConfirmResult {
                request,
                scope,
                binding,
                value,
                result,
            }
        },
        Job::MarkProperty {
            request,
            scope,
            binding,
            assertion,
        } => {
            let result = (|| {
                current(bridge, &scope)?;
                let deposit = record_request(&request)?;
                command(trail, &scope, |reply| {
                    record_actor::Kind::MarkAttached(
                        deposit,
                        binding.manifest,
                        binding.statement_id.clone(),
                        assertion.clone(),
                        reply,
                    )
                })?;
                current(bridge, &scope)
            })();
            JobResult::MarkProperty {
                request,
                scope,
                binding,
                assertion,
                result,
            }
        },
        Job::MarkResult {
            request,
            scope,
            binding,
            assertion,
            value,
        } => {
            let result = (|| {
                current(bridge, &scope)?;
                let saved = result_request(&request, binding.manifest)?;
                command(trail, &scope, |reply| {
                    record_actor::Kind::MarkResultAttached(
                        saved,
                        binding.statement_id.clone(),
                        assertion.clone(),
                        reply,
                    )
                })?;
                current(bridge, &scope)
            })();
            JobResult::MarkResult {
                request,
                scope,
                binding,
                assertion,
                value,
                result,
            }
        },
        Job::PrepareWithdrawal {
            id,
            scope,
            binding,
            nonce,
        } => {
            let result = (|| {
                current(bridge, &scope)?;
                command(trail, &scope, |reply| {
                    record_actor::Kind::PrepareWithdrawal(
                        nonce,
                        *binding.graph.as_uuid(),
                        binding.manifest,
                        binding.statement_id.clone(),
                        reply,
                    )
                })?;
                current(bridge, &scope)
            })();
            JobResult::PrepareWithdrawal {
                id,
                scope,
                binding,
                nonce,
                result,
            }
        },
        Job::ConfirmWithdrawal {
            id,
            scope,
            binding,
            nonce,
        } => {
            let result = (|| {
                current(bridge, &scope)?;
                command(trail, &scope, |reply| {
                    record_actor::Kind::AcknowledgeWithdrawal(
                        nonce,
                        *binding.graph.as_uuid(),
                        binding.manifest,
                        binding.statement_id.clone(),
                        reply,
                    )
                })?;
                current(bridge, &scope)
            })();
            JobResult::ConfirmWithdrawal {
                id,
                scope,
                binding,
                nonce,
                result,
            }
        },
        Job::Recovery { id, scope } => {
            let result = (|| {
                current(bridge, &scope)?;
                let items = command(trail, &scope, |reply| {
                    record_actor::Kind::Inventory(
                        scope.settings.max_records,
                        scope.settings.max_input_bytes,
                        reply,
                    )
                })?;
                current(bridge, &scope)?;
                Ok(items)
            })();
            JobResult::Recovery { id, scope, result }
        },
        Job::Reconcile { id, scope, item } => {
            let result = (|| {
                let inputs = sources(
                    bridge,
                    trail,
                    &scope,
                    scope.selected.clone(),
                    scope.settings.max_sources,
                    scope.settings.max_input_bytes,
                    Some(item.nonce()),
                )?;
                match &item {
                    record_actor::RecoveryItem::Property(receipt) => {
                        let manifest = receipt
                            .manifest
                            .ok_or("The unbound deposit has no sealed record")?;
                        let (record, definition) = command(trail, &scope, |reply| {
                            record_actor::Kind::ReadRecord(manifest, reply)
                        })?;
                        if record != receipt.request.record
                            || definition != receipt.request.definition
                        {
                            return Err("Recovery payload differs from its receipt".into());
                        }
                        let source = inputs
                            .sources
                            .iter()
                            .find(|s| s.retained.source == *record.evidence.source())
                            .ok_or("Recovery evidence is not selected and admitted")?;
                        validate_evidence(&record, source)?;
                    },
                    record_actor::RecoveryItem::Result(receipt) => {
                        if !receipt.deposited {
                            return Err("The unbound result has no sealed payload".into());
                        }
                        let saved = command(trail, &scope, |reply| {
                            record_actor::Kind::ReadResult(receipt.request.manifest, reply)
                        })?;
                        validate_saved_inputs(trail, &scope, &inputs, &saved)?;
                    },
                }
                current(bridge, &scope)
            })();
            JobResult::Reconcile {
                id,
                scope,
                item,
                result,
            }
        },
        Job::RecoveryMark {
            id,
            scope,
            item,
            binding,
            assertion,
        } => {
            let result = (|| {
                current(bridge, &scope)?;
                match &item {
                    record_actor::RecoveryItem::Property(receipt) => {
                        command(trail, &scope, |reply| {
                            record_actor::Kind::MarkAttached(
                                receipt.request.clone(),
                                binding.manifest,
                                binding.statement_id.clone(),
                                assertion.clone(),
                                reply,
                            )
                        })?
                    },
                    record_actor::RecoveryItem::Result(receipt) => {
                        command(trail, &scope, |reply| {
                            record_actor::Kind::MarkResultAttached(
                                receipt.request.clone(),
                                binding.statement_id.clone(),
                                assertion.clone(),
                                reply,
                            )
                        })?
                    },
                }
                current(bridge, &scope)
            })();
            JobResult::RecoveryMark {
                id,
                scope,
                item,
                binding,
                assertion,
                result,
            }
        },
        Job::RecoveryAck {
            id,
            scope,
            item,
            binding,
        } => {
            let result = (|| {
                current(bridge, &scope)?;
                match &item {
                    record_actor::RecoveryItem::Property(receipt)
                        if receipt.withdrawal.as_ref().is_some_and(|w| !w.saved) =>
                    {
                        command(trail, &scope, |reply| {
                            record_actor::Kind::AcknowledgeWithdrawal(
                                receipt.request.nonce,
                                receipt.request.graph,
                                binding.manifest,
                                binding.statement_id.clone(),
                                reply,
                            )
                        })?
                    },
                    record_actor::RecoveryItem::Property(receipt) => {
                        command(trail, &scope, |reply| {
                            record_actor::Kind::Acknowledge(
                                receipt.request.clone(),
                                binding.manifest,
                                binding.statement_id.clone(),
                                reply,
                            )
                        })?
                    },
                    record_actor::RecoveryItem::Result(receipt) => {
                        command(trail, &scope, |reply| {
                            record_actor::Kind::AcknowledgeResult(
                                receipt.request.clone(),
                                binding.statement_id.clone(),
                                reply,
                            )
                        })?
                    },
                }
                current(bridge, &scope)
            })();
            JobResult::RecoveryAck {
                id,
                scope,
                item,
                binding,
                result,
            }
        },
        Job::Abandon { id, scope, nonce } => {
            let result = (|| {
                current(bridge, &scope)?;
                let bindings = scope
                    .graph_manifests
                    .iter()
                    .map(|(graph, manifest)| (*graph.as_uuid(), *manifest))
                    .collect();
                command(trail, &scope, |reply| {
                    record_actor::Kind::Abandon(nonce, bindings, reply)
                })?;
                current(bridge, &scope)
            })();
            JobResult::Abandon {
                id,
                scope,
                nonce,
                result,
            }
        },
        Job::OpenSource {
            id,
            scope,
            generation,
            source,
            version,
        } => {
            let result =
                (|| {
                    let inputs = sources(
                        bridge,
                        trail,
                        &scope,
                        scope.selected.clone(),
                        scope.settings.max_sources,
                        scope.settings.max_input_bytes,
                        None,
                    )?;
                    if !inputs.sources.iter().any(|held| {
                        held.retained.source == source && held.retained.version == version
                    }) {
                        return Err("Frozen source is no longer selected and admitted".into());
                    }
                    let target = research::target(&source, &version)?;
                    current(bridge, &scope)?;
                    Ok(target)
                })();
            JobResult::OpenSource {
                id,
                scope,
                generation,
                source,
                version,
                result,
            }
        },
        Job::OpenEvidence {
            id,
            scope,
            generation,
            binding,
            record,
        } => {
            let result = (|| {
                let inputs = sources(
                    bridge,
                    trail,
                    &scope,
                    scope.selected.clone(),
                    scope.settings.max_sources,
                    scope.settings.max_input_bytes,
                    None,
                )?;
                if !scope
                    .owner_bindings
                    .iter()
                    .any(|owner| owner.binding == binding && owner.author == record.author)
                {
                    return Err("Property owner is unavailable".into());
                }
                let (held, _) = command(trail, &scope, |reply| {
                    record_actor::Kind::ReadRecord(binding.manifest, reply)
                })?;
                if held != record {
                    return Err("Frozen property payload changed".into());
                }
                let source = inputs
                    .sources
                    .iter()
                    .find(|held| held.retained.source == *record.evidence.source())
                    .ok_or("Property evidence is no longer selected and admitted")?;
                validate_evidence(&record, source)?;
                let Evidence::Passage {
                    source,
                    version,
                    selector,
                    ..
                } = &record.evidence
                else {
                    return Err("Typed field evidence is unsupported".into());
                };
                let mut target = research::target(source, version)?;
                target.set_selector(crate::saved_sources::Selector {
                    start: selector.start,
                    end: selector.end,
                });
                current(bridge, &scope)?;
                Ok(target)
            })();
            JobResult::OpenEvidence {
                id,
                scope,
                generation,
                binding,
                record,
                result,
            }
        },
        Job::Discover {
            id,
            scope,
            bindings,
            history,
        } => {
            let result = (|| {
                current(bridge, &scope)?;
                let held = scope
                    .graph_manifests
                    .iter()
                    .map(|(graph, manifest)| (*graph.as_uuid(), *manifest))
                    .collect();
                command(trail, &scope, |reply| {
                    record_actor::Kind::Pending(
                        held,
                        scope.settings.max_records,
                        scope.settings.max_input_bytes,
                        None,
                        reply,
                    )
                })?;
                current(bridge, &scope)
            })();
            JobResult::Discover {
                id,
                scope,
                bindings,
                history,
                result,
            }
        },
    }
}
