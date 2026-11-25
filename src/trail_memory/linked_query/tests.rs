// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::{
    content::FetchedDocument,
    panes::{GraphId, SessionId},
    research::{ClipAccess, Context},
    source_capture::{CaptureRequest, SourceIdentity},
};
use std::{fs, path::PathBuf};

const CITES: &str = "http://purl.org/spar/cito/cites";
const TARGET: &str = "https://example.org/work";

fn capture(
    store: &mut FjallStore,
    custody: &SourceCustody,
    session: SessionId,
    graph: GraphId,
    node: Uuid,
    document: &str,
    subject: &str,
    at: u64,
) -> (Admission, String) {
    let payload = serde_json::json!({"@id":subject,CITES:{"@id":TARGET}}).to_string();
    let html = format!(
        "<html><head><script type=\"application/ld+json\">{payload}</script></head><body><main>A retained page.</main></body></html>"
    );
    let response = FetchedDocument {
        bytes: html.as_bytes().to_vec(),
        body: html,
        effective_url: Some(document.into()),
        content_type: Some("text/html".into()),
        http_response: None,
        acquired_at_ms: at,
    };
    let request = CaptureRequest {
        id: Uuid::new_v4(),
        session,
        graph,
        node,
        url: document.into(),
        source: Some(SourceIdentity::of(&response)),
        keep: true,
    };
    let stored =
        sealed_capture::capture_request(store, custody.sealer.as_ref(), &request, Some(&response))
            .unwrap();
    sealed_capture::acknowledge(store, custody.sealer.as_ref(), &request).unwrap();
    (
        Admission {
            key: Key {
                session,
                graph,
                node,
                annotation_manifest: stored.annotation_manifest,
            },
            occurrence: 0,
            title: subject.into(),
            url: document.into(),
            error: None,
        },
        payload,
    )
}

