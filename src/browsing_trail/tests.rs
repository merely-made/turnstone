// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use kernel::geometry::PortablePoint;
use kernel::graph::apply::{
    GraphDelta, add_node, apply_graph_delta, assert_relation, node_history_back,
    node_history_forward,
};
use kernel::graph::{
    ArrangementSubKind, EdgeAssertion, Graph, NavigationTrigger, NodeHistoryTransitionKind,
};

use super::*;

const SOURCE: &str = "https://trail.example/index";
const VISITED: &str = "https://trail.example/shared";
const FOLLOWUP: &str = "https://trail.example/next";
const SOURCE_ID: Uuid = Uuid::from_u128(11);
const FIRST_ID: Uuid = Uuid::from_u128(12);
const SECOND_ID: Uuid = Uuid::from_u128(13);
const IMPORT_FIXTURE: &str = "TURNSTONE_TEST_RDF_TRAIL_REOPEN";

fn fixture() -> Graph {
    let mut graph = Graph::new();
    let source = add_node(
        &mut graph,
        Some(SOURCE_ID),
        SOURCE.into(),
        PortablePoint::new(0.0, 0.0),
    );
    apply_graph_delta(
        &mut graph,
        GraphDelta::ReplayNavigateNodeById {
            node_id: SOURCE_ID,
            url: SOURCE.into(),
            transition: NodeHistoryTransitionKind::UrlTyped,
            timestamp_ms: 1_700_000_000_100,
            last_session_visited: 0,
        },
    );
    for (id, timestamp_ms) in [
        (FIRST_ID, 1_700_000_000_123),
        (SECOND_ID, 1_700_000_000_456),
    ] {
        let child = add_node(
            &mut graph,
            Some(id),
            VISITED.into(),
            PortablePoint::new(1.0, 1.0),
        );
        apply_graph_delta(
            &mut graph,
            GraphDelta::BranchHistory {
                child,
                parent: source,
            },
        );
        apply_graph_delta(
            &mut graph,
            GraphDelta::ReplayNavigateNodeById {
                node_id: id,
                url: VISITED.into(),
                transition: NodeHistoryTransitionKind::LinkClick,
                timestamp_ms,
                last_session_visited: 0,
            },
        );
        apply_graph_delta(
            &mut graph,
            GraphDelta::AppendTraversal {
                from: source,
                to: child,
                trigger: NavigationTrigger::LinkClick,
                timestamp_ms: Some(timestamp_ms),
            },
        );
        assert_relation(
            &mut graph,
            source,
            child,
            EdgeAssertion::Arrangement {
                sub_kind: ArrangementSubKind::FrameMember,
            },
        )
        .unwrap();
    }
    let second = graph.get_node_key_by_id(SECOND_ID).unwrap();
    // A second observation on the same endpoints and millisecond is a distinct
    // event too. An RDF set must not collapse identical payloads.
    apply_graph_delta(
        &mut graph,
        GraphDelta::AppendTraversal {
            from: source,
            to: second,
            trigger: NavigationTrigger::LinkClick,
            timestamp_ms: Some(1_700_000_000_456),
        },
    );
    apply_graph_delta(
        &mut graph,
        GraphDelta::ReplayNavigateNodeById {
            node_id: FIRST_ID,
            url: FOLLOWUP.into(),
            transition: NodeHistoryTransitionKind::LinkClick,
            timestamp_ms: 1_700_000_000_789,
            last_session_visited: 0,
        },
    );
    graph
}

fn clicks(graph: &Graph) -> Vec<(Uuid, Uuid, u64, NavigationTrigger)> {
    let mut events = Vec::new();
    for (_, from, to, payload) in graph.surface_relation_buckets() {
        for event in payload.traversal.iter().flat_map(|t| &t.traversals) {
            events.push((
                graph.get_node(from).unwrap().id,
                graph.get_node(to).unwrap().id,
                event.timestamp_ms,
                event.trigger,
            ));
        }
    }
    events.sort_by_key(|(from, to, at, _)| (*from, *to, *at));
    events
}

