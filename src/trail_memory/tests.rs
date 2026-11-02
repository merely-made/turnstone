// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::sync::Arc;

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir()
        .join("turnstone-trail-memory-tests")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// One synthetic page's url and title. Deterministic, so a test can total
/// the corpus bytes without asking the code under test.
fn synthetic_page(index: usize) -> (String, String) {
    (
        format!("https://page{index}.example/trail/{index}"),
        format!("Synthetic Page {index}"),
    )
}

/// `events` traversals cycling over `pages` distinct URLs, chained and
/// chunked into `SEGMENT_SIZE` segments exactly as the actor stores them.
fn synthetic_traces(events: usize, pages: usize) -> Vec<BrowsingTrace> {
    let mut built = Vec::new();
    let mut segment = Vec::new();
    let mut previous: Option<PageRef> = None;
    for index in 0..events {
        let (url, title) = synthetic_page(index % pages);
        let to = PageRef {
            url,
            title: Some(title),
        };
        segment.push(TraceEvent {
            from: previous.replace(to.clone()),
            to,
            transition: TraceTransition::LinkClick,
            at_ms: index as u64 + 1,
            dwell_ms: None,
            candidates: Vec::new(),
        });
        if segment.len() == SEGMENT_SIZE {
            built.push(BrowsingTrace::from_events(
                "p",
                std::mem::take(&mut segment),
            ));
        }
    }
    if !segment.is_empty() {
        built.push(BrowsingTrace::from_events("p", segment));
    }
    built
}

fn expected_corpus_bytes(events: usize, pages: usize) -> usize {
    (0..events)
        .map(|index| {
            let (url, title) = synthetic_page(index % pages);
            url.len() + title.len()
        })
        .sum()
}

/// The W6a receipt: counts are exact against a corpus the test built, and
/// both mint lanes are actually timed.
#[test]
fn mint_receipt_counts_the_corpus_exactly() {
    const EVENTS: usize = 1_000;
    const PAGES: usize = 300;
    let traces = synthetic_traces(EVENTS, PAGES);

    let index = RecallIndex::mint(&temp_dir(), &traces, &PageTexts::default()).unwrap();
    let receipt = index.receipt;
    assert_eq!(receipt.traces, EVENTS.div_ceil(SEGMENT_SIZE));
    assert_eq!(receipt.events, EVENTS);
    assert_eq!(receipt.pages, PAGES, "distinct destination URLs");
    assert_eq!(receipt.corpus_bytes, expected_corpus_bytes(EVENTS, PAGES));
    assert!(
        receipt.lexical > Duration::ZERO,
        "the lexical lane is timed"
    );
    assert!(
        receipt.frecency > Duration::ZERO,
        "the behavioural lane is timed"
    );
    assert!(receipt.total >= receipt.lexical, "total covers both lanes");
}

/// W6d's done-condition: two addresses for one page are one page record,
/// one index document, one hit, and one summed frecency.
#[test]
fn two_urls_for_one_page_recall_once_with_one_frecency() {
    let now = now_ms();
    let addresses = [
        "https://gazette.test/morning?utm_source=newsletter&utm_medium=email",
        "https://gazette.test/morning#lede",
        "https://gazette.test/morning",
    ];
    let events: Vec<TraceEvent> = addresses
        .iter()
        .enumerate()
        .map(|(nth, url)| TraceEvent {
            from: None,
            to: PageRef {
                url: (*url).to_string(),
                title: Some("The Morning Paper".to_string()),
            },
            transition: TraceTransition::UrlTyped,
            at_ms: now - nth as u64 * 1_000,
            dwell_ms: None,
            candidates: Vec::new(),
        })
        .collect();
    let traces = vec![BrowsingTrace::from_events("p", events)];

    let index = RecallIndex::mint(&temp_dir(), &traces, &PageTexts::default()).unwrap();
    assert_eq!(index.receipt.events, 3);
    assert_eq!(index.receipt.pages, 1, "one page behind three addresses");
    assert_eq!(index.receipt.collapsed_urls, 2);
    assert!(
        index.receipt.page_table > Duration::ZERO,
        "the fold is timed"
    );
    assert_eq!(
        index.lexical.doc_count().unwrap(),
        1,
        "one index document per page, not per event"
    );

    let hits = index
        .search("gazette morning", 5, RecallConfig::default())
        .unwrap();
    assert_eq!(hits.len(), 1, "one page, one row");
    assert_eq!(
        hits[0].url, addresses[0],
        "the row opens the latest address"
    );

    // One combined score, not three split ones — and it is the sum.
    assert_eq!(index.frecency.len(), 1);
    let combined = index.frecency[addresses[0]];
    let single = RecallIndex::mint(
        &temp_dir(),
        &traces[..1]
            .iter()
            .map(|trace| BrowsingTrace::from_events("p", trace.events[..1].to_vec()))
            .collect::<Vec<_>>(),
        &PageTexts::default(),
    )
    .unwrap()
    .frecency[addresses[0]];
    assert!(
        (combined - 3.0 * single).abs() < single * 0.01,
        "three typed visits to one page sum: {combined} vs 3 x {single}"
    );
}