#[test]
fn linked_query_reopens_and_rebinds_only_disclosed_revision() {
    let root = tempfile::tempdir().unwrap();
    let session = SessionId::from_uuid(Uuid::from_u128(1));
    let graph = GraphId::from_uuid(Uuid::from_u128(2));
    let generation = Uuid::new_v4();
    let dir = root
        .path()
        .join(session.as_uuid().to_string())
        .join("memory");
    let sealed = dir.join("sealed");
    fs::create_dir_all(&sealed).unwrap();
    let custody = crate::trail_memory::custody_test_fixture::custody();
    let mut store = FjallStore::open(&sealed).unwrap();
    let (a, first) = capture(
        &mut store,
        &custody,
        session,
        graph,
        Uuid::from_u128(3),
        "https://example.org/a",
        "urn:edition:first",
        1,
    );
    let (b, second) = capture(
        &mut store,
        &custody,
        session,
        graph,
        Uuid::from_u128(4),
        "https://example.org/b",
        "urn:edition:second",
        2,
    );
    let (newer, _) = capture(
        &mut store,
        &custody,
        session,
        graph,
        Uuid::from_u128(3),
        "https://example.org/a",
        "urn:edition:new",
        3,
    );
    let bridge = Bridge::default();
    bridge.set_available(true);
    bridge.access(Some(ClipAccess {
        root: root.path().into(),
        sealer: custody.sealer.clone(),
        epoch: custody.epoch.clone(),
    }));
    let context = |admissions| Context {
        session,
        generation,
        admissions,
        clips: vec![],
    };
    bridge.project(context(vec![a.clone(), b.clone(), newer.clone()]));
    let profile = selected_profile().unwrap();
    let scope = Stamp::current(&bridge).unwrap().scope();
    let ids = vec![linked_queries::source_id(&a), linked_queries::source_id(&b)];
    assert_eq!(ids[0], linked_queries::source_id(&newer));
    let view = turnstone_ontology::SavedView::new(
        "Citations of the work".into(),
        scope,
        ids.clone(),
        turnstone_ontology::ViewQuery::Incoming {
            record: TARGET.into(),
        },
        profile,
    )
    .unwrap();
    let bind = Operation::Bind(vec![a.key.clone(), b.key.clone()]);
    let execute = |view: &turnstone_ontology::SavedView, op, store: &FjallStore| {
        let job = Request {
            token: Uuid::new_v4(),
            stamp: Stamp::current(&bridge).unwrap(),
            view: Some(view.clone()),
            operation: op,
        };
        run(
            job,
            &bridge,
            &dir,
            generation,
            Some(&custody),
            Some(store),
            &Ok(StorageLimits::default()),
        )
        .result
    };
    let first_run = execute(&view, bind, &store).unwrap();
    assert_eq!(first_run.rows[0].statements[0].subject, "urn:edition:first");
    assert_eq!(
        first_run.rows[1].statements[0].subject,
        "urn:edition:second"
    );
    let empty_view = turnstone_ontology::SavedView::new(
        "Other work".into(),
        view.scope.clone(),
        ids.clone(),
        turnstone_ontology::ViewQuery::Incoming {
            record: "urn:work:other".into(),
        },
        profile,
    )
    .unwrap();
    let empty = execute(&empty_view, Operation::Run, &store).unwrap();
    assert_eq!(empty.rows.len(), 2);
    assert!(
        empty
            .rows
            .iter()
            .all(|row| row.revision.is_some() && row.statements.is_empty())
    );
    let revision = first_run.rows[0].revision.clone().unwrap();
    let path = root.path().join("query.json");
    linked_queries::export(&view, &path).unwrap();
    let reopened = linked_queries::import(&path).unwrap();
    drop(store);
    let store = FjallStore::open(&sealed).unwrap();
    let names = execute(&reopened, Operation::Catalog, &store)
        .unwrap()
        .names;
    assert_eq!(names, vec![reopened.name.clone()]);
    let restored = execute(&reopened, Operation::Named(reopened.name.clone()), &store).unwrap();
    assert_eq!(restored.rows, first_run.rows);
    let open = Operation::Open {
        source: ids[0].clone(),
        revision: revision.clone(),
    };
    assert_eq!(
        execute(&reopened, open.clone(), &store)
            .unwrap()
            .open
            .unwrap()
            .key,
        a.key
    );
    if let Some(out) = std::env::var_os("TURNSTONE_LINKED_FIXTURE_DIR").map(PathBuf::from) {
        fs::create_dir_all(&out).unwrap();
        linked_queries::export(&reopened, &out.join("query.json")).unwrap();
        fs::write(out.join("a.jsonld"), first).unwrap();
        fs::write(out.join("b.jsonld"), second).unwrap();
        fs::write(
            out.join("expected.json"),
            serde_json::to_vec_pretty(&restored.rows).unwrap(),
        )
        .unwrap();
    }
    if let Ok(domain) = std::env::var("TURNSTONE_LINKED_RESIDENT_DOMAIN") {
        let _held = crate::resident_graph::open_fixture_graph(&bridge.1, &domain).unwrap();
        let destination = execute(&reopened, Operation::Preview(domain.clone()), &store)
            .unwrap()
            .destination
            .unwrap();
        assert_eq!(destination.count, 2);
        let shared = execute(
            &reopened,
            Operation::Share {
                domain: domain.clone(),
                destination: destination.scope.clone(),
                session: destination.session.clone(),
                expected: destination.digest.clone(),
            },
            &store,
        )
        .unwrap();
        assert!(shared.message.starts_with("Shared "));
        let out = PathBuf::from(
            std::env::var_os("TURNSTONE_LINKED_FIXTURE_DIR")
                .expect("Set a fixture output directory for the resident test"),
        );
        let receipt = serde_json::json!({
            "domain": domain,
            "scope": destination.scope,
            "session": destination.session,
            "name": reopened.name,
            "statement_count": destination.count,
        });
        fs::write(
            out.join("resident-share.json"),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
    }
    let changed = execute(&reopened, Operation::Bind(vec![newer.key.clone()]), &store).unwrap();
    assert_eq!(changed.rows[0].statements[0].subject, "urn:edition:new");
    assert_ne!(changed.rows[0].revision.as_deref(), Some(revision.as_str()));
    assert!(execute(&reopened, open, &store).is_err());
    bridge.project(context(vec![a.clone(), b]));
    let withdrawn = execute(&reopened, Operation::Run, &store).unwrap();
    assert_eq!(withdrawn.rows[0].revision, None);
    assert_eq!(withdrawn.rows[1], restored.rows[1]);
    let stale = Request {
        token: Uuid::new_v4(),
        stamp: Stamp::current(&bridge).unwrap(),
        view: Some(reopened),
        operation: Operation::Run,
    };
    bridge.set_available(false);
    assert!(
        run(
            stale,
            &bridge,
            &dir,
            generation,
            Some(&custody),
            Some(&store),
            &Ok(StorageLimits::default())
        )
        .result
        .is_err()
    );
}
