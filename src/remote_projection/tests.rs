// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use mere::kernel::graph::{CoverageLayer, CoverageLimit, CoverageNote};

fn scoped_coverage() -> CoverageNote {
    let layers = [
        CoverageLayer::Possession,
        CoverageLayer::Residency,
        CoverageLayer::Disclosure,
        CoverageLayer::Synchronization,
        CoverageLayer::Projection,
    ];
    CoverageNote {
        limits: layers
            .into_iter()
            .enumerate()
            .map(|(index, layer)| CoverageLimit {
                layer,
                reason: format!("known limit {index}"),
                resources: vec![uuid::Uuid::from_u128(100 + index as u128)],
                surfaces: vec![uuid::Uuid::from_u128(200 + index as u128)],
                count: if index % 2 == 0 { None } else { Some(index) },
            })
            .collect(),
    }
}

fn set_coverage(app: &mut App, note: CoverageNote) {
    let mut context = app.graph_runtimes.graph().clone();
    context.set_known_coverage(note);
    app.graph_runtimes
        .active_canvas_mut()
        .refresh_semantic_context(&context);
}

#[test]
fn disclosed_scene_retains_every_scoped_coverage_layer() {
    let mut app = App::projection_fixture();
    let known = scoped_coverage();
    set_coverage(&mut app, known.clone());
    let disclosed = disclose_scene(
        app.graph_runtimes.graph(),
        app.graph_runtimes.focused_key(),
        (248.0, 168.0),
        Spiral::default(),
    );
    for limit in &known.limits {
        assert!(disclosed.coverage.limits.contains(limit), "lost {limit:?}");
    }
    assert_eq!(disclosed.scene.items.len(), 3);
    assert_eq!(disclosed.scene.relations.len(), 2);
}

#[test]
fn coverage_survives_disclosure_and_snapshot_refusal() {
    let mut app = App::projection_fixture();
    let known = scoped_coverage();
    set_coverage(&mut app, known.clone());
    let mut endpoint = TurnstoneEndpoint::new(app).unwrap();
    assert!(endpoint.snapshot_coverage().is_none());
    let request = ProjectionRequest {
        version: ProtocolVersion::V1,
        session: endpoint.session().clone(),
        score: Score::new(Arrangement::Spiral(Spiral::default())),
    };
    let first = endpoint.snapshot(request.clone()).unwrap();
    for limit in &known.limits {
        assert!(endpoint.snapshot_coverage().unwrap().limits.contains(limit));
    }
    assert_eq!(first.scene.active_item_count(), 3);
    assert_eq!(first.presentation.bindings.len(), 3);

    let truth_revision = endpoint.app.graph_runtimes.graph().revision();
    let mut changed = known.clone();
    changed.limits[0].count = Some(7);
    changed.limits[1].resources = vec![uuid::Uuid::from_u128(999)];
    set_coverage(&mut endpoint.app, changed.clone());
    assert_eq!(
        endpoint.app.graph_runtimes.graph().revision(),
        truth_revision
    );
    let second = endpoint.snapshot(request.clone()).unwrap();
    assert_eq!(second.scene.epoch, first.scene.epoch);
    assert_eq!(second.scene.revision, first.scene.revision);
    for limit in &changed.limits {
        assert!(endpoint.snapshot_coverage().unwrap().limits.contains(limit));
    }
    assert!(
        !endpoint
            .snapshot_coverage()
            .unwrap()
            .limits
            .contains(&known.limits[0])
    );
    assert!(
        !endpoint
            .snapshot_coverage()
            .unwrap()
            .limits
            .contains(&known.limits[1])
    );

    let prior_coverage = endpoint.snapshot_coverage().unwrap().clone();
    let prior_resources = endpoint.resources.clone();
    let mut legacy = request;
    legacy.score.version = 4;
    assert!(matches!(
        endpoint.snapshot(legacy),
        Err(ProjectionRefusal::UnsupportedScore {
            received: 4,
            supported: 5
        })
    ));
    assert_eq!(endpoint.snapshot_coverage(), Some(&prior_coverage));
    assert_eq!(endpoint.snapshot.as_ref(), Some(&second));
    assert_eq!(endpoint.resources, prior_resources);

    // V1 carries scene/presentation only. Local retention is not a claim
    // that remote clients received these known limits.
    let wire = serde_json::to_value(&second).unwrap();
    assert!(wire.get("coverage").is_none());
}