/// L1's retention boundary: an ordinary body is absent while address and
/// title remain recallable from the same traversal.
#[test]
fn ordinary_page_body_is_not_retained_but_address_and_title_recall() {
    const BODY: &str = "A kestrel hovers on a fixed point of air, head still while \
                            the wings work, and stoops only when the vole below commits \
                            to a run across the open verge.";
    let url = "https://field.test/notes/7";
    let dir = temp_dir();
    let store = FjallStore::open(&dir).unwrap();
    // Exercise the actor's boundary directly, including stale queued work.
    keep_page_text(&store, url, BODY);
    let texts = stored_page_texts(&store);
    assert!(
        texts.is_empty(),
        "ordinary fetch text never enters PageText"
    );

    let traces = vec![BrowsingTrace::from_events(
        "p",
        vec![TraceEvent {
            from: None,
            to: PageRef {
                url: url.to_string(),
                title: Some("Notes".to_string()),
            },
            transition: TraceTransition::UrlTyped,
            at_ms: now_ms(),
            dwell_ms: None,
            candidates: Vec::new(),
        }],
    )];

    let index = RecallIndex::mint(&temp_dir(), &traces, &texts).unwrap();
    assert_eq!(index.receipt.pages_with_text, 0);

    // "kestrel" is in neither the URL nor the title.
    assert!(!url.contains("kestrel"));
    let body_hits = index
        .search(
            "kestrel",
            5,
            RecallConfig::default().with_frecency_weight(0.0),
        )
        .unwrap();
    assert!(body_hits.is_empty(), "body-only terms are not searchable");

    let title_hits = index
        .search(
            "Notes",
            5,
            RecallConfig::default().with_frecency_weight(0.0),
        )
        .unwrap();
    assert_eq!(title_hits.len(), 1, "title recall remains available");
    assert_eq!(title_hits[0].url, url, "address remains attached to recall");
    let address_hits = index
        .search(
            "field",
            5,
            RecallConfig::default().with_frecency_weight(0.0),
        )
        .unwrap();
    assert_eq!(address_hits.len(), 1, "address recall remains available");
    assert_eq!(address_hits[0].url, url);
}

#[test]
fn legacy_page_text_survives_ordinary_fetch_without_rewrite() {
    let url = "https://field.test/legacy";
    let dir = temp_dir();
    let store = FjallStore::open(&dir).unwrap();
    let legacy = "legacy amberstone body";
    pollster::block_on(PageTextStore::new(&store).put(url, legacy, 17)).unwrap();
    let before = stored_page_texts(&store);

    keep_page_text(&store, url, "new automatic body must be ignored");

    assert_eq!(stored_page_texts(&store), before);
    assert_eq!(stored_page_texts(&store).get(url), Some(legacy));
}

/// The W6a cost curve. A measurement run, not a correctness gate: build in
/// release and pass `--ignored --nocapture`.
#[test]
#[ignore = "measurement run: cargo test --release ... -- --ignored --nocapture"]
fn mint_receipt_scale_ladder() {
    for events in [1_000usize, 10_000, 100_000] {
        // Hold the revisit ratio constant so the curve compares like sizes.
        let pages = events / 3;
        let traces = synthetic_traces(events, pages);
        let index = RecallIndex::mint(&temp_dir(), &traces, &PageTexts::default()).unwrap();
        println!("synthetic pages={pages} {:?}", index.receipt);
    }
}

