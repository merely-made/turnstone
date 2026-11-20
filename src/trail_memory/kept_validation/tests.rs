// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::{collections::BTreeSet, sync::Arc};

use super::*;
use crate::content::FetchedDocument;
use crate::panes::{GraphId, SessionId};
use crate::saved_sources::Key;
use crate::source_capture::SourceIdentity;

const HTML: &str = r#"<!doctype html>
<html>
<head><title>Field notes</title><base href="/archive/"></head>
<body>
<p>Notes from the survey.</p>
<script type="application/ld+json" id="accepted">
{"@context":{"ex":"https://example.invalid/vocab#"},"@id":"readable-claim",
 "@type":"ex:Record","ex:score":7}
</script>
<script type="application/ld+json" id="malformed">{"incomplete":</script>
<script type="application/ld+json" id="remote">
{"@context":"https://contexts.invalid/should-not-load","@id":"remote-claim"}
</script>
<script type="application/ld+json" id="finding">
{"@context":{"ex":"https://example.invalid/vocab#"},"@id":"invalid-claim",
 "@type":"ex:Record","ex:score":"l1-private-claim-canary"}
</script>
</body>
</html>"#;
const SHAPES: &str = r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix ex: <https://example.invalid/vocab#> .
ex:RecordShape a sh:NodeShape ;
    sh:targetClass ex:Record ;
    sh:name "l1-private-shapes-canary" ;
    sh:property [ sh:path ex:score ; sh:datatype xsd:integer ] .
"#;
const ORIGIN: &str = "https://page.invalid/articles/field-notes";
const ACCEPTED: &str = "https://page.invalid/archive/readable-claim";
const INVALID: &str = "https://page.invalid/archive/invalid-claim";
const PATH: &str = "https://example.invalid/vocab#score";

fn order(report: &Report, id: &str) -> u64 {
    report
        .blocks
        .iter()
        .find(|block| block.element_id.as_deref() == Some(id))
        .unwrap()
        .document_order
}

fn assert_finding(report: &Report) {
    assert_eq!(report.origin, ORIGIN);
    assert_eq!(report.blocks.len(), 4);
    assert_eq!(report.claims.as_ref().unwrap().rows.len(), 4);
    assert!(!report.conforms);
    assert_eq!(report.diagnostics.len(), 2);
    for id in ["malformed", "remote"] {
        let prefix = format!("JSON-LD block {}: ", order(report, id) + 1);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|issue| issue.starts_with(&prefix))
        );
    }

    assert_eq!(report.findings.len(), 1);
    let entry = &report.findings[0];
    assert_eq!(entry.finding.focus_node, format!("<{INVALID}>"));
    assert_eq!(entry.finding.path.as_deref(), Some(PATH));
    assert_eq!(
        entry
            .evidence
            .iter()
            .map(|evidence| evidence.id)
            .collect::<Vec<_>>(),
        vec![order(report, "finding")],
    );
}

