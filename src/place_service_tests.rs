// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
#[test]
fn release_and_custody_retirement_before_use_do_not_start_places() {
    let (port, updates) = configure(std::sync::Arc::new(|| {}), PlaceWorkerSettings::default());
    let (ack, rx) = std::sync::mpsc::sync_channel(1);
    assert!(port.command(C::Release(ack)));
    rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    let (ack, rx) = std::sync::mpsc::sync_channel(1);
    assert!(port.command(C::RetireIdentity(ack)));
    rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert!(!port.has_started());
    assert!(updates.try_recv().is_err());
}
#[cfg(not(feature = "peers"))]
#[test]
fn absent_peers_refuse_the_exact_request_without_a_worker() {
    let (port, updates) = configure(std::sync::Arc::new(|| {}), PlaceWorkerSettings::default());
    let session = crate::panes::SessionId(uuid::Uuid::new_v4());
    assert!(port.command(C::Found {
        session,
        generation: 17,
        directory: std::path::PathBuf::from("/unused"),
        name: "test".into()
    }));
    assert!(
        matches!(updates.try_recv(),Ok(Update::PlaceFounded{session:s,generation:17,result:Err(_ )}) if s==session)
    );
    assert!(!port.has_started());
}
