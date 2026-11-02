// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::action::{Action, Update};

#[test]
fn staged_installs_are_content_derived_and_reviewable() {
    let dir = std::env::temp_dir().join(format!("turnstone-denizen-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("trail-keeper.lua");
    std::fs::write(&path, "mere.open('mere://kept')").unwrap();

    let a = stage_install(&path).unwrap();
    let b = stage_install(&path).unwrap();
    assert_eq!(a.subject, b.subject, "same source, same subject");
    assert_eq!(a.label, "trail-keeper");
    let review = review_line(&a);
    assert!(review.contains("grants:"), "the ask is visible: {review}");
    for ring in default_rings() {
        assert!(
            review.contains(ring.name()),
            "the ask names {}: {review}",
            ring.name()
        );
    }
    assert!(
        review.contains("own world"),
        "the ask names its scope: {review}"
    );
    assert!(
        review.ends_with("Confirm"),
        "the ask names its decision: {review}"
    );
    // Review rows may wrap; exercise the actual Confirm projection.
    let mut app = App::test_stub();
    app.viewport = (1024.0, 320.0);
    app.pending_install = Some(a.clone());
    app.update(Action::OmnibarOpen { command: true });
    crate::chrome_view::tests::assert_install_review_layout(&app, &review);

    std::fs::write(&path, "mere.open('mere://other')").unwrap();
    let c = stage_install(&path).unwrap();
    assert_ne!(
        a.subject, c.subject,
        "a modified script is a different subject"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_deadband_is_strictly_parsed_and_named_in_review() {
    let source = "-- @deadband 5 1000\nmere.output(10)";
    let band = parse_deadband(source).unwrap().unwrap();
    assert_eq!(band.minimum_change(), 5);
    assert_eq!(band.minimum_interval_ms(), 1_000);
    for malformed in [
        "-- @deadband 0 1000",
        "-- @deadband 5 0",
        "-- @deadband five 1000",
        "-- @deadband 5",
        "-- @deadband 5 1000 extra",
    ] {
        assert!(
            parse_deadband(malformed).is_err(),
            "malformed declarations fail closed: {malformed}"
        );
    }

    let pending = PendingInstall {
        path: PathBuf::from("controller.lua"),
        label: "controller".into(),
        body: PackBody::Scenario(source.into()),
        subject: Subject::new([3; 32]),
        rings: default_rings(),
        watch_url: None,
        deadband: Some(band),
    };
    let review = review_line(&pending);
    assert!(review.contains("change >= 5"), "{review}");
    assert!(review.contains("interval >= 1000 ms"), "{review}");
}

#[test]
fn a_legacy_binding_rebuilds_and_asks_for_a_heal() {
    // A binding written before the containment ruling names the world in
    // the facet. The resident still rebuilds (no one is orphaned by an
    // upgrade), and the member is listed for the adopt-path heal that
    // moves the pointer onto `Node.nested`.
    let dir = std::env::temp_dir().join(format!("turnstone-legacy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let member = Uuid::from_u128(0xa);
    let mut store = pandect::NodeFacetStore::new();
    store
        .set(
            member,
            chartulary::FacetId::new(pandect::DENIZEN_BINDING),
            serde_json::json!({
                "subject": "aa".repeat(32),
                "nested_log": "aa".repeat(32),
                "kind": "scenario",
            }),
            &chartulary::AcceptAll,
        )
        .unwrap();

    let graph = mere::kernel::graph::Graph::new();
    let provider = identity::InMemoryProvider::from_seed([5u8; 32]);
    let denizens = rebuild(
        &store,
        &graph,
        &dir,
        Some(identity::IdentityProvider::master_public_key(&provider).to_bytes()),
    );
    assert_eq!(denizens.residents.len(), 1, "the legacy resident survives");
    assert_eq!(
        denizens.legacy_heals,
        vec![(member, "aa".repeat(32))],
        "and is queued for the one-time heal"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nested_logs_round_trip_through_disk() {
    let dir = std::env::temp_dir().join(format!("turnstone-nested-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let gate = Gate::new();
    let subject = Subject::new([7u8; 32]);
    let mut nested = GraphLog::with_id(LogId::new("aa".repeat(32)));
    gate.project_grant(&mut nested, &Grant::new(subject, world_cap(), Mode::Write))
        .unwrap();

    save_nested(&dir, &"aa".repeat(32), &nested);
    let restored = load_nested(&dir, &"aa".repeat(32)).expect("log restored");
    assert_eq!(restored.revision(), nested.revision());
    assert!(
        restored
            .graph()
            .key_of(&Gate::projection_id(&world_cap()))
            .is_some(),
        "the grant projection survived the round trip"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
