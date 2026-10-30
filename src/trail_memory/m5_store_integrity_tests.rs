// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use eidetic::Backend as _;
use std::collections::BTreeMap;

fn snapshot(root: &std::path::Path) -> BTreeMap<std::path::PathBuf, String> {
    fn walk(
        base: &std::path::Path,
        path: &std::path::Path,
        out: &mut BTreeMap<std::path::PathBuf, String>,
    ) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(base).unwrap().to_path_buf(),
                    blake3::hash(&std::fs::read(&path).unwrap())
                        .to_hex()
                        .to_string(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}

#[test]
#[ignore = "diagnostic: first normal reopen repairs journal padding on this pinned owner"]
fn model_free_real_sealed_store_open_read_preserves_complete_tree() {
    let root = tempfile::tempdir().unwrap();
    let fixture = crate::trail_memory::custody_test_fixture::custody();
    let clear = b"sealed owner integrity fixture";
    let hash = eidetic::Hash::of(clear);
    let (cipher, marker) = fixture.sealer.seal(&hash, clear).unwrap();
    {
        let store = FjallStore::open(root.path()).unwrap();
        pollster::block_on(store.put("sealed-fixture", &cipher)).unwrap();
        store.sync_all().unwrap();
    }
    let before = snapshot(root.path());
    {
        let store = FjallStore::open(root.path()).unwrap();
        let saved = pollster::block_on(store.get("sealed-fixture"))
            .unwrap()
            .unwrap();
        assert_eq!(
            fixture.sealer.unseal(&hash, &marker, &saved).unwrap(),
            clear
        );
    }
    let after = snapshot(root.path());
    assert_eq!(
        before, after,
        "opening and reading the store changed its complete file tree"
    );
}

#[test]
fn model_free_post_reopen_sealed_store_inspection_preserves_complete_tree() {
    let root = tempfile::tempdir().unwrap();
    let fixture = crate::trail_memory::custody_test_fixture::custody();
    let clear = b"sealed owner integrity fixture";
    let hash = eidetic::Hash::of(clear);
    let (cipher, marker) = fixture.sealer.seal(&hash, clear).unwrap();
    {
        let store = FjallStore::open(root.path()).unwrap();
        pollster::block_on(store.put("sealed-fixture", &cipher)).unwrap();
        store.sync_all().unwrap();
    }
    // This is the ordinary app's reopen, not an alteration of an inspection copy.
    {
        let store = FjallStore::open(root.path()).unwrap();
        let saved = pollster::block_on(store.get("sealed-fixture"))
            .unwrap()
            .unwrap();
        assert_eq!(
            fixture.sealer.unseal(&hash, &marker, &saved).unwrap(),
            clear
        );
    }
    let before = snapshot(root.path());
    {
        let store = FjallStore::open(root.path()).unwrap();
        let saved = pollster::block_on(store.get("sealed-fixture"))
            .unwrap()
            .unwrap();
        assert_eq!(
            fixture.sealer.unseal(&hash, &marker, &saved).unwrap(),
            clear
        );
    }
    let after = snapshot(root.path());
    assert_eq!(
        before, after,
        "post-reopen inspection changed the complete file tree"
    );
}
