// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::inference_service::RetainedSourceOwner;
#[test]
fn the_actor_lends_versions_without_reopening_its_store_or_substituting_a_new_head() {
    let temp = tempfile::tempdir().unwrap();
    let session = uuid::Uuid::new_v4();
    let dir = temp
        .path()
        .join("sessions")
        .join(session.to_string())
        .join("memory");
    std::fs::create_dir_all(&dir).unwrap();
    pollster::block_on(PageTextStore::new(&FjallStore::open(&dir).unwrap()).put(
        "https://retained.test/note",
        "nebula amberstone",
        1,
    ))
    .unwrap();
    let (handle, _updates) = spawn_trail_for_session(
        std::sync::Arc::new(|| {}),
        Some(dir),
        Default::default(),
        uuid::Uuid::nil(),
    );
    assert!(handle.command(TrailCommand::SetCustody(Some(
        custody_test_fixture::custody(),
    ))));
    let owner = TrailInferenceOwner(std::sync::Arc::new(handle));
    let namespace = format!("turnstone/{session}");
    let old = owner.snapshot(&namespace).unwrap().remove(0);
    assert_eq!(old.text, "nebula amberstone");
    owner.0.command(TrailCommand::RecordText {
        url: "https://retained.test/note".into(),
        text: "nebula indigo".into(),
    });
    let still_legacy = owner.snapshot(&namespace).unwrap().remove(0);
    assert_eq!(old.version, still_legacy.version);
    assert_eq!(still_legacy.text, "nebula amberstone");
    assert_eq!(
        owner.resolve(&old.source, &old.version).unwrap(),
        "nebula amberstone"
    );
    assert!(owner.snapshot("turnstone/another-session").is_err());
    let (ack, receive) = std::sync::mpsc::sync_channel(1);
    owner.0.command(TrailCommand::Release(ack));
    receive.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(owner.snapshot(&namespace).is_err());
    match std::sync::Arc::try_unwrap(owner.0) {
        Ok(actor) => actor.join().unwrap(),
        Err(_) => panic!("unexpected actor borrower"),
    }
}
