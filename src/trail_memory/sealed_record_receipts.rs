// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Encrypted deposit intents and save acknowledgements for structured records.
use super::sealed_result_receipts::{
    ResultReceipt, read_result_receipt, result_receipts, write_result_receipt,
};
use eidetic::records::{PropertyDefinitionV1, PropertyRecordV1};
use eidetic::{Backend as _, ManifestId, PayloadSealer, Store};
use serde::{Deserialize, Serialize};

const LEGACY_PREFIX: &str = "turnstone/sealed-property-request/v1/";
const PREFIX: &str = "turnstone/sealed-property-complete/v2/";
const PENDING_PREFIX: &str = "turnstone/sealed-property-pending/v2/";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DepositRequest {
    pub nonce: uuid::Uuid,
    pub session: uuid::Uuid,
    pub graph: uuid::Uuid,
    pub generation: uuid::Uuid,
    pub definition: PropertyDefinitionV1,
    pub record: PropertyRecordV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Receipt {
    pub request: DepositRequest,
    pub manifest: Option<ManifestId>,
    pub statement_id: Option<String>,
    #[serde(default)]
    pub assertion: Option<serde_json::Value>,
    pub saved: bool,
    #[serde(default)]
    pub abandoned: bool,
    #[serde(default)]
    pub withdrawal: Option<Withdrawal>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Withdrawal {
    pub statement_id: String,
    pub saved: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    hash: eidetic::Hash,
    marker: eidetic::SealedBlobRef,
    ciphertext: Vec<u8>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bound {
    namespace: String,
    receipt: Receipt,
}
fn key(nonce: uuid::Uuid) -> String {
    format!("{PREFIX}{nonce}")
}
fn pending_key(nonce: uuid::Uuid) -> String {
    format!("{PENDING_PREFIX}{nonce}")
}
fn write_receipt(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    receipt: &Receipt,
) -> Result<(), String> {
    let namespace = key(receipt.request.nonce);
    let clear = serde_json::to_vec(&Bound {
        namespace: namespace.clone(),
        receipt: receipt.clone(),
    })
    .map_err(|_| "Property receipt encoding failed")?;
    let hash = eidetic::Hash::of(&clear);
    let (ciphertext, marker) = sealer
        .seal(&hash, &clear)
        .map_err(|_| "Property custody refused")?;
    let encoded = serde_json::to_vec(&Envelope {
        hash,
        marker,
        ciphertext,
    })
    .map_err(|_| "Property receipt envelope invalid")?;
    let pending = pending_key(receipt.request.nonce);
    if (!receipt.saved && !receipt.abandoned)
        || receipt
            .withdrawal
            .as_ref()
            .is_some_and(|value| !value.saved)
    {
        pollster::block_on(store.put(&pending, &encoded))
            .map_err(|e| format!("Property pending receipt write failed: {e}"))?;
    } else {
        pollster::block_on(store.put(&namespace, &encoded))
            .map_err(|e| format!("Property completion write failed: {e}"))?;
        pollster::block_on(store.delete(&pending))
            .map_err(|e| format!("Property pending receipt cleanup failed: {e}"))?;
    }
    Ok(())
}
pub(super) fn read_receipt(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    nonce: uuid::Uuid,
) -> Result<Option<Receipt>, String> {
    Ok(read_receipt_bounded(store, sealer, nonce, usize::MAX)?.map(|(value, _)| value))
}
fn read_receipt_bounded(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    nonce: uuid::Uuid,
    limit: usize,
) -> Result<Option<(Receipt, usize)>, String> {
    let namespace = key(nonce);
    if pollster::block_on(store.get(&format!("{LEGACY_PREFIX}{nonce}")))
        .map_err(|e| format!("Legacy property receipt read failed: {e}"))?
        .is_some()
    {
        return Err("Unsupported prior property receipt format; preserved for recovery".into());
    }
    let pending = pollster::block_on(store.get(&pending_key(nonce)))
        .map_err(|e| format!("Property pending receipt read failed: {e}"))?;
    let bytes = match pending {
        Some(bytes) => Some(bytes),
        None => pollster::block_on(store.get(&namespace))
            .map_err(|e| format!("Property receipt read failed: {e}"))?,
    };
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    if bytes.len() > limit {
        return Err("Property receipt envelope byte limit exceeded".into());
    }
    let stored_bytes = bytes.len();
    let envelope: Envelope =
        serde_json::from_slice(&bytes).map_err(|_| "Property receipt envelope invalid")?;
    let clear = sealer
        .unseal(&envelope.hash, &envelope.marker, &envelope.ciphertext)
        .map_err(|_| "Property custody refused")?;
    if clear.len() > limit {
        return Err("Property receipt clear byte limit exceeded".into());
    }
    if eidetic::Hash::of(&clear) != envelope.hash {
        return Err("Property receipt digest mismatch".into());
    }
    let held: Bound = serde_json::from_slice(&clear).map_err(|_| "Property receipt invalid")?;
    if held.namespace != namespace
        || held.receipt.request.nonce != nonce
        || held.receipt.request.record.id != nonce.to_string()
        || (held.receipt.saved
            && (held.receipt.manifest.is_none()
                || held.receipt.statement_id.is_none()
                || held.receipt.assertion.is_none()))
        || (held.receipt.statement_id.is_some() && held.receipt.manifest.is_none())
        || held.receipt.statement_id.as_deref()
            != held
                .receipt
                .assertion
                .as_ref()
                .and_then(|v| v.get("statement_id"))
                .and_then(|v| v.as_str())
        || (held.receipt.abandoned && (held.receipt.saved || held.receipt.statement_id.is_some()))
    {
        return Err("Property receipt identity mismatch".into());
    }
    Ok(Some((held.receipt, stored_bytes)))
}
pub(super) fn deposit_request(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    request: &DepositRequest,
) -> Result<ManifestId, String> {
    if request.record.id != request.nonce.to_string() {
        return Err("Property nonce mismatch".into());
    }
    request
        .record
        .validate(&request.definition)
        .map_err(|e| format!("Invalid property: {e:?}"))?;
    let existing = read_receipt(store, sealer, request.nonce)?;
    if let Some(held) = &existing {
        if held.request != *request {
            return Err("Property nonce conflicts with existing request".into());
        }
        if held.abandoned {
            return Err("Property request was abandoned".into());
        }
        if let Some(id) = held.manifest {
            let (record, definition) = super::sealed_records::load_record(store, sealer, id)?;
            if record != request.record || definition != request.definition {
                return Err("Property deposit differs from receipt".into());
            }
            return Ok(id);
        }
    }
    if existing.is_none() {
        write_receipt(
            store,
            sealer,
            &Receipt {
                request: request.clone(),
                manifest: None,
                statement_id: None,
                assertion: None,
                saved: false,
                abandoned: false,
                withdrawal: None,
            },
        )?;
    }
    let definition = super::sealed_records::deposit_definition(store, sealer, &request.definition)?;
    if definition != request.record.definition_manifest {
        return Err("Property definition identity mismatch".into());
    }
    let id = super::sealed_records::deposit_record(store, sealer, &request.record)?;
    write_receipt(
        store,
        sealer,
        &Receipt {
            request: request.clone(),
            manifest: Some(id),
            statement_id: None,
            assertion: None,
            saved: false,
            abandoned: false,
            withdrawal: None,
        },
    )?;
    Ok(id)
}

pub(super) fn prepare_withdrawal(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    nonce: uuid::Uuid,
    session: uuid::Uuid,
    graph: uuid::Uuid,
    manifest: ManifestId,
    statement: &str,
) -> Result<(), String> {
    let mut receipt =
        read_receipt(store, sealer, nonce)?.ok_or("Property receipt unavailable for withdrawal")?;
    if !receipt.saved
        || receipt.request.session != session
        || receipt.request.graph != graph
        || receipt.manifest != Some(manifest)
        || receipt.statement_id.as_deref() != Some(statement)
    {
        return Err("Withdrawal does not match the saved property".into());
    }
    if let Some(held) = &receipt.withdrawal {
        if held.statement_id != statement || held.saved {
            return Err("Property withdrawal is already complete or differs".into());
        }
        return Ok(());
    }
    receipt.withdrawal = Some(Withdrawal {
        statement_id: statement.into(),
        saved: false,
    });
    write_receipt(store, sealer, &receipt)
}
pub(super) fn acknowledge_withdrawal(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    nonce: uuid::Uuid,
    session: uuid::Uuid,
    graph: uuid::Uuid,
    manifest: ManifestId,
    statement: &str,
) -> Result<(), String> {
    let mut receipt =
        read_receipt(store, sealer, nonce)?.ok_or("Property receipt unavailable for withdrawal")?;
    if !receipt.saved
        || receipt.request.session != session
        || receipt.request.graph != graph
        || receipt.manifest != Some(manifest)
        || receipt.statement_id.as_deref() != Some(statement)
    {
        return Err("Withdrawal does not match the saved property".into());
    }
    let withdrawal = receipt
        .withdrawal
        .as_mut()
        .ok_or("Withdrawal intent missing")?;
    if withdrawal.statement_id != statement {
        return Err("Withdrawal assertion mismatch".into());
    }
    withdrawal.saved = true;
    write_receipt(store, sealer, &receipt)
}
pub(super) fn acknowledge(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    request: &DepositRequest,
    id: ManifestId,
    statement_id: &str,
) -> Result<(), String> {
    let mut held = read_receipt(store, sealer, request.nonce)?.ok_or("Property receipt missing")?;
    if held.request != *request
        || held.abandoned
        || held.manifest != Some(id)
        || statement_id.trim().is_empty()
        || held.statement_id.as_deref() != Some(statement_id)
        || held.assertion.is_none()
    {
        return Err("Property acknowledgement does not match attached deposit".into());
    }
    let (record, definition) = super::sealed_records::load_record(store, sealer, id)?;
    if record != request.record || definition != request.definition {
        return Err("Property payload changed before acknowledgement".into());
    }
    held.statement_id = Some(statement_id.into());
    held.saved = true;
    write_receipt(store, sealer, &held)
}
pub(super) fn mark_attached(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    request: &DepositRequest,
    id: ManifestId,
    statement_id: &str,
    assertion: &serde_json::Value,
) -> Result<(), String> {
    let mut receipt =
        read_receipt(store, sealer, request.nonce)?.ok_or("Property receipt missing")?;
    if receipt.request != *request
        || receipt.manifest != Some(id)
        || receipt.saved
        || receipt.abandoned
        || statement_id.trim().is_empty()
        || receipt
            .statement_id
            .as_deref()
            .is_some_and(|previous| previous != statement_id)
        || assertion.get("statement_id").and_then(|v| v.as_str()) != Some(statement_id)
        || receipt
            .assertion
            .as_ref()
            .is_some_and(|previous| previous != assertion)
    {
        return Err("Property attachment does not match receipt".into());
    }
    receipt.statement_id = Some(statement_id.into());
    receipt.assertion = Some(assertion.clone());
    write_receipt(store, sealer, &receipt)
}

pub(super) fn receipts(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    max_items: usize,
    max_bytes: usize,
) -> Result<(Vec<Receipt>, usize), String> {
    if !pollster::block_on(store.list(LEGACY_PREFIX))
        .map_err(|e| format!("Legacy property inventory failed: {e}"))?
        .is_empty()
    {
        return Err("Unsupported prior property receipt format; preserved for recovery".into());
    }
    let keys = pollster::block_on(store.list(PENDING_PREFIX))
        .map_err(|e| format!("Property recovery inventory failed: {e}"))?;
    if keys.len() > max_items {
        return Err("Property recovery inventory limit exceeded".into());
    }
    let mut values = Vec::new();
    let mut bytes = 0usize;
    for key in keys {
        let suffix = key
            .strip_prefix(PENDING_PREFIX)
            .ok_or("Invalid property recovery key")?;
        let nonce = uuid::Uuid::parse_str(suffix).map_err(|_| "Invalid property recovery nonce")?;
        if key != pending_key(nonce) {
            return Err("Noncanonical property recovery key".into());
        }
        let (value, used) =
            read_receipt_bounded(store, sealer, nonce, max_bytes.saturating_sub(bytes))?
                .ok_or("Property recovery receipt disappeared")?;
        bytes = bytes
            .checked_add(used)
            .ok_or("Property recovery inventory overflow")?;
        if bytes > max_bytes {
            return Err("Property recovery inventory byte limit exceeded".into());
        }
        values.push(value);
    }
    Ok((values, bytes))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryItem {
    Property(Receipt),
    Result(ResultReceipt),
}
impl RecoveryItem {
    pub(crate) fn nonce(&self) -> uuid::Uuid {
        match self {
            Self::Property(r) => r.request.nonce,
            Self::Result(r) => r.request.nonce,
        }
    }
    pub(crate) fn orphan(&self) -> bool {
        match self {
            Self::Property(r) => !r.saved && r.statement_id.is_none(),
            Self::Result(r) => !r.saved && r.statement_id.is_none(),
        }
    }
}
pub(super) fn inventory(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    session: uuid::Uuid,
    max_items: usize,
    max_bytes: usize,
) -> Result<Vec<RecoveryItem>, String> {
    let mut values = Vec::new();
    let (properties, property_bytes) = receipts(store, sealer, max_items, max_bytes)?;
    for receipt in properties {
        if receipt.request.session != session {
            return Err("Property receipt belongs to another session".into());
        }
        if (!receipt.saved && !receipt.abandoned)
            || receipt.withdrawal.as_ref().is_some_and(|w| !w.saved)
        {
            values.push(RecoveryItem::Property(receipt));
        }
    }
    let (results, _) = result_receipts(
        store,
        sealer,
        max_items.saturating_sub(values.len()),
        max_bytes.saturating_sub(property_bytes),
    )?;
    for receipt in results {
        if receipt.request.session != session {
            return Err("Result receipt belongs to another session".into());
        }
        if !receipt.saved && !receipt.abandoned {
            values.push(RecoveryItem::Result(receipt));
        }
    }
    if values.len() > max_items {
        return Err("Recovery inventory limit exceeded".into());
    }
    let mut total = 0usize;
    for item in &values {
        let bytes = match item {
            RecoveryItem::Property(r) => serde_json::to_vec(r),
            RecoveryItem::Result(r) => serde_json::to_vec(r),
        }
        .map_err(|e| e.to_string())?;
        total = total
            .checked_add(bytes.len())
            .ok_or("Recovery inventory size overflow")?;
        if total > max_bytes {
            return Err("Recovery inventory byte limit exceeded".into());
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    if values.iter().any(|item| !ids.insert(item.nonce())) {
        return Err("Ambiguous recovery nonce".into());
    }
    Ok(values)
}
pub(super) fn abandon(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    session: uuid::Uuid,
    nonce: uuid::Uuid,
    bindings: &[(uuid::Uuid, ManifestId)],
) -> Result<(), String> {
    let property = read_receipt(store, sealer, nonce)?;
    let result = read_result_receipt(store, sealer, nonce)?;
    match (property, result) {
        (Some(mut held), None) => {
            if held.request.session != session
                || held.saved
                || held.statement_id.is_some()
                || held.withdrawal.is_some()
                || held
                    .manifest
                    .is_some_and(|id| bindings.contains(&(held.request.graph, id)))
            {
                return Err("Only an unbound property deposit can be abandoned".into());
            }
            held.abandoned = true;
            write_receipt(store, sealer, &held)
        },
        (None, Some(mut held)) => {
            if held.request.session != session
                || held.saved
                || held.statement_id.is_some()
                || (held.deposited
                    && bindings.contains(&(held.request.graph, held.request.manifest)))
            {
                return Err("Only an unbound result deposit can be abandoned".into());
            }
            held.abandoned = true;
            write_result_receipt(store, sealer, &held)
        },
        (None, None) => Err("Recovery receipt unavailable".into()),
        _ => Err("Ambiguous recovery nonce".into()),
    }
}
