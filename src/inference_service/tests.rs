// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use distillery::inference_protocol::{
    RunStateV1,
    recall::{Citation, CitationBasis, RecallEvidence, SuppliedPassage},
};
use eidetic::models::recall::{InferencePassage, RecallSourceKind};
struct Source {
    text: String,
}
impl RetainedSourceOwner for Source {
    fn snapshot(&self, _: &str) -> Result<Vec<RetainedSourceText>, String> {
        unreachable!()
    }
    fn resolve(&self, _: &RecallSourceId, version: &str) -> Result<String, String> {
        if version != eidetic::Hash::of(self.text.as_bytes()).to_string() {
            return Err("wrong version".into());
        }
        Ok(self.text.clone())
    }
}
fn fixture() -> (Source, Vec<RunV1>) {
    let text = "Retained nebula source version: amberstone.".to_string();
    let passage = InferencePassage {
        source: RecallSourceId {
            kind: RecallSourceKind::PageText,
            owner: Some("turnstone/session".into()),
            source: "https://retained.test/note".into(),
            node: None,
        },
        version: eidetic::Hash::of(text.as_bytes()).to_string(),
        text_hash: eidetic::Hash::of(text.as_bytes()),
        selector: PassageSelector {
            start: 0,
            end: text.chars().count(),
        },
        chunker: "test/v1".into(),
        text: text.clone(),
    };
    let citation = Citation {
        handle: 1,
        passage: passage.id(),
        basis: CitationBasis::ModelMarker,
        answer_selector: PassageSelector { start: 0, end: 3 },
    };
    let run = RunV1 {
        run: "retained".into(),
        question: "nebula?".into(),
        state: RunStateV1::Finished,
        text: "[1]".into(),
        error: None,
        session: None,
        tokens: 1,
        continuations: vec![],
        recall: Some(RecallEvidence {
            answer_format: Default::default(),
            answer_contract: None,
            support: None,
            owner: Some("turnstone/session".into()),
            scope: vec![],
            config: Default::default(),
            embedding: None,
            passages: vec![SuppliedPassage { handle: 1, passage }],
            stale: vec![],
            citations: vec![citation],
            unknown_citations: vec![],
        }),
    };
    (Source { text }, vec![run])
}
#[test]
fn claim_inspection_rechecks_the_exact_subrange_and_raw_answer_binding() {
    let (owner, mut runs) = fixture();
    let raw = r#"{"claims":[{"status":"supported","text":"An amberstone version","evidence":[{"handle":1,"quote":"source version: amberstone."}]}]}"#;
    runs[0].text = raw.into();
    let r = runs[0].recall.as_mut().unwrap();
    r.support = Some(eidetic::models::answer::AnswerSupport::assess(
        raw,
        &[(1, &r.passages[0].passage)],
    ));
    let opened = resolve_claim_source(&owner, &runs, "retained", 1, 0, 0).unwrap();
    assert!(opened.selector.start > 0);
    assert_eq!(
        opened
            .retained
            .text
            .chars()
            .skip(opened.selector.start)
            .take(opened.selector.end - opened.selector.start)
            .collect::<String>(),
        "source version: amberstone."
    );
    runs[0]
        .recall
        .as_mut()
        .unwrap()
        .support
        .as_mut()
        .unwrap()
        .claims[0]
        .evidence[0]
        .quote = "different quotation".into();
    assert!(resolve_claim_source(&owner, &runs, "retained", 1, 0, 0).is_err());
    runs[0].text.push_str("changed output");
    assert!(resolve_claim_source(&owner, &runs, "retained", 1, 0, 0).is_err());
}
#[test]
fn citation_open_reads_the_exact_source_version_and_checks_its_passage_identity() {
    let (owner, mut runs) = fixture();
    let opened = resolve_source(&owner, &runs, "retained", 1).unwrap();
    assert_eq!(opened.retained.text, owner.text);
    runs[0].recall.as_mut().unwrap().citations[0].passage =
        eidetic::ManifestId::of_blob(b"other passage");
    assert!(
        resolve_source(&owner, &runs, "retained", 1)
            .unwrap_err()
            .contains("does not name")
    );
}
#[test]
fn withdrawn_citations_and_wrong_source_text_cannot_open() {
    let (mut owner, mut runs) = fixture();
    owner.text.push_str(" changed");
    assert!(resolve_source(&owner, &runs, "retained", 1).is_err());
    runs[0].recall.as_mut().unwrap().citations.clear();
    assert!(
        resolve_source(&owner, &runs, "retained", 1)
            .unwrap_err()
            .contains("stale or unavailable")
    );
}
#[test]
fn a_selector_beyond_the_source_is_refused_even_when_it_would_copy_the_same_text() {
    let (owner, mut runs) = fixture();
    let evidence = runs[0].recall.as_mut().unwrap();
    evidence.passages[0].passage.selector.end += 1;
    evidence.citations[0].passage = evidence.passages[0].passage.id();
    assert!(
        resolve_source(&owner, &runs, "retained", 1)
            .unwrap_err()
            .contains("range is invalid")
    );
    let evidence = runs[0].recall.as_mut().unwrap();
    evidence.passages[0].passage.selector.end = evidence.passages[0].passage.selector.start;
    evidence.citations[0].passage = evidence.passages[0].passage.id();
    assert!(
        resolve_source(&owner, &runs, "retained", 1)
            .unwrap_err()
            .contains("range is invalid")
    );
}
fn saved_fixture(text: &str) -> RetainedSourceText {
    let target = crate::saved_sources::Target {
        key: crate::saved_sources::Key {
            session: crate::panes::SessionId::new(),
            graph: crate::panes::GraphId::new(),
            node: uuid::Uuid::new_v4(),
            annotation_manifest: eidetic::ManifestId::of_blob(b"saved-A1").to_string(),
        },
        text_hash: eidetic::Hash::of(text.as_bytes()).to_string(),
        selector: None,
    };
    RetainedSourceText {
        source: crate::research::source_id(&target.clone().into()).unwrap(),
        version: target.text_hash,
        title: Some("A1".into()),
        text: text.into(),
    }
}
#[test]
fn publication_bounds_encoded_bytes_and_rejects_owner_hash_and_legacy_substitution() {
    let source = saved_fixture("ordinary saved text");
    let owner = source.source.owner.as_ref().unwrap().clone();
    assert!(publication(&owner, vec![source.clone()]).is_ok());
    assert!(publication("another owner", vec![source.clone()]).is_err());
    let mut corrupt = source.clone();
    corrupt.text.push('!');
    assert!(publication(&owner, vec![corrupt]).is_err());
    let mut legacy = source;
    legacy.source.kind = RecallSourceKind::PageText;
    assert!(publication(&owner, vec![legacy]).is_err());
    // JSON escaping exceeds the wire cap before raw Unicode character count.
    let big = saved_fixture(&"\u{0000}".repeat(MAX_PUBLISHED_CORPUS_BYTES / 5));
    let big_owner = big.source.owner.as_ref().unwrap().clone();
    assert!(
        publication(&big_owner, vec![big])
            .unwrap_err()
            .contains("too much saved text")
    );
}
#[test]
fn saved_citation_requires_requested_scope_and_exact_unicode_passage() {
    let (mut owner, mut runs) = fixture();
    owner.text = "α 🐦 東京 saved citation".into();
    let source = saved_fixture(&owner.text);
    let evidence = runs[0].recall.as_mut().unwrap();
    evidence.owner = source.source.owner.clone();
    evidence.scope = vec![source.source.clone()];
    let passage = &mut evidence.passages[0].passage;
    passage.source = source.source.clone();
    passage.version = source.version.clone();
    passage.text_hash = eidetic::Hash::of(owner.text.as_bytes());
    passage.selector = PassageSelector { start: 2, end: 6 };
    passage.text = owner.text.chars().skip(2).take(4).collect();
    evidence.citations[0].passage = passage.id();
    let opened = resolve_source(&owner, &runs, "retained", 1).unwrap();
    assert_eq!(opened.selector, PassageSelector { start: 2, end: 6 });
    assert_eq!(
        opened
            .retained
            .text
            .chars()
            .skip(2)
            .take(4)
            .collect::<String>(),
        "🐦 東京"
    );
    runs[0].recall.as_mut().unwrap().scope.clear();
    assert!(
        resolve_source(&owner, &runs, "retained", 1)
            .unwrap_err()
            .contains("outside")
    );
}
#[test]
#[ignore = "requires the isolated R3 real resident and a public probe receipt path"]
fn admitted_research_route_refuses_wrong_owner() {
    let output = std::path::PathBuf::from(
        std::env::var_os("TURNSTONE_R3_PROBE_RECEIPT").expect("explicit probe receipt path"),
    );
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let mut client = AppBrokerClient::open_route(AppId::new("turnstone"), AppRouteId::new(wire::INFERENCE_ASK_ROUTE).unwrap()).await.unwrap();
            let opened = client.open_session().await.unwrap();
            let request = opened.descriptor.projections.first().unwrap().request.clone();
            let mut connection = Connection { client, request, published: None };
            let source = saved_fixture("Public R3 wrong-owner boundary probe.");
            let wrong_owner = "turnstone/r3-wrong-owner/research/v1";
            let publish = PublishCorpusRequestV1 { owner: wrong_owner.into(), sources: vec![source.clone()] };
            let publication_error = intent(&mut connection, PUBLISH_CORPUS_INTENT, serde_json::to_vec(&publish).unwrap()).await.unwrap_err();
            let ask = PublishedAskRequestV1 { answer_format: Default::default(), owner: wrong_owner.into(), ask: wire::AskRequestV1 { run: format!("{wrong_owner}/probe"), question: "This wrong-owner ask must never generate.".into() }, scope: vec![source.source.clone()] };
            let ask_error = intent(&mut connection, ASK_PUBLISHED_INTENT, serde_json::to_vec(&ask).unwrap()).await.unwrap_err();
            connection.client.close().await.unwrap();
            std::fs::write(output, serde_json::to_vec_pretty(&serde_json::json!({ "actual_admitted_route": true, "source_kind": "fleece", "wrong_owner_publication_refused": true, "wrong_owner_ask_refused": true, "publication_error": publication_error, "ask_error": ask_error })).unwrap()).unwrap();
        });
}
