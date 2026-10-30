// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Structured records through the existing session-bound sealed actor.
use super::{SourceCustody, sealed_record_receipts, sealed_records, sealed_result_receipts};
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
    pub kind: Kind,
}
pub(crate) enum Kind {
    Deposit(DepositRequest, SyncSender<Result<ManifestId, String>>),
    ReadRecord(
        ManifestId,
        usize,
        usize,
        SyncSender<Result<(PropertyRecordV1, PropertyDefinitionV1), String>>,
    ),
    SaveResult(
        ResultRequest,
        RecordResult,
        SyncSender<Result<ManifestId, String>>,
    ),
    ReadResult(
        ManifestId,
        eidetic::records::RecordSettings,
        SyncSender<Result<RecordResult, String>>,
    ),
    Acknowledge(
        DepositRequest,
        ManifestId,
        String,
        SyncSender<Result<(), String>>,
    ),
    MarkAttached(
        DepositRequest,
        ManifestId,
        String,
        serde_json::Value,
        SyncSender<Result<(), String>>,
    ),
    AcknowledgeResult(ResultRequest, String, SyncSender<Result<(), String>>),
    MarkResultAttached(
        ResultRequest,
        String,
        serde_json::Value,
        SyncSender<Result<(), String>>,
    ),
    Pending(
        Vec<(uuid::Uuid, ManifestId)>,
        usize,
        usize,
        Option<uuid::Uuid>,
        SyncSender<Result<(), String>>,
    ),
    PrepareWithdrawal(
        uuid::Uuid,
        uuid::Uuid,
        ManifestId,
        String,
        SyncSender<Result<(), String>>,
    ),
    AcknowledgeWithdrawal(
        uuid::Uuid,
        uuid::Uuid,
        ManifestId,
        String,
        SyncSender<Result<(), String>>,
    ),
    Inventory(usize, usize, SyncSender<Result<Vec<RecoveryItem>, String>>),
    Abandon(
        uuid::Uuid,
        Vec<(uuid::Uuid, ManifestId)>,
        SyncSender<Result<(), String>>,
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
    sync_after: bool,
    sync: &(dyn Fn(&muniment::FjallStore) -> Result<(), String> + Sync),
    operation: impl FnOnce(&mut dyn eidetic::Store, &dyn eidetic::PayloadSealer) -> Result<T, String>,
) -> Result<T, String> {
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
pub(super) fn handle(
    command: Command,
    store: &mut Option<muniment::FjallStore>,
    custody: Option<&SourceCustody>,
    directory: &Path,
    generation: uuid::Uuid,
) {
    handle_with_sync(command, store, custody, directory, generation, &|store| {
        store
            .sync_all()
            .map_err(|error| format!("Structured store sync failed: {error}"))
    });
}
pub(super) fn handle_with_sync(
    command: Command,
    store: &mut Option<muniment::FjallStore>,
    custody: Option<&SourceCustody>,
    directory: &Path,
    generation: uuid::Uuid,
    sync: &(dyn Fn(&muniment::FjallStore) -> Result<(), String> + Sync),
) {
    let Command {
        session,
        generation: request_generation,
        max_bytes,
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
                max_bytes,
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
                sync_after,
                sync,
                |store, sealer| {
                    sealed_record_receipts::inventory(store, sealer, session, max_items, max_bytes)
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
                sync_after,
                sync,
                |store, sealer| {
                    for item in sealed_record_receipts::inventory(
                        store, sealer, session, max_items, max_bytes,
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
