// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use muniment::{Backend, FjallStore};

#[test]
fn model_free_capture_library_publishes_unavailable_after_bounded_refresh_refusal() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("memory");
    let mut initial = FjallStore::open(&directory).unwrap();
    let html = "<html><main>library fixture</main></html>";
    crate::trail_memory::capture_source_document_in(
        &mut initial,
        &crate::content::FetchedDocument {
            bytes: html.as_bytes().to_vec(),
            body: html.into(),
            content_type: Some("text/html".into()),
            effective_url: Some("https://example.test/library".into()),
            acquired_at_ms: 1,
        },
    )
    .unwrap();
    drop(initial);
    let library = LocalCaptureLibrary::default();
    assert!(library.bind_custody(&crate::trail_memory::custody_test_fixture::custody()));
    let limits = crate::research_storage::StorageLimits::default();
    let owner = FjallStore::open_with_limits(&directory, limits.owner()).unwrap();
    let mut bounded = crate::research_storage::BudgetStore {
        store: &owner,
        budget: limits.budget(),
    };
    refresh_local_capture_library(&mut bounded, &directory, &library).unwrap();
    let ticket = library.ticket().unwrap();
    assert!(matches!(
        library.snapshot_for(&directory, ticket),
        CaptureLibraryState::Ready { .. }
    ));
    let mut tight = limits.clone();
    tight.operation_read.max_output_items = 0;
    let bounded = crate::research_storage::BudgetStore {
        store: &owner,
        budget: tight.budget(),
    };
    assert!(matches!(
        pollster::block_on(bounded.list("manifest:")),
        Err(muniment::StoreError::LimitExceeded {
            kind: muniment::StoreLimitKind::OutputItems,
            limit: 0
        })
    ));
    let mut bounded = crate::research_storage::BudgetStore {
        store: &owner,
        budget: tight.budget(),
    };
    let error = refresh_local_capture_library(&mut bounded, &directory, &library).unwrap_err();
    assert!(error.contains("OutputItems"));
    // This is the publication step performed by the trail actor on refresh failure.
    library.unavailable(error);
    assert!(matches!(library.snapshot_for(&directory, ticket),
        CaptureLibraryState::Unavailable(ref error) if error.contains("OutputItems")));
    let mut bounded = crate::research_storage::BudgetStore {
        store: &owner,
        budget: limits.budget(),
    };
    refresh_local_capture_library(&mut bounded, &directory, &library).unwrap();
    assert!(matches!(
        library.snapshot_for(&directory, ticket),
        CaptureLibraryState::Ready { .. }
    ));
}