#[test]
fn graphshell_rdf_browsing_trail_reopens_in_a_fresh_turnstone_store() {
    if let Some(path) = std::env::var_os(IMPORT_FIXTURE) {
        assert!(crate::offline::enabled());
        let root = Path::new(&path);
        command([
            "reopen".into(),
            root.join("trail.trig").into_os_string(),
            root.join("reopened").into_os_string(),
        ])
        .unwrap();
        let (app, _) = crate::app::App::boot(None);
        assert!(!app.session_load_refused());
        let graph = app.graph_runtimes.graph();
        assert_eq!(clicks(graph).len(), 3);
        for surface in [FIRST_ID, SECOND_ID] {
            let key = graph.get_node_key_by_id(surface).unwrap();
            assert_eq!(
                graph
                    .node_history_projection(key)
                    .entries
                    .first()
                    .map(String::as_str),
                Some(VISITED)
            );
        }
        return;
    }

    let profile = tempfile::tempdir().unwrap();
    let exchange = tempfile::tempdir().unwrap();
    let id = SessionId::new();
    let graph = fixture();
    let generic_rdf = mere::formats::serialize::to_trig(&graph).unwrap();
    let contribution = mere::formats::serialize::from_trig(&generic_rdf, "trail-baseline").unwrap();
    let mut generic_reopen = Graph::new();
    kernel::rdf::apply_contribution(&mut generic_reopen, &contribution);
    assert_eq!(generic_reopen.to_snapshot_at(0).navigation.visit_count(), 0);
    assert!(clicks(&generic_reopen).is_empty());
    println!("Generic TriG control reopened without visits or traversal events.");
    let session = session_dir(profile.path(), id);
    crate::session_graph_file::save(
        &session.join(pandect::session_graph_store::GRAPH_FILE),
        &graph,
    )
    .unwrap();
    let mut manifests = ManifestStore::with_root(sessions_root(profile.path()));
    manifests.insert(GraphSessionManifest::new(id, GraphId::new()));
    manifests.flush_dirty().unwrap();
    let trail = exchange.path().join("trail.trig");
    command([
        "export".into(),
        profile.path().as_os_str().into(),
        id.as_uuid().to_string().into(),
        trail.as_os_str().into(),
    ])
    .unwrap();
    drop(profile);
    let rdf = fs::read_to_string(&trail).unwrap();

    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            std::thread::current().name().unwrap(),
            "--nocapture",
        ])
        .env("TURNSTONE_OFFLINE", "1")
        .env("TURNSTONE_ROOT", exchange.path().join("reopened"))
        .env("MERE_ROOT", exchange.path().join("family"))
        .env(IMPORT_FIXTURE, exchange.path())
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );

    println!("Fresh Turnstone process booted and adopted the imported trail.");
    let fresh = exchange.path().join("reopened");
    let manifests = load_manifests(&fresh);
    assert_eq!(manifests.len(), 1);
    let (reopened, manifest) = manifests.iter().next().unwrap();
    assert_ne!(reopened, id);
    assert_eq!(manifest.extension_get::<Uuid>(ORIGIN), Some(*id.as_uuid()));
    let mut restored = crate::session::load_session_graph(&session_dir(&fresh, reopened)).unwrap();

    assert_eq!(
        clicks(&restored),
        [
            (
                SOURCE_ID,
                FIRST_ID,
                1_700_000_000_123,
                NavigationTrigger::LinkClick
            ),
            (
                SOURCE_ID,
                SECOND_ID,
                1_700_000_000_456,
                NavigationTrigger::LinkClick
            ),
            (
                SOURCE_ID,
                SECOND_ID,
                1_700_000_000_456,
                NavigationTrigger::LinkClick
            ),
        ]
    );
    assert_eq!(
        restored.to_snapshot_at(0).navigation,
        graph.to_snapshot_at(0).navigation
    );
    let first = restored.get_node_key_by_id(FIRST_ID).unwrap();
    let second = restored.get_node_key_by_id(SECOND_ID).unwrap();
    let source = restored.get_node_key_by_id(SOURCE_ID).unwrap();
    for target in [first, second] {
        assert!(restored.arrangement_edges().any(|edge| edge.from == source
            && edge.to == target
            && edge.sub_kind == ArrangementSubKind::FrameMember));
    }
    assert_eq!(
        restored.node_history_projection(first).entries,
        [VISITED, FOLLOWUP]
    );
    assert_eq!(restored.node_history_projection(second).entries, [VISITED]);
    assert_eq!(
        node_history_back(&mut restored, first).as_deref(),
        Some(VISITED)
    );
    assert_eq!(
        node_history_forward(&mut restored, first).as_deref(),
        Some(FOLLOWUP)
    );

    // Re-export through the public command to check the durable session
    // provenance and the RDF event/visit identities independently of Graph.
    let exported_again = exchange.path().join("again.trig");
    export(&fresh, reopened, &exported_again).unwrap();
    assert_eq!(fs::read_to_string(&exported_again).unwrap(), rdf);
    if let Some(directory) = std::env::var_os("TURNSTONE_TEST_RDF_TRAIL_EVIDENCE") {
        let directory = Path::new(&directory);
        fs::copy(&trail, directory.join("trail.trig")).unwrap();
        fs::copy(&exported_again, directory.join("reexport.trig")).unwrap();
    }
    let parsed = mere::formats::parse::parse_trig(&rdf, None).unwrap();
    let quads: Vec<_> = parsed.quads(mere::dataset::View::Live).collect();
    let scope = format!("urn:graphshell:browsing-trail:{}", id.as_uuid());
    assert!(
        quads
            .iter()
            .all(|q| q.graph_name.to_string() == format!("<{scope}>"))
    );
    let derived = "http://www.w3.org/ns/prov#wasDerivedFrom";
    let rows: Vec<_> = quads
        .iter()
        .filter(|q| {
            q.predicate.as_str() == derived && q.subject.to_string().contains(":traversal:")
        })
        .collect();
    assert_eq!(rows.len(), 3);
    assert!(
        rows.iter()
            .all(|q| q.object.to_string() == format!("<{scope}>"))
    );
    println!(
        "source session {}, reopened session {}, repeated surfaces ({FIRST_ID}, {SECOND_ID}), clicks {:?}, RDF quads {}",
        id.as_uuid(),
        reopened.as_uuid(),
        clicks(&restored),
        quads.len()
    );

    // A wrong source cannot be admitted under the original graph.
    let foreign = "urn:graphshell:browsing-trail:00000000-0000-0000-0000-000000000000";
    let old = format!("<{derived}> <{scope}>");
    assert!(rdf.contains(&old));
    let mut corrupt = rdf;
    corrupt = corrupt.replacen(&old, &format!("<{derived}> <{foreign}>"), 1);
    let bad = exchange.path().join("bad.trig");
    fs::write(&bad, corrupt).unwrap();
    let bad_profile = exchange.path().join("bad");
    assert!(reopen(&bad, &bad_profile).is_err());
    assert!(!bad_profile.exists());
}