#[test]
fn score_five_accepts_and_score_four_refuses_without_replacing_snapshot() {
    assert_eq!(
        sceno::SCORE_VERSION,
        5,
        "this repin qualifies the version-five family"
    );
    let mut endpoint = TurnstoneEndpoint::new(App::projection_fixture()).unwrap();
    let request = ProjectionRequest {
        version: ProtocolVersion::V1,
        session: endpoint.session().clone(),
        score: Score::new(Arrangement::Spiral(Spiral::default())),
    };
    endpoint.snapshot(request.clone()).unwrap();
    let prior_revision = endpoint.snapshot.as_ref().unwrap().scene.revision;
    let prior_resources = endpoint.resources.clone();
    let mut legacy = request;
    legacy.score.version = 4;
    assert!(matches!(
        endpoint.snapshot(legacy),
        Err(ProjectionRefusal::UnsupportedScore {
            received: 4,
            supported: 5
        })
    ));
    assert_eq!(
        endpoint.snapshot.as_ref().unwrap().scene.revision,
        prior_revision
    );
    assert_eq!(endpoint.resources, prior_resources);
}

/// The endpoint's authority is DELEGATED by the user, not self-issued.
/// Before the capability-model audit its subject was `blake3(session
/// name)` — not a key — and it granted itself the layout capability, so a
/// refusal only said "it did not grant itself". Now the refusal is an
/// authority statement: the capability descends from the profile identity
/// and is worthless to anyone else.
#[test]
fn the_endpoint_holds_a_delegation_from_the_profile_identity() {
    use servitor::AuthorityProvider;

    let app = App::projection_fixture();
    let user = app
        .personae_root()
        .expect("the fixture's vault is not locked");
    let endpoint = TurnstoneEndpoint::new(app).unwrap();
    let layout = Cap::Scope(layout_scope());

    assert_eq!(
        endpoint.authority.root(),
        user,
        "the chain roots at the user, not at the endpoint itself"
    );
    assert_ne!(
        endpoint.subject.0,
        *blake3::hash(SESSION.as_bytes()).as_bytes(),
        "the subject is a derived KEY, not a hash of its own name"
    );
    assert!(
        endpoint
            .authority
            .covers(endpoint.subject, &layout, Mode::Write),
        "the delegated layout capability covers"
    );
    assert!(
        !endpoint
            .authority
            .covers(endpoint.subject, &Cap::Scope(graph_scope()), Mode::Write),
        "and graph truth was never delegated"
    );

    // The load-bearing half: the SAME certificates under a different root
    // authorize nothing. A self-issued grant could not fail this way.
    let mut foreign = DelegationTable::new([9u8; 32]);
    for cert in endpoint.authority.certificates() {
        foreign.adopt(cert.clone());
    }
    foreign.set_now(endpoint.authority.now());
    assert!(
        !foreign.covers(endpoint.subject, &layout, Mode::Write),
        "the endpoint's authority is anchored to THIS user's identity"
    );
}

#[test]
fn live_mere_graph_becomes_cards_and_routed_relations() {
    let mut endpoint = TurnstoneEndpoint::new(App::projection_fixture()).unwrap();
    let snapshot = endpoint
        .snapshot(ProjectionRequest {
            version: ProtocolVersion::V1,
            session: endpoint.session().clone(),
            score: Score::new(Arrangement::Spiral(Spiral::default())),
        })
        .unwrap();
    assert_eq!(snapshot.scene.active_item_count(), 3);
    assert_eq!(snapshot.scene.tables.relations.iter().flatten().count(), 2);
    assert!(
        snapshot
            .scene
            .tables
            .sources
            .iter()
            .flatten()
            .all(|source| source.adapter == cartography::MERE_GRAPH_ADAPTER)
    );
    assert_eq!(snapshot.presentation.bindings.len(), 3);
}

#[test]
fn projected_grant_accepts_view_and_refuses_graph_change() {
    let run = run_g3_canary().unwrap();
    assert_eq!(run.fit_result, IntentResult::Accepted);
    assert!(matches!(run.open_result, IntentResult::Rejected { .. }));
    assert_eq!(run.graph_nodes_before, run.graph_nodes_after);
    assert_eq!(run.graph_revision_before, run.graph_revision_after);
    assert_eq!(run.audit_revision, 2, "grant plus accepted petition");
    assert!(run.audit_author.starts_with("denizen:"));
    assert_eq!(run.layout.placements.len(), 3);
    assert_eq!(run.layout.relations.len(), 2);
}

#[test]
fn committed_receipt_matches_the_live_turnstone_endpoint() {
    let committed =
        include_str!("../../docs/receipts/g3_turnstone_endpoint.html").replace("\r\n", "\n");
    assert_eq!(render_g3_receipt().unwrap(), committed);
}
