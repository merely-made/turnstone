// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;

pub(super) fn apply(
    state: &mut Option<(FjallStore, BrowsingMemory)>,
    sealed: &mut Option<FjallStore>,
    binding: &SourceCustody,
    limits: &Result<crate::research_storage::StorageLimits, String>,
    memory_error: &Option<crate::research_storage::SourceReadError>,
    sealed_error: &Option<crate::research_storage::SourceReadError>,
    request: &crate::source_capture::CaptureRequest,
    document: Option<&crate::content::FetchedDocument>,
    acknowledge: bool,
) -> Result<Option<StoredSourceDocument>, crate::research_storage::SourceReadError> {
    let budget = limits.as_ref().map_err(Clone::clone)?.budget();
    let store = &mut state
        .as_mut()
        .ok_or_else(|| {
            memory_error
                .clone()
                .unwrap_or_else(|| "legacy capture store unavailable".into())
        })?
        .0;
    let mut legacy = crate::research_storage::BudgetStore {
        store,
        budget: budget.clone(),
    };
    let legacy_key = format!("turnstone/source-capture-request/v1/{}", request.id);
    let existing = pollster::block_on(legacy.get(&legacy_key))?;
    if existing.is_some() {
        if acknowledge {
            source_capture_receipts::acknowledge_saved(&mut legacy, request)?;
            return Ok(None);
        }
        return source_capture_receipts::completed_legacy(&mut legacy, request).map(Some);
    }
    let store = sealed.as_mut().ok_or_else(|| {
        sealed_error
            .clone()
            .unwrap_or_else(|| "sealed capture store unavailable".into())
    })?;
    let mut sealed = crate::research_storage::BudgetStore { store, budget };
    if acknowledge {
        sealed_capture::acknowledge(&mut sealed, binding.sealer.as_ref(), request)?;
        Ok(None)
    } else {
        sealed_capture::capture_request(&mut sealed, binding.sealer.as_ref(), request, document)
            .map(Some)
    }
}

pub(super) fn refresh(
    store: &FjallStore,
    directory: &Path,
    library: &crate::place::captured_collection::LocalCaptureLibrary,
    limits: &Result<crate::research_storage::StorageLimits, String>,
) -> Result<(), String> {
    let mut bounded = crate::research_storage::BudgetStore {
        store,
        budget: limits.as_ref().map_err(Clone::clone)?.budget(),
    };
    crate::place::captured_collection::refresh_local_capture_library(
        &mut bounded,
        directory,
        library,
    )
}

#[cfg(test)]
mod review_tests {
    use super::*;
    #[test]
    fn model_free_capture_and_ack_preserve_both_owner_failures() {
        let root = tempfile::tempdir().unwrap();
        let store = FjallStore::open_with_limits(
            root.path(),
            crate::research_storage::StorageLimits::default().owner(),
        )
        .unwrap();
        let request = crate::source_capture::CaptureRequest {
            id: uuid::Uuid::new_v4(),
            session: crate::panes::SessionId::new(),
            graph: crate::panes::GraphId::new(),
            node: uuid::Uuid::new_v4(),
            url: "https://example.test".into(),
            source: None,
            keep: true,
        };
        let failure = crate::research_storage::SourceReadError::Storage(
            muniment::StoreError::LimitExceeded {
                kind: muniment::StoreLimitKind::ValueBytes,
                limit: 16,
            },
        );
        let custody = custody_test_fixture::custody();
        let limits = Ok(crate::research_storage::StorageLimits::default());
        for acknowledge in [false, true] {
            let mut state = None;
            let mut sealed = None;
            assert_eq!(
                apply(
                    &mut state,
                    &mut sealed,
                    &custody,
                    &limits,
                    &Some(failure.clone()),
                    &None,
                    &request,
                    None,
                    acknowledge
                )
                .unwrap_err(),
                failure
            );
        }
        let mut state = Some((store, BrowsingMemory::new(64)));
        for acknowledge in [false, true] {
            let mut sealed = None;
            assert_eq!(
                apply(
                    &mut state,
                    &mut sealed,
                    &custody,
                    &limits,
                    &None,
                    &Some(failure.clone()),
                    &request,
                    None,
                    acknowledge
                )
                .unwrap_err(),
                failure
            );
        }
    }
}
