// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use eidetic::{ManifestId, TypedPayload};
use fleece::eidetic_bridge::{CaptureIdentity, FLEECE_ANNOTATION_SCHEMA_ID};
use fleece::{TextPositionSelector, anchor_for_range, extract_document};
use genet_static_dom::StaticDocument;
use muniment::FjallStore;
use servitor::{Cap, Mode, Subject};

struct Allowed;

fn library_with_test_custody() -> LocalCaptureLibrary {
    let library = LocalCaptureLibrary::default();
    let custody = crate::trail_memory::custody_test_fixture::custody();
    assert!(library.bind_custody(&custody));
    library
}

fn install_ready_for_test(
    library: &LocalCaptureLibrary,
    directory: &Path,
    ready: ReadyCaptureLibrary,
) {
    let ticket = library.ticket().unwrap();
    assert!(library.replace_if_current(ticket, directory, ready));
}

#[test]
fn custody_retirement_rejects_a_queued_captured_library_refill() {
    let library = library_with_test_custody();
    let refresh_ticket = library.ticket().unwrap();

    library.retire_custody();

    assert!(
        !library.replace_if_current(
            refresh_ticket,
            "/synthetic/session",
            ReadyCaptureLibrary::default(),
        ),
        "a refresh started before lock cannot repopulate the shared cache"
    );
    assert!(matches!(
        library.snapshot_for(Path::new("/synthetic/session"), refresh_ticket),
        CaptureLibraryState::Unavailable(_)
    ));
}

#[test]
fn retired_source_custody_cannot_be_reenabled_by_stale_adoption() {
    let library = library_with_test_custody();
    let source_epoch = Arc::new(AtomicU64::new(7));
    let callback_epoch = source_epoch.clone();
    let mut custody = crate::trail_memory::custody_test_fixture::custody();
    custody.epoch = Arc::new(move || match callback_epoch.load(Ordering::SeqCst) {
        0 => None,
        epoch => Some(epoch),
    });

    // The shell's initial SetCustody snapshot succeeds, then lock occurs
    // before the actor installs its library admission.
    assert_eq!((custody.epoch)(), Some(7));
    source_epoch.store(0, Ordering::SeqCst);
    library.retire_custody();
    assert!(!library.bind_custody(&custody));
    assert!(library.ticket().is_err());
    assert!(matches!(
        library.snapshot_for(
            Path::new("/synthetic/session"),
            CustodyTicket {
                generation: 1,
                source_epoch: 7,
            },
        ),
        CaptureLibraryState::Unavailable(_)
    ));
}

#[test]
fn debug_output_never_includes_capture_library_contents_or_status_text() {
    let library = LocalCaptureLibrary::default();
    library.unavailable("PRIVATE_CANARY_CAPTURE_TEXT");

    let debug = format!("{library:?}");

    assert!(!debug.contains("PRIVATE_CANARY_CAPTURE_TEXT"));
    assert!(debug.contains("available: false"));
}

impl AuthorityProvider for Allowed {
    fn covers(&self, _: Subject, _: &Cap, mode: Mode) -> bool {
        mode == Mode::Write
    }
}

fn record(source: &str, html: &str, raw_tag: u8) -> FleeceAnnotationRecord {
    let document = extract_document(&StaticDocument::parse(html));
    let end = document.page.text.chars().count() as u64;
    let anchor = anchor_for_range(
        &document.page.text,
        TextPositionSelector { start: 0, end },
        document.contract.quote_context,
    )
    .unwrap();
    FleeceAnnotationRecord::from_fleece(
        CaptureIdentity::new(source, eidetic::Hash::of(&[raw_tag])).unwrap(),
        &document,
        &anchor,
    )
    .unwrap()
}

fn share(manifest: ManifestId, operation: u8, at_ms: u64) -> gemot::moot::FaunaEntry {
    gemot::moot::FaunaEntry {
        manifest_id: *manifest.0.as_bytes(),
        schema_id: FLEECE_ANNOTATION_SCHEMA_ID.to_string(),
        title: format!("capture {operation}"),
        shared_by: [7; 32],
        at_ms,
        op_hash: [operation; 32],
    }
}

fn empty_roster(fauna: Vec<gemot::moot::FaunaEntry>) -> gemot::moot::MootRoster {
    gemot::moot::MootRoster {
        fauna,
        ..Default::default()
    }
}

fn save(store: &mut FjallStore, record: &FleeceAnnotationRecord, at_ms: u64) -> ManifestId {
    pollster::block_on(fleece::bootstrap_fleece_annotation_schema(store)).unwrap();
    pollster::block_on(fleece::save_fleece_annotation(store, record, at_ms)).unwrap()
}

