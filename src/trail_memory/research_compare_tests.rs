// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::research::{Bridge, ClipAccess, Context, Target};
use std::sync::Arc;

#[test]
fn model_free_comparison_retained_capture_series_and_withdrawal() {
    let root = tempfile::tempdir().unwrap();
    let session = crate::panes::SessionId::new();
    let directory = root
        .path()
        .join(session.as_uuid().to_string())
        .join("memory");
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = FjallStore::open(&directory).unwrap();
    let mut manifests = vec![];
    for (text, time) in [
        ("<main>café old</main>", 1),
        ("<main>café 🐦 new</main>", 2),
    ] {
        let capture = capture_source_document_in(
            &mut store,
            &crate::content::FetchedDocument {
                bytes: text.as_bytes().to_vec(),
                body: text.into(),
                content_type: Some("text/html".into()),
                effective_url: Some("https://absent.test/article".into()),
                acquired_at_ms: time,
            },
        )
        .unwrap();
        manifests.push(capture.annotation_manifest);
    }
    drop(store);
    let graph = crate::panes::GraphId::new();
    let node = uuid::Uuid::new_v4();
    let admissions: Vec<_> = manifests
        .iter()
        .enumerate()
        .map(|(occurrence, manifest)| crate::saved_sources::Admission {
            key: crate::saved_sources::Key {
                session,
                graph,
                node,
                annotation_manifest: manifest.clone(),
            },
            occurrence,
            title: "Same origin".into(),
            url: "https://absent.test/article".into(),
            error: None,
        })
        .collect();
    let context = Context {
        session,
        generation: uuid::Uuid::new_v4(),
        admissions: admissions.clone(),
        clips: vec![],
    };
    let custody = custody_test_fixture::custody();
    let (actor, _) = spawn_trail_for_session(
        Arc::new(|| {}),
        Some(directory),
        Default::default(),
        context.generation,
    );
    assert!(actor.command(TrailCommand::SetCustody(Some(custody.clone()))));
    let bridge = Bridge::default();
    bridge.set_available(true);
    bridge.project(context.clone());
    bridge.access(Some(ClipAccess {
        root: root.path().into(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    let (_, revision) = bridge.context_revision().unwrap();
    let search = crate::research_search::Request {
        id: uuid::Uuid::new_v4(),
        context: context.clone(),
        revision,
        epoch: 1,
        query_revision: 1,
        selected: None,
        query: Default::default(),
        settings: Default::default(),
    };
    let result = crate::research_search::run(&bridge, &actor, &search).unwrap();
    let target = |key: &crate::saved_sources::Key| {
        result
            .inputs
            .iter()
            .filter_map(|i| crate::research::target(&i.source, &i.version).ok())
            .find(|t| matches!(t, Target::Capture(t) if &t.key == key))
            .unwrap()
    };
    let left = target(&admissions[0].key);
    let right = target(&admissions[1].key);
    let mut request = crate::research_compare::Request {
        id: uuid::Uuid::new_v4(),
        context: context.clone(),
        revision,
        epoch: 1,
        left,
        right,
        settings: Default::default(),
    };
    let compared = crate::research_compare::run(&bridge, &actor, &request).unwrap();
    assert!(compared.left.text.contains("café old"));
    assert!(compared.right.text.contains("café 🐦 new"));
    assert_ne!(
        compared.comparison.old.source,
        compared.comparison.new.source
    );
    assert_eq!(
        compared.comparison.relationship,
        eidetic_search::compare::ComparisonRelationship::Explicit
    );
    let mut withdrawn = context.clone();
    withdrawn.admissions.remove(0);
    bridge.project(withdrawn.clone());
    let (_, changed_revision) = bridge.context_revision().unwrap();
    request.context = withdrawn;
    request.revision = changed_revision;
    assert!(
        crate::research_compare::run(&bridge, &actor, &request)
            .unwrap_err()
            .contains("withdrawn")
    );
    // The exact stored bytes become readable again only after explicit re-admission.
    bridge.project(context.clone());
    request.context = context;
    request.revision = bridge.context_revision().unwrap().1;
    assert_eq!(
        crate::research_compare::run(&bridge, &actor, &request)
            .unwrap()
            .left
            .text,
        compared.left.text
    );
}
