// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use incipit_session::IncipitSessions;
use std::{collections::BTreeSet, sync::Arc};

use mere::dataset::View;

use super::*;
use crate::content::FetchedDocument;
use crate::panes::{GraphId, SessionId};
use crate::saved_sources::{KeptSource, Key};
use crate::source_capture::{CaptureRequest, SourceIdentity};
use crate::swatch_pane::validation::EvidenceTarget;
use crate::trail_memory::sealed_capture;

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
const INDEPENDENT_SOURCE: &str = "urn:fixture:independent-kept-source";

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

struct KeptPage {
    custody: SourceCustody,
    source: KeptSource,
    capture: CaptureRequest,
    stored: crate::action::StoredSourceDocument,
}

impl KeptPage {
    fn request(&self, directory: &Path, generation: Uuid, shapes: Shapes) -> ValidationRequest {
        ValidationRequest {
            request: Uuid::new_v4(),
            pane: PaneId(1),
            shapes,
            read: KeptDocumentRead {
                generation,
                source: self.source.clone(),
                capture: self.capture.clone(),
                stored: self.stored.clone(),
                directory: directory.to_owned(),
            },
        }
    }
}

fn fixture_custody() -> SourceCustody {
    SourceCustody {
        sealer: Arc::new(sporran::WalletEpochSealer::from_epoch(
            pandect::PersonaId(Uuid::from_u128(17)),
            sporran::KeyEpochId(Uuid::from_u128(19)),
            b"l1 kept-source fixture",
        )),
        epoch: Arc::new(|| Some(1)),
    }
}

fn keep_page(
    session_id: SessionId,
    store: &mut FjallStore,
    html: &str,
    custody: SourceCustody,
) -> KeptPage {
    let document = FetchedDocument {
        bytes: html.as_bytes().to_vec(),
        body: html.into(),
        effective_url: Some(ORIGIN.into()),
        content_type: Some("text/html; charset=utf-8".into()),
        http_response: None,
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

    let stored =
        sealed_capture::capture_request(store, custody.sealer.as_ref(), &capture, Some(&document))
            .unwrap();
    sealed_capture::acknowledge(store, custody.sealer.as_ref(), &capture).unwrap();
    drop(document);

    kept_from_receipt(custody, capture, stored)
}

fn kept_from_receipt(
    custody: SourceCustody,
    capture: CaptureRequest,
    stored: crate::action::StoredSourceDocument,
) -> KeptPage {
    let source = KeptSource {
        key: Key {
            session: capture.session,
            graph: capture.graph,
            node: capture.node,
            annotation_manifest: stored.annotation_manifest.clone(),
        },
        request: capture.id,
        capture_hash: capture.source.as_ref().unwrap().hash.clone(),
    };
    KeptPage {
        custody,
        source,
        capture,
        stored,
    }
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

    let mut store = FjallStore::open(&sealed_dir).unwrap();
    let page = keep_page(session_id, &mut store, HTML, fixture_custody());
    let custody = &page.custody;
    let limits = Ok(StorageLimits::default());
    let request = |generation, shapes| page.request(&directory, generation, shapes);
    let generation = Uuid::new_v4();
    let live = request(generation, Shapes::File(shapes_path.clone()))
        .run(&directory, generation, Some(custody), Some(&store), &limits)
        .result
        .unwrap();
    assert_finding(&live);
    assert!(live.projection.revision().cursor.0 > 0);
    drop(store);

    let edition_digest = eidetic::Hash::of(page.stored.annotation_manifest.as_bytes()).to_hex();
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
        assert_eq!(
            session.journal().live_cursor(),
            live.projection.revision().cursor
        );
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
                Some(custody),
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
            Some(custody),
            Some(&store),
            &limits,
        )
        .result
        .unwrap();
    assert_finding(&reopened);
    assert_eq!(reopened.projection.revision(), live.projection.revision());
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

    let evidence = |report: &Report| EvidenceRequest {
        generation: generation_after_restart,
        pane: PaneId(1),
        target: EvidenceTarget {
            source: page.source.clone(),
            report: report.id,
            order: order(report, "finding"),
        },
        revision: report.projection.revision().clone(),
        directory: directory.clone(),
    };
    let open_evidence = |report: &Report| {
        evidence(report)
            .run(&directory, generation_after_restart, Some(custody))
            .result
    };
    assert!(open_evidence(&reopened).is_ok());
    let (before, advanced) = append_independent_source(&journal_path, custody);
    let outdated = open_evidence(&reopened).unwrap_err();
    assert!(
        outdated.contains("Reopen validation for this capture"),
        "{outdated}"
    );
    assert_eq!(&before, reopened.projection.revision());
    assert!(advanced.cursor > before.cursor);
    assert_ne!(advanced.digest, before.digest);
    let independent_source = NamedNode::new(INDEPENDENT_SOURCE).unwrap();
    assert!(
        reopened
            .projection
            .pin()
            .kept_page_claims(&independent_source)
            .unwrap()
            .is_none()
    );
    assert_finding(&reopened);

    let refreshed = request(generation_after_restart, Shapes::Reopen)
        .run(
            &directory,
            generation_after_restart,
            Some(custody),
            Some(&store),
            &limits,
        )
        .result
        .unwrap();
    assert_eq!(refreshed.projection.revision(), &advanced);
    assert_finding(&refreshed);
    assert!(open_evidence(&refreshed).is_ok());
}

