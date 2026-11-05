// SPDX-License-Identifier: MPL-2.0

use super::*;
use identity::InMemoryProvider;
#[test]
fn catalogue_owner_roundtrip_reopen_withdraw_and_expiry_use_real_place_records() {
    let dir = tempfile::tempdir().unwrap();
    let identity = InMemoryProvider::from_seed([67; 32]);
    let mut settings = PlaceWorkerSettings::default();
    settings.authority_clock = AuthorityClock::Fixed(100);
    let binding = found_place(dir.path(), &identity, "public garden", &settings).unwrap();
    let (open, _) = open_cached_place(dir.path(), &binding, &identity, &settings).unwrap();
    let draft = CatalogueSiteDraft {
        visibility: crate::place::catalogue::CatalogueVisibility::Public,
        address: "scroll://garden.test/".into(),
        title: "Garden".into(),
        abstract_text: "Painting and music".into(),
        scroll_subject: Some(7),
    };
    advertise(&open, &identity, &draft, 100).unwrap();
    let rows = refresh(&open, 101).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        open_address(&open, rows[0].digest, 102).unwrap(),
        draft.address
    );
    assert!(refresh(&open, rows[0].expires_at_ms).unwrap().is_empty());
    drop(open);
    let (open, _) = open_cached_place(dir.path(), &binding, &identity, &settings).unwrap();
    assert_eq!(refresh(&open, 103).unwrap(), rows);
    withdraw(&open, &identity, rows[0].collection, 104).unwrap();
    assert!(refresh(&open, 105).unwrap().is_empty());
    assert!(open_address(&open, rows[0].digest, 105).is_err());
}
#[test]
fn catalogue_refuses_standing_policy_without_amending_it() {
    let dir = tempfile::tempdir().unwrap();
    let identity = InMemoryProvider::from_seed([69; 32]);
    let settings = PlaceWorkerSettings {
        founding_admission: mien::Policy::OpenWithFloor(Default::default()),
        ..Default::default()
    };
    let binding = found_place(dir.path(), &identity, "garden", &settings).unwrap();
    let (open, _) = open_cached_place(dir.path(), &binding, &identity, &settings).unwrap();
    let before = pollster::block_on(open.moot.snapshot()).unwrap().governance;
    let draft = CatalogueSiteDraft {
        visibility: crate::place::catalogue::CatalogueVisibility::Public,
        address: "https://garden.test/".into(),
        title: "Garden".into(),
        abstract_text: String::new(),
        scroll_subject: Some(7),
    };
    assert!(
        advertise(&open, &identity, &draft, settings.authority_clock.now_ms())
            .unwrap_err()
            .contains("BelowThreshold")
    );
    let after = pollster::block_on(open.moot.snapshot()).unwrap().governance;
    assert_eq!(before.revision, after.revision);
    assert_eq!(before.rules, after.rules);
}
#[test]
fn malformed_catalogue_journal_refuses_before_changing_constitution() {
    let dir = tempfile::tempdir().unwrap();
    let identity = InMemoryProvider::from_seed([68; 32]);
    let settings = PlaceWorkerSettings::default();
    let binding = found_place(dir.path(), &identity, "garden", &settings).unwrap();
    let (open, _) = open_cached_place(dir.path(), &binding, &identity, &settings).unwrap();
    let before = pollster::block_on(open.moot.snapshot())
        .unwrap()
        .governance
        .revision;
    std::fs::write(dir.path().join("catalogue-hints.cbor"), b"broken").unwrap();
    let draft = CatalogueSiteDraft {
        visibility: crate::place::catalogue::CatalogueVisibility::Public,
        address: "scroll://garden.test/".into(),
        title: "Garden".into(),
        abstract_text: String::new(),
        scroll_subject: Some(4),
    };
    assert!(advertise(&open, &identity, &draft, 100).is_err());
    assert!(
        place_snapshot(&open, binding.moot.0, &settings)
            .unwrap()
            .catalogue_error
            .is_some()
    );
    assert_eq!(
        pollster::block_on(open.moot.snapshot())
            .unwrap()
            .governance
            .revision,
        before
    );
}

