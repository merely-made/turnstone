// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Strict typed structured-research payloads in the existing sealed owner.
use eidetic::records::{PropertyDefinitionV1, PropertyRecordV1, RecordResult};
use eidetic::{ManifestId, PayloadSealer, Store, TypedPayload};

// Check both manifest and ciphertext before decoding or unsealing. The current
// Store::get interface still materializes the stored value before this check.
fn load_bounded<T: TypedPayload>(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
    max_bytes: usize,
) -> Result<(T, usize), String> {
    use eidetic::Backend as _;
    let manifest_bytes = pollster::block_on(store.get(&id.store_key()))
        .map_err(|_| "Structured manifest read failed")?
        .ok_or("Structured manifest missing")?;
    if manifest_bytes.len() > max_bytes {
        return Err("Structured manifest byte limit exceeded".into());
    }
    let manifest: eidetic::BlobManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| "Structured manifest invalid")?;
    let key = format!("blob:{}", id.0.to_hex());
    let marker = eidetic::seal::seal_marker(&manifest)
        .map_err(|_| "Structured seal marker invalid")?
        .ok_or("Structured artifact is not sealed")?;
    if manifest.id != id
        || manifest.content_hash != id.0
        || manifest.schema != T::schema_ref()
        || manifest.privacy != eidetic::PrivacyClass::LocalOnly
        || manifest.sources.len() != 1
        || !matches!(&manifest.sources[0], eidetic::BlobSource::Local { key: held } if held == &key)
        || manifest.byte_size > max_bytes as u64
    {
        return Err("Structured artifact metadata or byte limit refused".into());
    }
    let ciphertext = pollster::block_on(store.get(&key))
        .map_err(|_| "Structured ciphertext read failed")?
        .ok_or("Structured ciphertext missing")?;
    if ciphertext.len() > max_bytes {
        return Err("Structured ciphertext byte limit exceeded".into());
    }
    let clear = sealer
        .unseal(&manifest.content_hash, &marker, &ciphertext)
        .map_err(|_| "Structured custody or integrity refusal")?;
    if clear.len() > max_bytes
        || clear.len() as u64 != manifest.byte_size
        || eidetic::Hash::of(&clear) != id.0
    {
        return Err("Structured payload size or digest mismatch".into());
    }
    let value = T::deserialize_from_bytes(&clear).map_err(|_| "Structured payload invalid")?;
    // These three record payloads use TypedPayload's canonical JSON encoding.
    struct Compare<'a> {
        expected: &'a [u8],
        offset: usize,
    }
    impl std::io::Write for Compare<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let end = self
                .offset
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("identity overflow"))?;
            if self.expected.get(self.offset..end) != Some(bytes) {
                return Err(std::io::Error::other("identity mismatch"));
            }
            self.offset = end;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut compare = Compare {
        expected: &clear,
        offset: 0,
    };
    serde_json::to_writer(&mut compare, &value)
        .map_err(|_| "Structured payload canonical identity mismatch")?;
    if compare.offset != clear.len() {
        return Err("Structured payload canonical identity mismatch".into());
    }
    Ok((value, clear.len()))
}

fn schema<T: TypedPayload>(store: &mut dyn Store, value: &T) -> Result<(), String> {
    pollster::block_on(eidetic::records::validate_required_schema(
        store,
        &mut eidetic::NoFetcher,
        value,
    ))
    .map_err(|e| format!("Required record schema refused: {e}"))
}

pub(super) fn deposit_definition(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    definition: &PropertyDefinitionV1,
) -> Result<ManifestId, String> {
    definition
        .validate()
        .map_err(|e| format!("Invalid property definition: {e:?}"))?;
    pollster::block_on(eidetic::records::bootstrap_record_schemas(store))
        .map_err(|e| format!("Record schema bootstrap failed: {e}"))?;
    schema(store, definition)?;
    let id = super::sealed_capture::save(store, sealer, definition, 0)?;
    if id != definition.manifest_id().map_err(|e| e.to_string())? {
        return Err("Property definition identity mismatch".into());
    }
    if load_definition(store, sealer, id)? != *definition {
        return Err("Property definition readback mismatch".into());
    }
    Ok(id)
}

