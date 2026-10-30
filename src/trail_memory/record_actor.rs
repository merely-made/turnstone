// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Structured records through the existing session-bound sealed actor.
use super::{SourceCustody, sealed_record_receipts, sealed_records, sealed_result_receipts};
use crate::research_storage::SourceReadError as HostError;
use eidetic::ManifestId;
use eidetic::records::{PropertyDefinitionV1, PropertyRecordV1, RecordResult};
use std::path::Path;
use std::sync::mpsc::SyncSender;

pub(crate) use sealed_record_receipts::{DepositRequest, RecoveryItem};
pub(crate) use sealed_result_receipts::ResultRequest;
pub(crate) struct Command {
    pub session: uuid::Uuid,
    pub generation: uuid::Uuid,
    pub max_bytes: usize,
    pub budget: crate::research_storage::SharedReadBudget,
    pub kind: Kind,
}
pub(crate) enum Kind {
    Deposit(DepositRequest, SyncSender<Result<ManifestId, HostError>>),
    ReadRecord(
        ManifestId,
        usize,
        usize,
        SyncSender<Result<(PropertyRecordV1, PropertyDefinitionV1), HostError>>,
    ),
    SaveResult(
        ResultRequest,
        RecordResult,
        SyncSender<Result<ManifestId, HostError>>,
    ),
    ReadResult(
        ManifestId,
        eidetic::records::RecordSettings,
        SyncSender<Result<RecordResult, HostError>>,
    ),
    Acknowledge(
        DepositRequest,
        ManifestId,
        String,
        SyncSender<Result<(), HostError>>,
    ),
    MarkAttached(
        DepositRequest,
        ManifestId,
        String,
        serde_json::Value,
        SyncSender<Result<(), HostError>>,
    ),
    AcknowledgeResult(ResultRequest, String, SyncSender<Result<(), HostError>>),
    MarkResultAttached(
        ResultRequest,
        String,
        serde_json::Value,
        SyncSender<Result<(), HostError>>,
    ),
    Pending(
        Vec<(uuid::Uuid, ManifestId)>,
        usize,
        usize,
        Option<uuid::Uuid>,
        SyncSender<Result<(), HostError>>,
    ),
    PrepareWithdrawal(
        uuid::Uuid,
        uuid::Uuid,
        ManifestId,
        String,
        SyncSender<Result<(), HostError>>,
    ),
    AcknowledgeWithdrawal(
        uuid::Uuid,
        uuid::Uuid,
        ManifestId,
        String,
        SyncSender<Result<(), HostError>>,
    ),
    Inventory(
        usize,
        usize,
        SyncSender<Result<Vec<RecoveryItem>, HostError>>,
    ),
    Abandon(
        uuid::Uuid,
        Vec<(uuid::Uuid, ManifestId)>,
        SyncSender<Result<(), HostError>>,
    ),
}
fn execute<T>(
    session: uuid::Uuid,
    generation: uuid::Uuid,
    store: &mut Option<muniment::FjallStore>,
    custody: Option<&SourceCustody>,
    directory: &Path,
    current_generation: uuid::Uuid,
    max_bytes: usize,
    budget: &crate::research_storage::SharedReadBudget,
    sync_after: bool,
    sync: &(dyn Fn(&muniment::FjallStore) -> Result<(), muniment::StoreError> + Sync),
    operation: impl FnOnce(
        &mut super::structured_bounded_store::BoundedStore<'_>,
        &dyn eidetic::PayloadSealer,
    ) -> Result<T, HostError>,
) -> Result<T, HostError> {
    let current = directory
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str());
    if generation != current_generation || current != Some(session.to_string().as_str()) {
        return Err("Structured research belongs to another session opening".into());
    }
    let binding = custody.ok_or("Unlock the profile to use structured research")?;
    let epoch = binding
        .snapshot()
        .ok_or("Unlock the profile to use structured research")?;
    let store = store
        .as_mut()
        .ok_or("Structured research store unavailable")?;
    let mut bounded = super::structured_bounded_store::BoundedStore {
        inner: store,
        max_bytes,
        budget: budget.clone(),
        sync,
    };
    let result = operation(&mut bounded, binding.sealer.as_ref());
    drop(bounded);
    if result.is_ok() && sync_after {
        sync(store)?;
    }
    if !binding.remains_current(Some(epoch)) {
        return Err("Structured research custody changed".into());
    }
    result
}
pub(super) fn reject(command: Command, error: HostError) {
    match command.kind {
        Kind::Deposit(_, reply) | Kind::SaveResult(_, _, reply) => {
            let _ = reply.send(Err(error));
        },
        Kind::ReadRecord(_, _, _, reply) => {
            let _ = reply.send(Err(error));
        },
        Kind::ReadResult(_, _, reply) => {
            let _ = reply.send(Err(error));
        },
        Kind::Inventory(_, _, reply) => {
            let _ = reply.send(Err(error));
        },
        Kind::Acknowledge(_, _, _, reply)
        | Kind::MarkAttached(_, _, _, _, reply)
        | Kind::AcknowledgeResult(_, _, reply)
        | Kind::MarkResultAttached(_, _, _, reply)
        | Kind::Pending(_, _, _, _, reply)
        | Kind::PrepareWithdrawal(_, _, _, _, reply)
        | Kind::AcknowledgeWithdrawal(_, _, _, _, reply)
        | Kind::Abandon(_, _, reply) => {
            let _ = reply.send(Err(error));
        },
    }
}
pub(super) fn handle(
    command: Command,
    store: &mut Option<muniment::FjallStore>,
    custody: Option<&SourceCustody>,
    directory: &Path,
    generation: uuid::Uuid,
) {
    handle_with_sync(command, store, custody, directory, generation, &|store| {
        store.sync_all()
    });
}
pub(super) fn handle_with_sync(
    command: Command,
    store: &mut Option<muniment::FjallStore>,
    custody: Option<&SourceCustody>,
    directory: &Path,
    generation: uuid::Uuid,
    sync: &(dyn Fn(&muniment::FjallStore) -> Result<(), muniment::StoreError> + Sync),
) {
    let Command {
        session,
        generation: request_generation,
        max_bytes,
        budget,
        kind,
    } = command;
    let sync_after = matches!(
        &kind,
        Kind::Deposit(..)
            | Kind::SaveResult(..)
            | Kind::MarkAttached(..)
            | Kind::MarkResultAttached(..)
            | Kind::PrepareWithdrawal(..)
    );
    match kind {
        Kind::Deposit(request, reply) => {
            let result = execute(
                session,
                request_generation,
                store,
                custody,
                directory,
                generation,
                max_bytes,
                &budget,
                sync_after,
                sync,
                |store, sealer| {
                    if request.session != session || request.generation != request_generation {
                        return Err("Property request owner mismatch".into());
                    }
                    sealed_record_receipts::deposit_request(store, sealer, &request)
                },
            );
            let _ = reply.send(result);
        },
        Kind::ReadRecord(id, max_bytes, max_validation_bytes, reply) => {
            let _ = reply.send(execute(
                session,
                request_generation,
                store,
                custody,
                directory,
                generation,
                max_bytes,
                &budget,
                sync_after,
                sync,
                |store, sealer| {
                    sealed_records::load_record_bounded(
                        store,
                        sealer,
                        id,
                        max_bytes,
                        max_validation_bytes,
                    )
                },
            ));
        },
        Kind::SaveResult(request, result, reply) => {
            let _ = reply.send(execute(
                session,
                request_generation,
                store,
                custody,
                directory,
                generation,
                max_bytes,
                &budget,
                sync_after,
                sync,
                |store, sealer| {
                    if request.session != session || request.generation != request_generation {
                        return Err("Result request owner mismatch".into());
                    }
                    sealed_result_receipts::deposit_result_request(store, sealer, &request, &result)
                },
            ));
        },
        Kind::ReadResult(id, settings, reply) => {
            let _ = reply.send(execute(
                session,
                request_generation,
                store,
                custody,
                directory,
                generation,
                max_bytes.min(settings.max_input_bytes),
                &budget,
                sync_after,
                sync,
                |store, sealer| {
                    sealed_records::load_result_bounded(
                        store,
                        sealer,
                        id,
                        settings.max_input_bytes,
                        Some(&settings),
                    )
                },
            ));
        },
        Kind::Acknowledge(request, id, statement, reply) => {
            let _ = reply.send(execute(
                session,
                request_generation,
                store,
                custody,
                directory,
                generation,
                max_bytes,
                &budget,
                sync_after,
                sync,
                |store, sealer| {
                    sealed_record_receipts::acknowledge(store, sealer, &request, id, &statement)
                },
            ));
        },
        Kind::MarkAttached(request, id, statement, assertion, reply) => {
            let _ = reply.send(execute(
                session,
                request_generation,
                store,
                custody,
                directory,
                generation,
                max_bytes,
                &budget,
                sync_after,
                sync,
                |store, sealer| {
                    sealed_record_receipts::mark_attached(
                        store, sealer, &request, id, &statement, &assertion,
                    )
                },
            ));
        },
        Kind::AcknowledgeResult(request, statement, reply) => {
            let _ = reply.send(execute(
                session,
                request_generation,
                store,
                custody,
                directory,
                generation,
                max_bytes,
                &budget,
                sync_after,
                sync,
                |store, sealer| {
                    sealed_result_receipts::acknowledge_result(store, sealer, &request, &statement)
                },
            ));
        },
        Kind::MarkResultAttached(request, statement, assertion, reply) => {
            let _ = reply.send(execute(
                session,
                request_generation,
                store,
                custody,
                directory,
                generation,
                max_bytes,
                &budget,
                sync_after,
                sync,
                |store, sealer| {
                    sealed_result_receipts::mark_result_attached(
                        store, sealer, &request, &statement, &assertion,
                    )
                },
            ));
        },
        Kind::PrepareWithdrawal(nonce, graph, manifest, statement, reply) => {
            let _ = reply.send(execute(
                session,
                request_generation,
                store,
                custody,
                directory,
                generation,
                max_bytes,
                &budget,
                sync_after,
                sync,
                |store, sealer| {
                    sealed_record_receipts::prepare_withdrawal(
                        store, sealer, nonce, session, graph, manifest, &statement,
                    )
                },
            ));
        },
        Kind::AcknowledgeWithdrawal(nonce, graph, manifest, statement, reply) => {
            let _ = reply.send(execute(
                session,
                request_generation,
                store,
                custody,
                directory,
                generation,
                max_bytes,
                &budget,
                sync_after,
                sync,
                |store, sealer| {
                    sealed_record_receipts::acknowledge_withdrawal(
                        store, sealer, nonce, session, graph, manifest, &statement,
                    )
                },
            ));
        },
        Kind::Inventory(max_items, max_bytes, reply) => {
            let _ = reply.send(execute(
                session,
                request_generation,
                store,
                custody,
                directory,
                generation,
                max_bytes,
                &budget,
                sync_after,
                sync,
                |store, sealer| {
                    store.ensure_no_legacy_receipts()?;
                    sealed_record_receipts::inventory_after_legacy_check(
                        store, sealer, session, max_items, max_bytes, true,
                    )
                },
            ));
        },
        Kind::Abandon(nonce, bindings, reply) => {
            let _ = reply.send(execute(
                session,
                request_generation,
                store,
                custody,
                directory,
                generation,
                max_bytes,
                &budget,
                sync_after,
                sync,
                |store, sealer| {
                    sealed_record_receipts::abandon(store, sealer, session, nonce, &bindings)
                },
            ));
        },
        Kind::Pending(bindings, max_items, max_bytes, except, reply) => {
            let _ = reply.send(execute(
                session,
                request_generation,
                store,
                custody,
                directory,
                generation,
                max_bytes,
                &budget,
                sync_after,
                sync,
                |store, sealer| {
                    store.ensure_no_legacy_receipts()?;
                    for item in sealed_record_receipts::inventory_after_legacy_check(
                        store, sealer, session, max_items, max_bytes, true,
                    )? {
                        if Some(item.nonce()) == except {
                            continue;
                        }
                        match item {
                            RecoveryItem::Property(receipt) => {
                                if !receipt.saved
                                    && (receipt.statement_id.is_some()
                                        || receipt.manifest.is_some_and(|id| {
                                            bindings.contains(&(receipt.request.graph, id))
                                        }))
                                {
                                    return Err("A property save is pending recovery".into());
                                }
                                if receipt
                                    .withdrawal
                                    .as_ref()
                                    .is_some_and(|withdrawal| !withdrawal.saved)
                                {
                                    return Err("A property withdrawal is pending recovery".into());
                                }
                            },
                            RecoveryItem::Result(receipt) => {
                                if receipt.statement_id.is_some()
                                    || (receipt.deposited
                                        && bindings.contains(&(
                                            receipt.request.graph,
                                            receipt.request.manifest,
                                        )))
                                {
                                    return Err(
                                        "A structured result save is pending recovery".into()
                                    );
                                }
                            },
                        }
                    }
                    Ok(())
                },
            ));
        },
    }
}
