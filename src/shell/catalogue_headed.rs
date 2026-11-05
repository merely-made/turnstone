// SPDX-License-Identifier: MPL-2.0
//! Explicit ignored native trial; its only identity is a public test constant.
use super::*;
use identity::IdentityProvider;
#[test]
#[ignore = "interactive native window; requires an isolated acceptance root"]
fn catalogue_native_window() {
    use winit::platform::{
        pump_events::{EventLoopExtPumpEvents, PumpStatus},
        windows::EventLoopBuilderExtWindows,
    };
    let root = std::path::PathBuf::from(
        std::env::var_os("TURNSTONE_ROOT").expect("isolated TURNSTONE_ROOT required"),
    );
    assert!(root.is_absolute());
    assert!(
        root.join("PUBLIC-TEST-KEYS-NOT-A-PERSONAL-WALLET")
            .is_file()
    );
    assert_eq!(std::env::var("TURNSTONE_OFFLINE").as_deref(), Ok("1"));
    let mut event_loop = winit::event_loop::EventLoop::<ShellEvent>::with_user_event()
        .with_any_thread(true)
        .build()
        .unwrap();
    let mut shell = Shell::new(
        event_loop.create_proxy(),
        Some("https://garden.test/".into()),
    );
    if let Some((setup, _)) = shell.custody_setup.take() {
        setup.request_stop();
        drop(setup);
    }
    let identity = Arc::new(crate::identity::RootIdentity::from_seed([71; 32]));
    shell.app.identity = Some(identity.clone());
    shell
        .place_handle
        .command(crate::place::worker::PlaceWorkerCommand::AdoptIdentity(
            identity,
        ));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(900);
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "headed acceptance deadline"
        );
        if let PumpStatus::Exit(_) =
            event_loop.pump_app_events(Some(std::time::Duration::from_millis(100)), &mut shell)
        {
            break;
        }
    }
    let session = shell.app.session_dir();
    let path = session.join("catalogue-hints.cbor");
    let directory = gemot::moot::workflow::discovery::CatalogueDirectory::decode(
        &std::fs::read(&path).expect("advertisement journal"),
        128,
    )
    .unwrap();
    let provider = identity::InMemoryProvider::from_seed([71; 32])
        .master_public_key()
        .to_bytes();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let outgoing = directory.outgoing(provider, now);
    assert!(
        outgoing
            .iter()
            .any(|h| h.advertisement.description.is_none()),
        "withdrawal must be exercised through the UI"
    );
    std::fs::write(root.join("headed-domain-result.json"),serde_json::to_vec_pretty(&serde_json::json!({"public_test_identity":true,"window_closed":true,"signed_withdrawal_retained":true,"journal":path})).unwrap()).unwrap();
}
