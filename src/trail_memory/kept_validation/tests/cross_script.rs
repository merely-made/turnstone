// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

const REOPEN_DIRECTORY: &str = "TURNSTONE_TEST_CROSS_SCRIPT_REOPEN";
const REOPEN_CAPTURE: &str = "TURNSTONE_TEST_CROSS_SCRIPT_CAPTURE";

fn validate(directory: &Path, page: &KeptPage, store: &FjallStore, shapes: Shapes) -> Report {
    let generation = Uuid::new_v4();
    page.request(directory, generation, shapes)
        .run(
            directory,
            generation,
            Some(&page.custody),
            Some(store),
            &Ok(StorageLimits::default()),
        )
        .result
        .unwrap()
}

fn checked_report(report: &Report) -> serde_json::Value {
    assert!(report.diagnostics.is_empty());
    assert!(report.conforms && report.findings.is_empty());
    let kept = report.projection.kept();
    let quads: Vec<_> = kept.dataset().quads(View::Live).collect();
    assert_eq!(quads.len(), 2);
    let river = quads
        .iter()
        .find(|q| {
            q.predicate == oxrdf::vocab::rdf::TYPE
                && q.object == oxrdf::Term::from(NamedNode::new_unchecked("urn:survey:River"))
        })
        .unwrap();
    assert!(river.subject.is_blank_node());
    let score = NamedNode::new_unchecked("urn:survey:score");
    assert!(quads.iter().any(|q| q.subject == river.subject
        && q.predicate == score
        && q.object == oxrdf::Term::from(oxrdf::Literal::from(7))));
    let focus: oxrdf::Term = river.subject.clone().into();
    let sources: Vec<_> = kept
        .evidence_for(&focus, None)
        .iter()
        .map(|b| b.order)
        .collect();
    assert_eq!(sources, [order(report, "type"), order(report, "score")]);
    assert_eq!(
        kept.evidence_for(&focus, Some(&score))
            .iter()
            .map(|b| b.order)
            .collect::<Vec<_>>(),
        [order(report, "score")]
    );
    let report_quads: Vec<_> = report
        .projection
        .report()
        .dataset
        .iter()
        .map(oxrdf::QuadRef::into_owned)
        .collect();
    let canonical_report =
        mere::formats::canon::canonicalize_quads(&report_quads, &Default::default())
            .unwrap()
            .nquads;
    assert!(canonical_report.contains("<http://www.w3.org/ns/shacl#conforms>"));
    let revision = report.projection.revision();
    serde_json::json!({
        "session": revision.session.as_uuid(), "cursor": revision.cursor.0, "digest": revision.digest,
        "node": focus.to_string(), "blocks": sources, "report": canonical_report,
    })
}

#[test]
fn cross_script_blank_node_conforms_and_reopens_with_both_blocks_offline() {
    const PAGE: &str = r#"<!doctype html><title>Survey</title><p>River observations.</p>
    <script type="application/ld+json" id="type">
        {"@id":"_:river","@type":"urn:survey:River"}
    </script><script type="application/ld+json" id="score">
        {"@id":"_:river","urn:survey:score":7}
    </script>"#;
    const REQUIRED_SCORE: &str = r#"
        @prefix sh: <http://www.w3.org/ns/shacl#> .
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
        <urn:survey:shape> a sh:NodeShape ; sh:targetClass <urn:survey:River> ;
            sh:property [ sh:path <urn:survey:score> ; sh:minCount 1 ; sh:datatype xsd:integer ] .
    "#;

    if let Some(directory) = std::env::var_os(REOPEN_DIRECTORY) {
        assert!(crate::offline::enabled());
        let directory = PathBuf::from(directory);
        let selected: Uuid = std::env::var(REOPEN_CAPTURE).unwrap().parse().unwrap();
        let mut store =
            FjallStore::open(crate::trail_memory::sealed_memory_dir(&directory)).unwrap();
        let custody = fixture_custody();
        let receipt = sealed_capture::receipts(&mut store, custody.sealer.as_ref())
            .unwrap()
            .into_iter()
            .find(|r| r.request.id == selected && r.saved)
            .unwrap();
        let page = kept_from_receipt(custody, receipt.request, receipt.stored.unwrap());
        let report = validate(&directory, &page, &store, Shapes::Reopen);
        println!("\nREOPENED {}", checked_report(&report));
        return;
    }

    let root = tempfile::tempdir().unwrap();
    let session = SessionId::new();
    let directory =
        crate::trail_memory::memory_dir(&root.path().join(session.as_uuid().to_string()));
    std::fs::create_dir_all(&directory).unwrap();
    let shapes = root.path().join("river.ttl");
    std::fs::write(&shapes, REQUIRED_SCORE).unwrap();
    let sealed_dir = crate::trail_memory::sealed_memory_dir(&directory);
    let mut store = FjallStore::open(&sealed_dir).unwrap();
    let page = keep_page(session, &mut store, PAGE, fixture_custody());
    // Changing only the second label disconnects the score from the SHACL target.
    let other_text = PAGE.replace(
        r#""_:river","urn:survey:score""#,
        r#""_:other","urn:survey:score""#,
    );
    let other = keep_page(session, &mut store, &other_text, page.custody.clone());
    let disjoint = validate(&directory, &other, &store, Shapes::File(shapes.clone()));
    assert!(!disjoint.conforms);
    assert_eq!(disjoint.findings.len(), 1);
    assert_eq!(
        disjoint.findings[0].finding.path.as_deref(),
        Some("urn:survey:score")
    );
    drop(disjoint);

    let live = validate(&directory, &page, &store, Shapes::File(shapes.clone()));
    let expected = checked_report(&live);
    drop(live);
    drop(store);
    std::fs::remove_file(shapes).unwrap();
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            std::thread::current().name().unwrap(),
            "--nocapture",
        ])
        .env("TURNSTONE_OFFLINE", "1")
        .env(REOPEN_DIRECTORY, &directory)
        .env(REOPEN_CAPTURE, page.capture.id.to_string())
        .output()
        .unwrap();
    let stdout = String::from_utf8(child.stdout).unwrap();
    assert!(
        child.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&child.stderr)
    );
    let result = stdout
        .lines()
        .find_map(|line| line.strip_prefix("REOPENED "))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(result).unwrap(),
        expected
    );
    println!("Matched live and offline validation: {result}");
}