pub(super) fn load_definition(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
) -> Result<PropertyDefinitionV1, String> {
    load_definition_bounded(store, sealer, id, usize::MAX)
}
fn load_definition_bounded(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
    max_bytes: usize,
) -> Result<PropertyDefinitionV1, String> {
    let (value, _): (PropertyDefinitionV1, _) = load_bounded(store, sealer, id, max_bytes)?;
    schema(store, &value)?;
    value
        .validate()
        .map_err(|e| format!("Invalid property definition: {e:?}"))?;
    if value.manifest_id().map_err(|e| e.to_string())? != id {
        return Err("Property definition identity mismatch".into());
    }
    Ok(value)
}

pub(super) fn deposit_record(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    record: &PropertyRecordV1,
) -> Result<ManifestId, String> {
    let definition = load_definition(store, sealer, record.definition_manifest)?;
    record
        .validate(&definition)
        .map_err(|e| format!("Invalid property record: {e:?}"))?;
    schema(store, record)?;
    let id = super::sealed_capture::save(store, sealer, record, 0)?;
    if load_record(store, sealer, id)?.0 != *record {
        return Err("Property record readback mismatch".into());
    }
    Ok(id)
}

pub(super) fn load_record(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
) -> Result<(PropertyRecordV1, PropertyDefinitionV1), String> {
    load_record_bounded(store, sealer, id, usize::MAX, usize::MAX)
}
pub(super) fn load_record_bounded(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
    max_bytes: usize,
    max_validation_bytes: usize,
) -> Result<(PropertyRecordV1, PropertyDefinitionV1), String> {
    let (record, used): (PropertyRecordV1, _) = load_bounded(store, sealer, id, max_bytes)?;
    schema(store, &record)?;
    let definition = load_definition_bounded(
        store,
        sealer,
        record.definition_manifest,
        max_bytes
            .checked_sub(used)
            .ok_or("Structured record byte limit exceeded")?
            .min(max_validation_bytes),
    )?;
    record
        .validate(&definition)
        .map_err(|e| format!("Invalid property record: {e:?}"))?;
    Ok((record, definition))
}

pub(super) fn deposit_result(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    result: &RecordResult,
) -> Result<ManifestId, String> {
    pollster::block_on(eidetic::records::bootstrap_record_schemas(store))
        .map_err(|e| format!("Record schema bootstrap failed: {e}"))?;
    schema(store, result)?;
    let replay =
        eidetic_search::records::evaluate(&result.snapshot, &result.query, &result.settings)
            .map_err(|e| format!("Saved result input refused: {e:?}"))?;
    if replay != *result {
        return Err("Saved result does not match its frozen inputs".into());
    }
    let id = super::sealed_capture::save(store, sealer, result, 0)?;
    if load_result(store, sealer, id)? != *result {
        return Err("Saved result readback mismatch".into());
    }
    Ok(id)
}

pub(super) fn load_result(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
) -> Result<RecordResult, String> {
    load_result_bounded(store, sealer, id, usize::MAX, None)
}
pub(super) fn load_result_bounded(
    store: &mut dyn Store,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
    max_bytes: usize,
    current: Option<&eidetic::records::RecordSettings>,
) -> Result<RecordResult, String> {
    let (result, _): (RecordResult, _) = load_bounded(store, sealer, id, max_bytes)?;
    schema(store, &result)?;
    let mut effective = result.settings.clone();
    if let Some(current) = current {
        effective.max_sources = effective.max_sources.min(current.max_sources);
        effective.max_records = effective.max_records.min(current.max_records);
        effective.max_work = effective.max_work.min(current.max_work);
        effective.max_validation_bytes = effective
            .max_validation_bytes
            .min(current.max_validation_bytes);
        effective.max_rows = effective.max_rows.min(current.max_rows);
        effective.max_input_bytes = effective.max_input_bytes.min(current.max_input_bytes);
        effective.max_output_bytes = effective.max_output_bytes.min(current.max_output_bytes);
    }
    let replay = eidetic_search::records::evaluate(&result.snapshot, &result.query, &effective)
        .map_err(|e| format!("Saved result input refused: {e:?}"))?;
    if replay.operation_version != result.operation_version || replay.rows != result.rows {
        return Err("Saved result does not match its frozen inputs".into());
    }
    Ok(result)
}

#[cfg(test)]
#[path = "sealed_records_tests.rs"]
mod tests;