#[test]
fn protected_catalogue_encrypts_records_and_journal_then_reopens_and_withdraws() {
    let dir = tempfile::tempdir().unwrap();
    let identity = InMemoryProvider::from_seed([81; 32]);
    let settings = PlaceWorkerSettings {
        authority_clock: AuthorityClock::Fixed(100),
        ..Default::default()
    };
    let binding = found_place(dir.path(), &identity, "member room", &settings).unwrap();
    let (open, _) = open_cached_place(dir.path(), &binding, &identity, &settings).unwrap();
    let draft = CatalogueSiteDraft {
        visibility: CatalogueVisibility::Members,
        address: "scroll://private-catalogue.test/secret".into(),
        title: "Secret member index".into(),
        abstract_text: "Protected astronomy notes".into(),
        scroll_subject: Some(5),
    };
    advertise(&open, &identity, &draft, 100).unwrap();
    let row = refresh(&open, 101).unwrap().remove(0);
    assert_eq!(row.visibility, CatalogueVisibility::Members);
    assert!(!dir.path().join("catalogue-hints.cbor").exists());
    drop(open);
    let mut files = Vec::new();
    collect_files(dir.path(), &mut files);
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        for secret in [&draft.address, &draft.title, &draft.abstract_text] {
            assert!(
                !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
                "metadata leaked to {}",
                path.display()
            );
        }
    }
    let (open, _) = open_cached_place(dir.path(), &binding, &identity, &settings).unwrap();
    assert_eq!(refresh(&open, 102).unwrap()[0].digest, row.digest);
    advertise(&open, &identity, &draft, 103).unwrap();
    let revised = refresh(&open, 104).unwrap().remove(0);
    assert_eq!(revised.collection, row.collection);
    assert!(revised.sequence > row.sequence);
    withdraw(&open, &identity, row.collection, 105).unwrap();
    assert!(refresh(&open, 106).unwrap().is_empty());
    assert!(open_address(&open, revised.digest, 106).is_err());
    let path = dir.path().join("protected-catalogue-hints.cbor");
    let mut bytes = std::fs::read(&path).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    std::fs::write(path, bytes).unwrap();
    assert!(refresh(&open, 107).is_err());
    assert!(
        place_snapshot(&open, binding.moot.0, &settings)
            .unwrap()
            .catalogue_error
            .is_some()
    );
}
fn collect_files(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_files(&path, out)
        } else {
            out.push(path)
        }
    }
}
#[test]
fn protected_publication_missing_epoch_refuses_before_law_or_plaintext_writes() {
    let dir = tempfile::tempdir().unwrap();
    let identity = InMemoryProvider::from_seed([82; 32]);
    let settings = PlaceWorkerSettings::default();
    let binding = found_place(dir.path(), &identity, "member room", &settings).unwrap();
    let (open, _) = open_cached_place(dir.path(), &binding, &identity, &settings).unwrap();
    let (unjoined, _) =
        stickleback::GroupSession::new(stickleback::GroupSessionId(binding.moot.0), &identity)
            .unwrap();
    open.group
        .keys
        .replace_from_bytes(&unjoined.data_keyring_state().unwrap())
        .unwrap();
    let before = pollster::block_on(open.moot.snapshot())
        .unwrap()
        .governance
        .revision;
    let draft = CatalogueSiteDraft {
        visibility: CatalogueVisibility::Members,
        address: "https://private-catalogue.test/".into(),
        title: "Secret member index".into(),
        abstract_text: "Protected astronomy notes".into(),
        scroll_subject: Some(5),
    };
    assert!(advertise(&open, &identity, &draft, 100).is_err());
    assert_eq!(
        pollster::block_on(open.moot.snapshot())
            .unwrap()
            .governance
            .revision,
        before
    );
    assert!(!dir.path().join("catalogue-hints.cbor").exists());
    assert!(!dir.path().join("protected-catalogue-hints.cbor").exists());
}
