// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn sealed_workshop_refuses_a_schema_one_installation_without_replacing_its_bytes() {
    use athanor::workshop_driver::Driver;
    use muniment::{Backend, DirectoryBackend};
    use std::sync::Arc;

    pollster::block_on(async {
        let root = tempfile::tempdir().unwrap();
        let owner = owner("turnstone-public-example", Uuid::new_v4(), Uuid::new_v4());
        let store = DirectoryBackend::open(root.path()).unwrap();
        let driver = Driver::new_sealed(
            store.clone(),
            owner.clone(),
            [3; 16],
            Arc::new(|| {}),
            Arc::new(TestProtection(AtomicU64::new(1))),
        );
        driver.open([1; 32]).await.unwrap();
        let keys = store.list("alembic/workshop/sealed/v1/").await.unwrap();
        let [key] = &keys[..] else {
            panic!("expected one installed helper")
        };
        // An unused schema-1 installation has the same empty runs array and
        // binding as schema 2. The fixture protection passes its bytes through.
        let mut installation: serde_json::Value =
            serde_json::from_slice(&store.get(key).await.unwrap().unwrap()).unwrap();
        assert_eq!(installation["schema"], 2);
        assert_eq!(installation["runs"], serde_json::json!([]));
        installation["schema"] = 1.into();
        let legacy = serde_json::to_vec(&installation).unwrap();
        store.put(key, &legacy).await.unwrap();
        drop(driver);
        drop(store);
        let store = DirectoryBackend::open(root.path()).unwrap();
        let driver = Driver::new_sealed(
            store.clone(),
            owner,
            [3; 16],
            Arc::new(|| {}),
            Arc::new(TestProtection(AtomicU64::new(1))),
        );
        assert!(matches!(
            driver.open([1; 32]).await,
            Err(athanor::workshop::Error::Invalid(
                "installation schema or identity"
            ))
        ));
        assert_eq!(store.get(key).await.unwrap().unwrap(), legacy);
    });
}