#[test]
fn kept_validation_reopens_from_sealed_source_at_the_same_cursor() {
    let root = tempfile::tempdir().unwrap();
    let session_id = SessionId::new();
    let directory = super::super::memory_dir(&root.path().join(session_id.as_uuid().to_string()));
    std::fs::create_dir_all(&directory).unwrap();
    let sealed_dir = super::super::sealed_memory_dir(&directory);
    let shapes_path = root.path().join("record-shapes.ttl");
    std::fs::write(&shapes_path, SHAPES).unwrap();

    let custody = SourceCustody {
        sealer: Arc::new(sporran::WalletEpochSealer::from_epoch(
            pandect::PersonaId::new(),
            sporran::KeyEpochId::new(),
            b"l1 kept-source fixture",
        )),
        epoch: Arc::new(|| Some(1)),
    };
    let limits = Ok(StorageLimits::default());
    let document = FetchedDocument {
        bytes: HTML.as_bytes().to_vec(),
        body: HTML.into(),
        effective_url: Some(ORIGIN.into()),
        content_type: Some("text/html; charset=utf-8".into()),
        acquired_at_ms: 1,
    };
    let capture = CaptureRequest {
        id: Uuid::new_v4(),
        session: session_id,
        graph: GraphId::new(),
        node: Uuid::new_v4(),
        url: "https://page.invalid/redirect".into(),
        source: Some(SourceIdentity::of(&document)),
        keep: true,
    };
    let mut store = FjallStore::open(&sealed_dir).unwrap();
    let stored = sealed_capture::capture_request(
        &mut store,
        custody.sealer.as_ref(),
        &capture,
        Some(&document),
    )
    .unwrap();
    sealed_capture::acknowledge(&mut store, custody.sealer.as_ref(), &capture).unwrap();
    drop(document);

    let source = KeptSource {
        key: Key {
            session: capture.session,
            graph: capture.graph,
            node: capture.node,
            annotation_manifest: stored.annotation_manifest.clone(),
        },
        request: capture.id,
        capture_hash: stored.raw_manifest.clone(),
    };
    let request = |generation, shapes| ValidationRequest {
        generation,
        request: Uuid::new_v4(),
        pane: PaneId(1),
        source: source.clone(),
        capture: capture.clone(),
        stored: stored.clone(),
        directory: directory.clone(),
        shapes,
    };
    let generation = Uuid::new_v4();
    let live = request(generation, Shapes::File(shapes_path.clone()))
        .run(
            &directory,
            generation,
            Some(&custody),
            Some(&store),
            &limits,
        )
        .result
        .unwrap();
    assert_finding(&live);
    assert!(live.cursor > 0);
    drop(store);

    let edition_digest = eidetic::Hash::of(stored.annotation_manifest.as_bytes()).to_hex();
    let journal_path = directory
        .join("structured-validation")
        .join(&edition_digest);
    let disk = DirectoryBackend::open(&journal_path).unwrap();
    assert_journal_sealed(&disk);
    let journal_graph = NamedNode::new(format!("urn:mere:kept-source:{edition_digest}")).unwrap();
    pollster::block_on(async {
        let backend = sporran::WalletSealedBackend::with_sealer(disk, custody.sealer.clone());
        let sessions = MereSessions::new(backend);
        let manifest = sessions.latest_live().await.unwrap().unwrap();
        let session = sessions.open(manifest.session_id).await.unwrap();
        let kept = session.kept_page_claims(&journal_graph).unwrap().unwrap();
        assert_eq!(session.journal().live_cursor().0, live.cursor);
        assert_eq!(kept.claims.blocks.len(), 2);
        let subjects: BTreeSet<_> = kept
            .dataset()
            .quads(View::Live)
            .filter(|quad| quad.predicate.as_str() == PATH)
            .map(|quad| quad.subject.to_string())
            .collect();
        assert_eq!(
            subjects,
            BTreeSet::from([format!("<{ACCEPTED}>"), format!("<{INVALID}>")])
        );
    });

    // The stored Turtle and exact sealed response own the reopen. Drop the
    // original store handle, delete the caller's Turtle and change generation.
    std::fs::remove_file(&shapes_path).unwrap();
    let generation_after_restart = Uuid::new_v4();
    let store = FjallStore::open(&sealed_dir).unwrap();
    assert!(
        request(generation, Shapes::Reopen)
            .run(
                &directory,
                generation_after_restart,
                Some(&custody),
                Some(&store),
                &limits
            )
            .result
            .is_err()
    );
    let reopened = request(generation_after_restart, Shapes::Reopen)
        .run(
            &directory,
            generation_after_restart,
            Some(&custody),
            Some(&store),
            &limits,
        )
        .result
        .unwrap();
    assert_finding(&reopened);
    assert_eq!(reopened.cursor, live.cursor);
    assert_eq!(reopened.blocks, live.blocks);
    assert_eq!(reopened.diagnostics, live.diagnostics);
    assert_eq!(
        reopened.findings[0].finding.severity,
        live.findings[0].finding.severity
    );
    assert_eq!(
        reopened.findings[0].finding.message,
        live.findings[0].finding.message
    );
}

fn assert_journal_sealed(disk: &DirectoryBackend) {
    pollster::block_on(async {
        let unrelated_key = Arc::new(sporran::WalletEpochSealer::from_epoch(
            pandect::PersonaId::new(),
            sporran::KeyEpochId::new(),
            b"other l1 fixture epoch",
        ));
        let unrelated = sporran::WalletSealedBackend::with_sealer(disk.clone(), unrelated_key);
        assert!(unrelated.get(SHAPES_KEY).await.is_err());
        let keys = disk.list("").await.unwrap();
        assert!(keys.iter().any(|key| key == SHAPES_KEY));
        for key in keys {
            let bytes = disk.get(&key).await.unwrap().unwrap();
            for canary in [
                b"l1-private-claim-canary".as_slice(),
                b"l1-private-shapes-canary",
            ] {
                assert!(!bytes.windows(canary.len()).any(|window| window == canary));
            }
        }
    });
}