mod cross_script;

fn append_independent_source(
    journal: &Path,
    custody: &SourceCustody,
) -> (pandect::GraphRevision, pandect::GraphRevision) {
    use mere::acquire::page_claims::interpret;

    pollster::block_on(async {
        let disk = DirectoryBackend::open(journal).unwrap();
        let backend = sporran::WalletSealedBackend::with_sealer(disk, custody.sealer.clone());
        let sessions = MereSessions::new(backend);
        let manifest = sessions.latest_live().await.unwrap().unwrap();
        let mut session = sessions.open(manifest.session_id).await.unwrap();
        let before = session.live_revision().unwrap();
        let source = NamedNode::new(INDEPENDENT_SOURCE).unwrap();
        let claims = interpret(
            &mere::ContextCache::new(),
            ORIGIN,
            [(
                0,
                r#"{"@id":"urn:fixture:other","https://example.invalid/vocab#score":42}"#,
            )],
        );
        assert_eq!(claims.blocks.len(), 1);
        assert!(session.kept_page_claims(&source).unwrap().is_none());
        session
            .admit_page_claims(
                mere::model::author::Author::user(),
                &source,
                &claims,
                SystemTime::now(),
            )
            .await
            .unwrap();
        assert!(session.kept_page_claims(&source).unwrap().is_some());
        (before, session.live_revision().unwrap())
    })
}

// This edits the journal from outside the running GUI, with the same synthetic
// broker and directory ownership checks the GUI uses for a saved source.
#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires the isolated headed L1 pin fixture and its custody broker"]
fn kept_validation_advance_native_fixture() {
    use std::sync::atomic::AtomicU64;

    let fixture = PathBuf::from(std::env::var_os("TURNSTONE_L1_PINNED_FIXTURE").unwrap())
        .canonicalize()
        .unwrap();
    assert_eq!(
        std::fs::read(fixture.join("fixture-marker")).unwrap(),
        b"turnstone-l1-pinned-public-fixture/v1\n"
    );
    let profile = fixture.join("profile").canonicalize().unwrap();
    assert!(profile.starts_with(&fixture));
    let journal = PathBuf::from(std::env::var_os("TURNSTONE_L1_PINNED_JOURNAL").unwrap())
        .canonicalize()
        .unwrap();
    let scoped_path = journal.strip_prefix(&profile).unwrap();
    let components: Vec<_> = scoped_path
        .iter()
        .map(|component| component.to_str().unwrap())
        .collect();
    assert_eq!(components.len(), 5);
    assert_eq!(components[0], "sessions");
    assert!(Uuid::parse_str(components[1]).is_ok());
    assert_eq!(components[2], "memory");
    assert_eq!(components[3], "structured-validation");
    assert_eq!(components[4].len(), 64);
    assert!(components[4].bytes().all(|byte| byte.is_ascii_hexdigit()));

    let endpoint = std::env::var("GRAPHSHELL_APP_ENDPOINT").unwrap();
    assert_eq!(endpoint, r"\\.\pipe\turnstone-l1-pinned-check");
    let identity = crate::identity::bind_timeout(std::time::Duration::from_secs(2))
        .expect("isolated fixture custody");
    let bound =
        crate::custody_binding::BoundSealer::bind(identity, Arc::new(AtomicU64::new(1))).unwrap();
    let epoch_bound = bound.clone();
    let custody = SourceCustody {
        sealer: bound,
        epoch: Arc::new(move || epoch_bound.epoch()),
    };
    let (before, after) = append_independent_source(&journal, &custody);
    assert_eq!(after.session, before.session);
    assert_eq!(after.cursor.0, before.cursor.0 + 1);
    assert_ne!(after.digest, before.digest);
    println!("Fixture journal before edit: {before:?}");
    println!("Fixture journal after edit: {after:?}");
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
