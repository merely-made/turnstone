// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use muniment::Backend;

#[test]
fn model_free_comparison_storage_error_keeps_side_context() {
    let failure = SourceReadError::Storage(muniment::StoreError::LimitExceeded {
        kind: muniment::StoreLimitKind::ValueBytes,
        limit: 16,
    });
    let left = failure.clone().context("Left unavailable");
    assert!(left.to_string().contains("Left unavailable"));
    assert_eq!(left.storage_error(), failure.storage_error());
    assert!(left.to_string().contains("ValueBytes"));
    let right = failure.clone().context("Right unavailable");
    assert!(right.to_string().contains("Right unavailable"));
    assert_eq!(right.storage_error(), failure.storage_error());
}

#[test]
fn model_free_storage_error_survives_eidetic_source_boundary() {
    let failure = muniment::StoreError::LimitExceeded {
        kind: muniment::StoreLimitKind::BlockDecodedBytes,
        limit: 12,
    };
    let wrapped = eidetic::Error::from(failure.clone());
    assert!(
        matches!(SourceReadError::from(wrapped), SourceReadError::Storage(error) if error == failure)
    );
}

#[test]
fn model_free_storage_open_refusal_preserves_existing_data_and_can_retry() {
    fn files(
        root: &std::path::Path,
        dir: &std::path::Path,
        out: &mut Vec<(std::path::PathBuf, Vec<u8>)>,
    ) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files(root, &path, out);
            } else if path.is_file() {
                out.push((
                    path.strip_prefix(root).unwrap().to_owned(),
                    std::fs::read(path).unwrap(),
                ));
            }
        }
    }
    fn snapshot(root: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        files(root, root, &mut out);
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
    let root = tempfile::tempdir().unwrap();
    let owner = muniment::FjallStore::open(root.path()).unwrap();
    pollster::block_on(owner.put("retained", &vec![42; 8192])).unwrap();
    owner.sync_all().unwrap();
    drop(owner);
    let before = snapshot(root.path());
    let mut limits = StorageLimits::default();
    limits.recovery.max_value_bytes = 16;
    assert!(matches!(
        muniment::FjallStore::open_with_limits(root.path(), limits.owner()),
        Err(muniment::StoreError::LimitExceeded { .. })
    ));
    assert_eq!(snapshot(root.path()), before);
    let reopened =
        muniment::FjallStore::open_with_limits(root.path(), StorageLimits::default().owner())
            .unwrap();
    assert_eq!(
        pollster::block_on(reopened.get("retained")).unwrap(),
        Some(vec![42; 8192])
    );
}

#[test]
fn model_free_storage_owner_ceilings_apply_to_ordinary_backend_calls() {
    let root = tempfile::tempdir().unwrap();
    let initial = muniment::FjallStore::open(root.path()).unwrap();
    pollster::block_on(initial.put("stored", &[1; 32])).unwrap();
    initial.sync_all().unwrap();
    drop(initial);
    let mut limits = StorageLimits::default();
    limits.owner_read.max_value_bytes = 8;
    let store = muniment::FjallStore::open_with_limits(root.path(), limits.owner()).unwrap();
    assert!(matches!(
        pollster::block_on(store.get("stored")),
        Err(muniment::StoreError::LimitExceeded { .. })
    ));
    drop(store);
    limits.owner_read.max_value_bytes = 64;
    limits.owner_read.max_output_items = 0;
    let store = muniment::FjallStore::open_with_limits(root.path(), limits.owner()).unwrap();
    assert!(matches!(
        pollster::block_on(store.list("")),
        Err(muniment::StoreError::LimitExceeded { .. })
    ));
    assert!(matches!(
        pollster::block_on(store.scan("", "z")),
        Err(muniment::StoreError::LimitExceeded { .. })
    ));
}

#[test]
fn model_free_negative_prefix_lookup_refuses_when_budget_is_exhausted() {
    let root = tempfile::tempdir().unwrap();
    let limits = StorageLimits::default();
    let store = muniment::FjallStore::open_with_limits(root.path(), limits.owner()).unwrap();
    pollster::block_on(store.put("retained/present", b"value")).unwrap();
    let mut tight = limits.clone();
    tight.operation_read.max_alloc_bytes = 0;
    let adapter = BudgetStore {
        store: &store,
        budget: tight.budget(),
    };
    assert!(matches!(
        adapter.exists("retained/missing"),
        Err(muniment::StoreError::LimitExceeded {
            kind: muniment::StoreLimitKind::AllocationBytes,
            limit: 0
        })
    ));
    let adapter = BudgetStore {
        store: &store,
        budget: limits.budget(),
    };
    assert!(!adapter.exists("retained/missing").unwrap());
    assert!(adapter.exists("retained/present").unwrap());
}

#[test]
fn model_free_storage_budget_is_shared_across_distinct_owners() {
    let root = tempfile::tempdir().unwrap();
    let limits = StorageLimits::default();
    let first =
        muniment::FjallStore::open_with_limits(root.path().join("first"), limits.owner()).unwrap();
    let second =
        muniment::FjallStore::open_with_limits(root.path().join("second"), limits.owner()).unwrap();
    pollster::block_on(first.put("first", b"one")).unwrap();
    pollster::block_on(second.put("second", b"two")).unwrap();
    let mut low = 0;
    let mut high = 1_000_000;
    while low < high {
        let middle = (low + high) / 2;
        let mut trial = limits.clone();
        trial.operation_read.max_visited_items = middle;
        let each_fits = [(&first, "first"), (&second, "second")]
            .into_iter()
            .all(|(store, key)| {
                let adapter = BudgetStore {
                    store,
                    budget: trial.budget(),
                };
                pollster::block_on(adapter.get(key)).is_ok_and(|value| value.is_some())
            });
        if each_fits {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    assert!(low > 0 && low < 1_000_000);
    let mut small = limits.clone();
    small.operation_read.max_visited_items = low;
    let budget = small.budget();
    let a = BudgetStore {
        store: &first,
        budget: budget.clone(),
    };
    let b = BudgetStore {
        store: &second,
        budget,
    };
    assert!(pollster::block_on(a.get("first")).unwrap().is_some());
    assert!(matches!(
        pollster::block_on(b.get("second")),
        Err(muniment::StoreError::LimitExceeded { .. })
    ));
}