/// Two records chain `from`, Flush persists, Release closes the store,
/// and a fresh load reads one trace whose events carry the chain — the
/// round trip W2's re-mint depends on.
#[test]
fn records_chain_and_round_trip() {
    let dir = temp_dir();
    let wake: Wake = Arc::new(|| {});
    let (handle, _rx) = spawn_trail(wake, dir.clone());
    handle.command(TrailCommand::Record {
        owner: "p".into(),
        url: "https://a.example/".into(),
        transition: TraceTransition::UrlTyped,
        at_ms: 1,
    });
    handle.command(TrailCommand::Record {
        owner: "p".into(),
        url: "https://b.example/".into(),
        transition: TraceTransition::Back,
        at_ms: 2,
    });
    handle.command(TrailCommand::Flush);
    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(0);
    handle.command(TrailCommand::Release(ack_tx));
    ack_rx.recv().unwrap();

    let mut store = FjallStore::open(&dir).unwrap();
    let memory = pollster::block_on(BrowsingMemory::load(&mut store, SEGMENT_SIZE)).unwrap();
    let traces: Vec<_> = memory.traces().collect();
    assert_eq!(traces.len(), 1, "one flushed segment");
    let events = &traces[0].events;
    assert_eq!(events.len(), 2);
    assert!(events[0].from.is_none(), "first event is an origin");
    assert_eq!(events[0].to.url, "https://a.example/");
    assert_eq!(
        events[1].from.as_ref().map(|p| p.url.as_str()),
        Some("https://a.example/"),
        "the chain: last destination becomes the next origin"
    );
    assert_eq!(events[1].transition, TraceTransition::Back);
}

#[test]
fn source_capture_refuses_incomplete_acquisition_context() {
    let dir = temp_dir();
    let mut store = FjallStore::open(&dir).unwrap();
    let complete = crate::content::FetchedDocument {
        bytes: b"<p>kestrel</p>".to_vec(),
        content_type: Some("text/html".into()),
        body: "<p>kestrel</p>".into(),
        effective_url: Some("https://example.test/kestrel".into()),
        acquired_at_ms: 9,
    };
    let missing_url = crate::content::FetchedDocument {
        effective_url: None,
        ..complete.clone()
    };
    assert!(
        capture_source_document(&mut store, &missing_url)
            .unwrap_err()
            .contains("final response URL")
    );
    let missing_time = crate::content::FetchedDocument {
        acquired_at_ms: 0,
        ..complete
    };
    assert!(
        capture_source_document(&mut store, &missing_time)
            .unwrap_err()
            .contains("acquisition time")
    );
}

#[test]
fn explicit_source_capture_keeps_exact_bytes_and_reopens_annotation() {
    let dir = temp_dir();
    let exact = b"<html><body><main><p>source-only kestrel</p></main></body></html>";
    let document = crate::content::FetchedDocument {
        bytes: exact.to_vec(),
        content_type: Some("text/html; charset=utf-8".into()),
        body: String::from_utf8(exact.to_vec()).unwrap(),
        effective_url: Some("https://final.example/kestrel".into()),
        acquired_at_ms: 1_700_000_000_000,
    };
    let mut store = FjallStore::open(&dir).unwrap();
    let stored = capture_source_document(&mut store, &document).unwrap();
    assert_ne!(stored.raw_manifest, stored.annotation_manifest);
    drop(store);

    let mut reopened = FjallStore::open(&dir).unwrap();
    let manifests = pollster::block_on(
        eidetic::typed::list_typed::<fleece::FleeceAnnotationRecord>(&mut reopened),
    )
    .unwrap();
    assert_eq!(manifests.len(), 1);
    let record = pollster::block_on(fleece::load_fleece_annotation(
        &mut reopened,
        manifests[0].id,
    ))
    .unwrap()
    .unwrap();
    let evidence = record.extraction.capture.evidence.as_ref().unwrap();
    let raw_manifest_id = evidence.raw_manifest.expect("raw manifest");
    assert_eq!(evidence.final_source, "https://final.example/kestrel");
    assert_eq!(raw_manifest_id.to_string(), stored.raw_manifest);
    assert_eq!(evidence.capture_hash, eidetic::Hash::of(exact));
    assert!(evidence.has_authoritative_acquisition_context());
    let raw_manifest = pollster::block_on(eidetic::manifest::load_manifest(
        &mut reopened,
        raw_manifest_id,
    ))
    .unwrap()
    .unwrap();
    let raw = pollster::block_on(eidetic::manifest::resolve_blob(
        &mut reopened,
        &mut eidetic::NoFetcher,
        &raw_manifest,
    ))
    .unwrap();
    assert_eq!(raw, exact, "the raw response bytes are not Fleece text");
    assert!(
        record
            .extraction
            .canonical_text_record
            .canonical_text
            .contains("kestrel")
    );
}

/// A segment that fills flushes without an explicit Flush command.
#[test]
fn full_segment_flushes_itself() {
    let dir = temp_dir();
    let wake: Wake = Arc::new(|| {});
    let (handle, _rx) = spawn_trail(wake, dir.clone());
    for i in 0..SEGMENT_SIZE {
        handle.command(TrailCommand::Record {
            owner: "p".into(),
            url: format!("https://page{i}.example/"),
            transition: TraceTransition::LinkClick,
            at_ms: i as u64,
        });
    }
    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(0);
    handle.command(TrailCommand::Release(ack_tx));
    ack_rx.recv().unwrap();

    let mut store = FjallStore::open(&dir).unwrap();
    let memory = pollster::block_on(BrowsingMemory::load(&mut store, SEGMENT_SIZE)).unwrap();
    let total: usize = memory.traces().map(|t| t.events.len()).sum();
    assert_eq!(total, SEGMENT_SIZE);
}