#[test]
fn local_records_group_search_and_survive_cold_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let html = "<main><p>Mycelial mutual-aid field notes.</p></main>";
    let first = record("https://one.test/notes", html, 1);
    let second = record("https://two.test/copy", html, 1);
    let (first_id, second_id) = {
        let mut store = FjallStore::open(dir.path()).unwrap();
        (save(&mut store, &first, 10), save(&mut store, &second, 20))
    };
    let roster = empty_roster(vec![share(first_id, 1, 10), share(second_id, 2, 20)]);

    let library = library_with_test_custody();
    {
        let mut reopened = FjallStore::open(dir.path()).unwrap();
        refresh_local_capture_library(&mut reopened, dir.path(), &library).unwrap();
    }
    let collection = remint(&roster, &Allowed, dir.path(), &library);
    assert_eq!(collection.groups.len(), 1);
    assert_eq!(collection.groups[0].contributions.len(), 2);
    assert_eq!(collection.search("mycelial", 5).len(), 1);
    assert_eq!(collection.search("mycelial", 5)[0].contribution_count, 2);
    assert!(collection.rejected.is_empty());
}

#[test]
fn targeted_withdrawal_remints_only_the_selected_contribution_away() {
    let html = "<main><p>Withdrawal keeps a surviving contribution.</p></main>";
    let first = record("https://one.test", html, 1);
    let second = record("https://two.test", html, 1);
    let first_id = ManifestId::of_blob(&first.serialize_to_bytes().unwrap());
    let second_id = ManifestId::of_blob(&second.serialize_to_bytes().unwrap());
    let kept = share(first_id, 1, 10);
    let removed = share(second_id, 2, 20);
    let mut roster = empty_roster(vec![kept.clone(), removed.clone()]);
    roster.withdrawals.push(gemot::moot::FaunaWithdrawal {
        target_share: removed.op_hash,
        withdrawn_by: removed.shared_by,
        at_ms: 30,
        op_hash: [3; 32],
    });
    let library = library_with_test_custody();
    let scope = Path::new("test-capture-library");
    install_ready_for_test(
        &library,
        scope,
        ReadyCaptureLibrary {
            records: [(kept.manifest_id, first), (removed.manifest_id, second)]
                .into_iter()
                .collect(),
            invalid: BTreeMap::new(),
        },
    );

    let collection = remint(&roster, &Allowed, scope, &library);
    assert_eq!(collection.groups.len(), 1);
    assert_eq!(collection.groups[0].contributions.len(), 1);
    assert_eq!(
        collection.groups[0].contributions[0].share_operation,
        kept.op_hash
    );
}

#[test]
fn missing_and_unavailable_manifest_statuses_never_become_text() {
    let missing = share(ManifestId::of_blob(b"missing"), 4, 40);
    let invalid = share(ManifestId::of_blob(b"invalid"), 5, 50);
    let roster = empty_roster(vec![missing.clone(), invalid.clone()]);
    let ready = library_with_test_custody();
    let scope = Path::new("test-capture-library");
    install_ready_for_test(
        &ready,
        scope,
        ReadyCaptureLibrary {
            records: BTreeMap::new(),
            invalid: [(invalid.manifest_id, "payload hash mismatch".to_string())]
                .into_iter()
                .collect(),
        },
    );
    let missing_view = remint(&roster, &Allowed, scope, &ready);
    assert!(missing_view.groups.is_empty());
    assert!(missing_view.rejected.iter().any(|item| {
        item.share_operation == missing.op_hash
            && item.reason == CapturedRejectionReason::MissingRecord
    }));
    assert!(missing_view.rejected.iter().any(|item| {
        item.share_operation == invalid.op_hash
            && matches!(item.reason, CapturedRejectionReason::InvalidRecord(_))
    }));
    assert!(missing_view.search(&missing.title, 5).is_empty());

    let wrong_session = remint(
        &roster,
        &Allowed,
        Path::new("another-session/memory"),
        &ready,
    );
    assert!(wrong_session.groups.is_empty());
    assert!(matches!(
        wrong_session.rejected[0].reason,
        CapturedRejectionReason::LibraryUnavailable(_)
    ));

    let unavailable = library_with_test_custody();
    unavailable.unavailable("capture storage is offline");
    let unavailable_view = remint(&roster, &Allowed, scope, &unavailable);
    assert!(unavailable_view.groups.is_empty());
    assert!(matches!(
        &unavailable_view.rejected[0].reason,
        CapturedRejectionReason::LibraryUnavailable(error)
            if error == "capture storage is offline"
    ));
    assert!(unavailable_view.search(&missing.title, 5).is_empty());
}
