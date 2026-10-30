// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::structured_bounded_store::StructuredStore;
use crate::research_storage::SourceReadError as HostError;
use eidetic::{ManifestId, PayloadSealer};
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    hash: eidetic::Hash,
    marker: eidetic::SealedBlobRef,
    ciphertext: Vec<u8>,
}
const LEGACY_PREFIX: &str = "turnstone/sealed-result-request/v1/";
const RESULT_PREFIX: &str = "turnstone/sealed-result-complete/v2/";
const PENDING_PREFIX: &str = "turnstone/sealed-result-pending/v2/";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ResultRequest {
    pub nonce: uuid::Uuid,
    pub session: uuid::Uuid,
    pub graph: uuid::Uuid,
    pub entity: uuid::Uuid,
    pub generation: uuid::Uuid,
    pub manifest: ManifestId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ResultReceipt {
    pub request: ResultRequest,
    pub deposited: bool,
    pub statement_id: Option<String>,
    #[serde(default)]
    pub assertion: Option<serde_json::Value>,
    pub saved: bool,
    #[serde(default)]
    pub abandoned: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundResult {
    namespace: String,
    receipt: ResultReceipt,
}
fn result_key(id: uuid::Uuid) -> String {
    format!("{RESULT_PREFIX}{id}")
}
fn pending_key(id: uuid::Uuid) -> String {
    format!("{PENDING_PREFIX}{id}")
}
pub(super) fn write_result_receipt(
    store: &mut dyn StructuredStore,
    sealer: &dyn PayloadSealer,
    receipt: &ResultReceipt,
) -> Result<(), HostError> {
    let namespace = result_key(receipt.request.nonce);
    let clear = serde_json::to_vec(&BoundResult {
        namespace: namespace.clone(),
        receipt: receipt.clone(),
    })
    .map_err(|_| "Result receipt encoding failed")?;
    let hash = eidetic::Hash::of(&clear);
    let (ciphertext, marker) = sealer
        .seal(&hash, &clear)
        .map_err(|_| "Result custody refused")?;
    let encoded = serde_json::to_vec(&Envelope {
        hash,
        marker,
        ciphertext,
    })
    .map_err(|_| "Result receipt envelope invalid")?;
    let pending = pending_key(receipt.request.nonce);
    if !receipt.saved && !receipt.abandoned {
        pollster::block_on(store.put(&pending, &encoded))?;
    } else {
        pollster::block_on(store.put(&namespace, &encoded))?;
        pollster::block_on(store.delete(&pending))?;
    }
    Ok(())
}
pub(super) fn read_result_receipt(
    store: &mut dyn StructuredStore,
    sealer: &dyn PayloadSealer,
    nonce: uuid::Uuid,
) -> Result<Option<ResultReceipt>, HostError> {
    Ok(read_result_receipt_bounded(store, sealer, nonce, usize::MAX)?.map(|(value, _)| value))
}
fn read_result_receipt_bounded(
    store: &mut dyn StructuredStore,
    sealer: &dyn PayloadSealer,
    nonce: uuid::Uuid,
    limit: usize,
) -> Result<Option<(ResultReceipt, usize)>, HostError> {
    let namespace = result_key(nonce);
    if pollster::block_on(store.get(&format!("{LEGACY_PREFIX}{nonce}")))?.is_some() {
        return Err("Unsupported prior result receipt format; preserved for recovery".into());
    }
    let pending = store.get_limited(&pending_key(nonce), limit)?;
    let bytes = match pending {
        Some(bytes) => Some(bytes),
        None => store.get_limited(&namespace, limit)?,
    };
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    if bytes.len() > limit {
        return Err("Result receipt envelope byte limit exceeded".into());
    }
    let stored_bytes = bytes.len();
    let envelope: Envelope =
        serde_json::from_slice(&bytes).map_err(|_| "Result receipt envelope invalid")?;
    let clear = sealer
        .unseal(&envelope.hash, &envelope.marker, &envelope.ciphertext)
        .map_err(|_| "Result custody refused")?;
    if clear.len() > limit {
        return Err("Result receipt clear byte limit exceeded".into());
    }
    if eidetic::Hash::of(&clear) != envelope.hash {
        return Err("Result receipt digest mismatch".into());
    }
    let held: BoundResult = serde_json::from_slice(&clear).map_err(|_| "Result receipt invalid")?;
    if held.namespace != namespace
        || held.receipt.request.nonce != nonce
        || (held.receipt.saved
            && (!held.receipt.deposited
                || held.receipt.statement_id.is_none()
                || held.receipt.assertion.is_none()))
        || held.receipt.statement_id.as_deref()
            != held
                .receipt
                .assertion
                .as_ref()
                .and_then(|v| v.get("statement_id"))
                .and_then(|v| v.as_str())
        || (held.receipt.abandoned && (held.receipt.saved || held.receipt.statement_id.is_some()))
    {
        return Err("Result receipt identity mismatch".into());
    }
    Ok(Some((held.receipt, stored_bytes)))
}
pub(super) fn deposit_result_request(
    store: &mut dyn StructuredStore,
    sealer: &dyn PayloadSealer,
    request: &ResultRequest,
    result: &eidetic::records::RecordResult,
) -> Result<ManifestId, HostError> {
    use eidetic::TypedPayload;
    let id = ManifestId::of_blob(&result.serialize_to_bytes().map_err(|e| e.to_string())?);
    if id != request.manifest {
        return Err("Result request digest mismatch".into());
    }
    let existing = read_result_receipt(store, sealer, request.nonce)?;
    if let Some(held) = &existing {
        if held.request != *request {
            return Err("Result nonce conflicts with existing request".into());
        }
        if held.abandoned {
            return Err("Result request was abandoned".into());
        }
        if held.deposited {
            if super::sealed_records::load_result(store, sealer, id)? != *result {
                return Err("Saved result differs from receipt".into());
            }
            return Ok(id);
        }
    } else {
        write_result_receipt(
            store,
            sealer,
            &ResultReceipt {
                request: request.clone(),
                deposited: false,
                statement_id: None,
                assertion: None,
                saved: false,
                abandoned: false,
            },
        )?;
    }
    if super::sealed_records::deposit_result(store, sealer, result)? != id {
        return Err("Saved result identity mismatch".into());
    }
    write_result_receipt(
        store,
        sealer,
        &ResultReceipt {
            request: request.clone(),
            deposited: true,
            statement_id: None,
            assertion: None,
            saved: false,
            abandoned: false,
        },
    )?;
    Ok(id)
}
pub(super) fn acknowledge_result(
    store: &mut dyn StructuredStore,
    sealer: &dyn PayloadSealer,
    request: &ResultRequest,
    statement: &str,
) -> Result<(), HostError> {
    let mut held =
        read_result_receipt(store, sealer, request.nonce)?.ok_or("Result receipt missing")?;
    if held.request != *request
        || held.abandoned
        || !held.deposited
        || statement.trim().is_empty()
        || held.statement_id.as_deref() != Some(statement)
        || held.assertion.is_none()
    {
        return Err("Result acknowledgment mismatch".into());
    }
    super::sealed_records::load_result(store, sealer, request.manifest)?;
    held.statement_id = Some(statement.into());
    held.saved = true;
    write_result_receipt(store, sealer, &held)
}
pub(super) fn mark_result_attached(
    store: &mut dyn StructuredStore,
    sealer: &dyn PayloadSealer,
    request: &ResultRequest,
    statement: &str,
    assertion: &serde_json::Value,
) -> Result<(), HostError> {
    let mut held =
        read_result_receipt(store, sealer, request.nonce)?.ok_or("Result receipt missing")?;
    if held.request != *request
        || !held.deposited
        || held.saved
        || held.abandoned
        || statement.trim().is_empty()
        || held
            .statement_id
            .as_deref()
            .is_some_and(|previous| previous != statement)
        || assertion.get("statement_id").and_then(|v| v.as_str()) != Some(statement)
        || held
            .assertion
            .as_ref()
            .is_some_and(|previous| previous != assertion)
    {
        return Err("Result attachment does not match receipt".into());
    }
    held.statement_id = Some(statement.into());
    held.assertion = Some(assertion.clone());
    write_result_receipt(store, sealer, &held)
}
pub(super) fn result_receipts(
    store: &mut dyn StructuredStore,
    sealer: &dyn PayloadSealer,
    max_items: usize,
    max_bytes: usize,
    legacy_checked: bool,
) -> Result<(Vec<ResultReceipt>, usize), HostError> {
    if !legacy_checked && !pollster::block_on(store.list(LEGACY_PREFIX))?.is_empty() {
        return Err("Unsupported prior result receipt format; preserved for recovery".into());
    }
    let key_bytes = max_items.saturating_mul(PENDING_PREFIX.len() + 36);
    let keys = store.list_inventory(PENDING_PREFIX, max_items, key_bytes)?;
    if keys.len() > max_items {
        return Err("Result recovery inventory limit exceeded".into());
    }
    let mut values = Vec::new();
    let mut bytes = 0usize;
    for key in keys {
        let suffix = key
            .strip_prefix(PENDING_PREFIX)
            .ok_or("Invalid result recovery key")?;
        let nonce = uuid::Uuid::parse_str(suffix).map_err(|_| "Invalid result recovery nonce")?;
        if key != pending_key(nonce) {
            return Err("Noncanonical result recovery key".into());
        }
        let (value, used) =
            read_result_receipt_bounded(store, sealer, nonce, max_bytes.saturating_sub(bytes))?
                .ok_or("Result recovery receipt disappeared")?;
        bytes = bytes
            .checked_add(used)
            .ok_or("Result recovery inventory overflow")?;
        if bytes > max_bytes {
            return Err("Result recovery inventory byte limit exceeded".into());
        }
        values.push(value);
    }
    Ok((values, bytes))
}
