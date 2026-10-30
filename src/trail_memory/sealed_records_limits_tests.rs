// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;

#[test]
fn model_free_oversized_pending_envelope_refused_before_unseal() {
    let sealer = Sealer(AtomicBool::new(true));
    let counting = CountingSealer {
        inner: &sealer,
        unseals: Default::default(),
    };
    for kind in ["property", "result"] {
        let root = tempfile::tempdir().unwrap();
        let mut store = muniment::FjallStore::open(root.path()).unwrap();
        let nonce = uuid::Uuid::new_v4();
        let key = format!("turnstone/sealed-{kind}-pending/v2/{nonce}");
        pollster::block_on(store.put(&key, &vec![0; 8192])).unwrap();
        assert!(
            crate::trail_memory::sealed_record_receipts::inventory(
                &mut store,
                &counting,
                uuid::Uuid::new_v4(),
                1,
                4096,
            )
            .is_err()
        );
    }
    assert_eq!(counting.unseals.load(Ordering::SeqCst), 0);
}

#[test]
fn model_free_legacy_intents_are_preserved_and_refused() {
    for kind in ["property", "result"] {
        let root = tempfile::tempdir().unwrap();
        let mut store = muniment::FjallStore::open(root.path()).unwrap();
        let sealer = Sealer(AtomicBool::new(true));
        let key = format!(
            "turnstone/sealed-{kind}-request/v1/{}",
            uuid::Uuid::new_v4()
        );
        pollster::block_on(store.put(&key, b"legacy receipt")).unwrap();
        let error = crate::trail_memory::sealed_record_receipts::inventory(
            &mut store,
            &sealer,
            uuid::Uuid::new_v4(),
            1,
            4096,
        )
        .unwrap_err();
        assert!(error.contains("Unsupported prior"));
        assert_eq!(
            pollster::block_on(store.get(&key)).unwrap(),
            Some(b"legacy receipt".to_vec())
        );
    }
}