/// The event mapping: the four navigation events map, the rest do not.
#[test]
fn navigation_mapping() {
    use crate::observe::AppEvent;
    assert_eq!(
        navigation(&AppEvent::AddressOpened("https://x/".into())),
        Some(("https://x/".into(), TraceTransition::UrlTyped))
    );
    assert_eq!(
        navigation(&AppEvent::NavigatedBack("https://x/".into())),
        Some(("https://x/".into(), TraceTransition::Back))
    );
    assert_eq!(
        navigation(&AppEvent::NavigatedForward("https://x/".into())),
        Some(("https://x/".into(), TraceTransition::Forward))
    );
    assert_eq!(
        navigation(&AppEvent::Reloaded("https://x/".into())),
        Some(("https://x/".into(), TraceTransition::Reload))
    );
    assert_eq!(navigation(&AppEvent::WindowOpened), None);
}

/// Titles remain graph/bin authority: the actor overlays them into its
/// derived index and re-mints when that projection changes, without a new
/// traversal.
#[test]
fn recall_overlays_current_titles_and_remints_on_change() {
    let dir = temp_dir();
    let wake: Wake = Arc::new(|| {});
    let (handle, rx) = spawn_trail(wake, dir);
    let url = "https://example.test/page/42";
    handle.command(TrailCommand::Record {
        owner: "p".into(),
        url: url.into(),
        transition: TraceTransition::UrlTyped,
        at_ms: 1,
    });

    for (query, title) in [
        ("field notes", "Field Notes"),
        ("harmony map", "Harmony Map"),
    ] {
        handle.command(TrailCommand::Recall {
            query: query.into(),
            limit: 5,
            sources: vec![RecallSource::new(url, title).unwrap()],
            config: RecallConfig::default(),
        });
        let update = rx.recv().unwrap();
        let Update::RecallHits {
            query: answered,
            hits,
        } = update
        else {
            panic!("title recall must answer with hits");
        };
        assert_eq!(answered, query);
        assert_eq!(hits.len(), 1, "{query:?} should match the current title");
        assert_eq!(hits[0].url, url);
        assert_eq!(hits[0].title.as_deref(), Some(title));
    }

    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(0);
    handle.command(TrailCommand::Release(ack_tx));
    ack_rx.recv().unwrap();
}

/// W6b's done-condition: a typed prefix recalls the page the user keeps
/// going back to, not the page whose title repeats the query. The decoy
/// wins on BM25 (three occurrences in a short title) and loses on
/// behaviour (one link click, two hundred days ago).
#[test]
fn typed_prefix_recall_follows_frecency_over_title_overlap() {
    const DAY_MS: u64 = 24 * 60 * 60 * 1_000;
    let dir = temp_dir();
    let wake: Wake = Arc::new(|| {});
    let (handle, rx) = spawn_trail(wake, dir);
    let target = "https://gazette.test/";
    let decoy = "https://elsewhere.test/gazette/gazette-gazette";
    let now = now_ms();

    handle.command(TrailCommand::Record {
        owner: "p".into(),
        url: decoy.into(),
        transition: TraceTransition::LinkClick,
        at_ms: now - 200 * DAY_MS,
    });
    for days_ago in 0..6 {
        handle.command(TrailCommand::Record {
            owner: "p".into(),
            url: target.into(),
            transition: TraceTransition::UrlTyped,
            at_ms: now - days_ago * DAY_MS,
        });
    }
    let sources = vec![
        RecallSource::new(target, "The Morning Paper").unwrap(),
        RecallSource::new(decoy, "Gazette Gazette Gazette").unwrap(),
    ];

    let recall = |config| {
        handle.command(TrailCommand::Recall {
            query: "gazette".into(),
            limit: 5,
            sources: sources.clone(),
            config,
        });
        let Update::RecallHits { hits, .. } = rx.recv().unwrap() else {
            panic!("recall must answer with hits");
        };
        hits
    };

    let lexical = recall(RecallConfig::default().with_frecency_weight(0.0));
    assert_eq!(
        lexical[0].url, decoy,
        "BM25 alone follows the repeated title term"
    );

    let behavioural = recall(RecallConfig::default());
    assert_eq!(
        behavioural[0].url, target,
        "frecency lifts the page the prefix was typed into six times"
    );

    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(0);
    handle.command(TrailCommand::Release(ack_tx));
    ack_rx.recv().unwrap();
}
