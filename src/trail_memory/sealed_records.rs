// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Strict typed structured-research payloads in the existing sealed owner.
//! The sealed save, the bounded load, the schema checks and the replay are
//! `eidetic::capture` and `eidetic_search::records::sealed` (memory-gates plan
//! S6, D17); these are the sync entry points over Turnstone's bounded stores
//! and its error type.
use super::structured_bounded_store::{Bounded, StructuredStore};
use crate::research_storage::SourceReadError as HostError;
use eidetic::capture;
use eidetic::records::{PropertyDefinitionV1, PropertyRecordV1, RecordResult};
use eidetic::{ManifestId, PayloadSealer};
#[cfg(test)]
use eidetic::Store;
use eidetic_search::records::sealed;

pub(super) fn deposit_definition(
    store: &mut dyn StructuredStore,
    sealer: &dyn PayloadSealer,
    definition: &PropertyDefinitionV1,
) -> Result<ManifestId, HostError> {
    let mut store = Bounded(&*store);
    pollster::block_on(capture::deposit_definition(&mut store, sealer, definition))
        .map_err(Into::into)
}

pub(super) fn load_definition(
    store: &mut dyn StructuredStore,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
) -> Result<PropertyDefinitionV1, HostError> {
    let mut store = Bounded(&*store);
    pollster::block_on(capture::load_definition(&mut store, sealer, id)).map_err(Into::into)
}

pub(super) fn deposit_record(
    store: &mut dyn StructuredStore,
    sealer: &dyn PayloadSealer,
    record: &PropertyRecordV1,
) -> Result<ManifestId, HostError> {
    let mut store = Bounded(&*store);
    pollster::block_on(capture::deposit_record(&mut store, sealer, record)).map_err(Into::into)
}

pub(super) fn load_record(
    store: &mut dyn StructuredStore,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
) -> Result<(PropertyRecordV1, PropertyDefinitionV1), HostError> {
    let mut store = Bounded(&*store);
    pollster::block_on(capture::load_record(&mut store, sealer, id)).map_err(Into::into)
}

pub(super) fn load_record_bounded(
    store: &mut dyn StructuredStore,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
    max_bytes: usize,
    max_validation_bytes: usize,
) -> Result<(PropertyRecordV1, PropertyDefinitionV1), HostError> {
    let mut store = Bounded(&*store);
    pollster::block_on(capture::load_record_bounded(
        &mut store,
        sealer,
        id,
        max_bytes,
        max_validation_bytes,
    ))
    .map_err(Into::into)
}

pub(super) fn deposit_result(
    store: &mut dyn StructuredStore,
    sealer: &dyn PayloadSealer,
    result: &RecordResult,
) -> Result<ManifestId, HostError> {
    let mut store = Bounded(&*store);
    pollster::block_on(sealed::deposit_result(&mut store, sealer, result)).map_err(Into::into)
}

pub(super) fn load_result(
    store: &mut dyn StructuredStore,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
) -> Result<RecordResult, HostError> {
    let mut store = Bounded(&*store);
    pollster::block_on(sealed::load_result(&mut store, sealer, id)).map_err(Into::into)
}

pub(super) fn load_result_bounded(
    store: &mut dyn StructuredStore,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
    max_bytes: usize,
    current: Option<&eidetic::records::RecordSettings>,
) -> Result<RecordResult, HostError> {
    let mut store = Bounded(&*store);
    pollster::block_on(sealed::load_result_bounded(
        &mut store, sealer, id, max_bytes, current,
    ))
    .map_err(Into::into)
}

#[cfg(test)]
#[path = "sealed_records_tests.rs"]
mod tests;
