// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Strict typed structured-research payloads in the existing sealed owner.
use eidetic::records::{PropertyDefinitionV1, PropertyRecordV1, RecordResult};
use eidetic::{ManifestId, PayloadSealer, Store, TypedPayload};

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
    let value: PropertyDefinitionV1 = super::sealed_capture::load(store, sealer, id)?;
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
    let record: PropertyRecordV1 = super::sealed_capture::load(store, sealer, id)?;
    schema(store, &record)?;
    let definition = load_definition(store, sealer, record.definition_manifest)?;
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
    let result: RecordResult = super::sealed_capture::load(store, sealer, id)?;
    schema(store, &result)?;
    let replay =
        eidetic_search::records::evaluate(&result.snapshot, &result.query, &result.settings)
            .map_err(|e| format!("Saved result input refused: {e:?}"))?;
    if replay != result {
        return Err("Saved result does not match its frozen inputs".into());
    }
    Ok(result)
}

#[cfg(test)]
#[path = "sealed_records_tests.rs"]
mod tests;
