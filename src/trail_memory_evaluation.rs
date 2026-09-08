// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Explicit, local-only evaluation harness for the trail recall settings.
//!
//! This is test-only code because it reads private browsing data and exists to
//! produce a receipt, not an application feature. The runner copies a supplied
//! session before opening Fjall, prints digests and aggregate metrics only, and
//! never writes the judgments or browsing records into the repository.

use super::*;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::Instant;

const TIMING_ROUNDS: usize = 11;

struct EvaluationCorpus {
    traces: Vec<BrowsingTrace>,
    /// The page bodies the session kept (W6c); empty for a corpus captured
    /// before text was stored.
    texts: PageTexts,
    documents: BTreeMap<String, RecallHit>,
    trace_count: usize,
    traversal_count: usize,
    titled_documents: usize,
    digest: String,
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| format!("create private copy: {error}"))?;
    for entry in fs::read_dir(source).map_err(|error| format!("read session: {error}"))? {
        let entry = entry.map_err(|error| format!("read session entry: {error}"))?;
        let kind = entry
            .file_type()
            .map_err(|error| format!("inspect session entry: {error}"))?;
        let target = destination.join(entry.file_name());
        if kind.is_symlink() {
            return Err("evaluation refuses session symlinks".to_string());
        }
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target)
                .map_err(|error| format!("copy private session file: {error}"))?;
        } else {
            return Err("evaluation found an unsupported session entry".to_string());
        }
    }
    Ok(())
}

fn load_evaluation_corpus(source: &Path) -> Result<EvaluationCorpus, String> {
    let source = fs::canonicalize(source).map_err(|error| format!("resolve session: {error}"))?;
    if !source.is_dir() || !memory_dir(&source).is_dir() || !source.join("graph.json").is_file() {
        return Err("evaluation source must be one Turnstone session directory".to_string());
    }

    let private = tempfile::tempdir().map_err(|error| format!("private copy root: {error}"))?;
    let copied_session = private.path().join("session");
    copy_tree(&source, &copied_session)?;

    let graph = crate::session::load_session_graph(&copied_session)
        .ok_or_else(|| "load copied session graph".to_string())?;
    let mut sources = graph
        .nodes()
        .filter_map(|(_, node)| RecallSource::new(node.url(), node.title.clone()))
        .collect::<Vec<_>>();

    let copied_bin = crate::recycle::bin_dir(&copied_session);
    if copied_bin.is_dir() {
        let mut bin =
            FjallStore::open(&copied_bin).map_err(|error| format!("open copied bin: {error}"))?;
        pollster::block_on(eidetic::bootstrap(&mut bin))
            .map_err(|error| format!("bootstrap copied bin: {error}"))?;
        let deleted = pollster::block_on(eidetic::list_deleted(&mut bin))
            .map_err(|error| format!("list copied bin: {error}"))?;
        sources.extend(
            deleted
                .into_iter()
                .filter_map(|record| RecallSource::new(record.url, record.title?)),
        );
    }
    let sources = canonical_sources(sources);

    let copied_memory = memory_dir(&copied_session);
    let mut store =
        FjallStore::open(&copied_memory).map_err(|error| format!("open copied memory: {error}"))?;
    pollster::block_on(eidetic::bootstrap(&mut store))
        .map_err(|error| format!("bootstrap copied memory: {error}"))?;
    pollster::block_on(bootstrap_browsing_schema(&mut store))
        .map_err(|error| format!("bootstrap copied browsing schema: {error}"))?;
    let memory = pollster::block_on(BrowsingMemory::load(&mut store, SEGMENT_SIZE))
        .map_err(|error| format!("load copied browsing memory: {error}"))?;
    let trace_count = memory.traces().count();
    let traversal_count = memory.traces().map(|trace| trace.events.len()).sum();
    let traces = traces_with_titles(&memory, &sources);
    let texts = stored_page_texts(&store);
    let documents = recall_documents(&page_table_of(&traces, &texts));
    let titled_documents = documents
        .values()
        .filter(|hit| hit.title.is_some())
        .count();
    let canonical = serde_json::to_vec(&traces)
        .map_err(|error| format!("serialize corpus receipt: {error}"))?;
    let digest = blake3::hash(&canonical).to_hex().to_string();

    Ok(EvaluationCorpus {
        traces,
        texts,
        documents,
        trace_count,
        traversal_count,
        titled_documents,
        digest,
    })
}

/// A stable, non-identifying label for one page of a private corpus. The
/// receipt needs to show that an order changed, not which pages were visited.
fn page_label(url: &str) -> String {
    blake3::hash(url.as_bytes()).to_hex()[..8].to_string()
}

/// W6b's receipt against a real captured session: what a typed prefix recalls
/// with the behavioural lane off and on. Needs no manifest — the corpus's own
/// frecency table supplies the prefixes (the host label of each top page), so
/// nothing is hand-picked. Prints labels and ranks, never URLs.
#[test]
#[ignore = "requires an explicit private Turnstone session"]
fn captured_trail_frecency_receipt() {
    let session = std::env::var_os("TURNSTONE_RECALL_EVAL_SESSION")
        .map(PathBuf::from)
        .expect("TURNSTONE_RECALL_EVAL_SESSION is required");
    let corpus = load_evaluation_corpus(&session).unwrap();
    let index_root = tempfile::tempdir().expect("frecency receipt index root");
    let index = RecallIndex::mint(index_root.path(), &corpus.traces, &corpus.texts).unwrap();
    let behavioural = eidetic::browsing::frecency::ranked(&index.frecency);
    println!(
        "corpus digest={} source=captured pages={} scored_pages={} frecency_us={}",
        corpus.digest,
        index.receipt.pages,
        index.frecency.values().filter(|s| **s > 0.0).count(),
        index.receipt.frecency.as_micros(),
    );

    // A typed prefix, taken from the host of each of the top behavioural pages:
    // the omnibar case W6b exists for.
    for url in behavioural.iter().take(3) {
        let host = url
            .split("://")
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .unwrap_or(url);
        let prefix: String = host.chars().take(4).collect();
        println!(
            "prefix chars={} expects={}",
            prefix.chars().count(),
            page_label(url),
        );
        for (lane, config) in [
            (
                "without_frecency",
                RecallConfig::default().with_frecency_weight(0.0),
            ),
            ("with_frecency", RecallConfig::default()),
        ] {
            let hits = index.search(&prefix, 3, config).unwrap();
            let top: Vec<String> = hits
                .iter()
                .map(|hit| {
                    let rank = behavioural.iter().position(|url| *url == hit.url);
                    match rank {
                        Some(rank) => format!("{}@f{rank}", page_label(&hit.url)),
                        None => format!("{}@f-", page_label(&hit.url)),
                    }
                })
                .collect();
            println!("  lane={lane} top={top:?}");
        }
    }
}

/// W6a's mint cost against a real captured session: what a recall after one
/// navigation actually pays. Needs no manifest — the corpus alone answers it.
/// Copies the session first, like every runner here, and prints counts only.
#[test]
#[ignore = "requires an explicit private Turnstone session"]
fn captured_trail_mint_receipt() {
    let session = std::env::var_os("TURNSTONE_RECALL_EVAL_SESSION")
        .map(PathBuf::from)
        .expect("TURNSTONE_RECALL_EVAL_SESSION is required");
    let corpus = load_evaluation_corpus(&session).unwrap();
    println!(
        "corpus digest={} source=captured traces={} traversals={} documents={} titled_documents={}",
        corpus.digest,
        corpus.trace_count,
        corpus.traversal_count,
        corpus.documents.len(),
        corpus.titled_documents,
    );
    let index_root = tempfile::tempdir().expect("mint receipt index root");
    let index = RecallIndex::mint(index_root.path(), &corpus.traces, &corpus.texts).unwrap();
    println!("captured {:?}", index.receipt);
}

// --- W6e: the Firefox history corpus lane -----------------------------------
//
// A second corpus for the same receipts W6a-W6d took on a captured session:
// two years of real browsing, exported by `mere/scripts/firefox_history_export.py`
// and lowered through `mere_import::history_to_traces`. The export lives outside
// the repo and is named by environment; nothing here prints an address, a title,
// or a term.

/// Firefox's own `frecency` per place, keyed by the BLAKE3 hex of the place URL
/// — the same digest the exporter's default writes.
type FirefoxFrecency = HashMap<String, f64>;

fn firefox_env(name: &str) -> Result<PathBuf, String> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{name} is required"))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|error| format!("read export: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse export: {error}"))
}

/// Fractional ranks, ties averaged — Spearman's input.
fn fractional_ranks(values: &[f64]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|left, right| values[*left].total_cmp(&values[*right]));
    let mut ranks = vec![0.0; values.len()];
    let mut start = 0;
    while start < order.len() {
        let mut end = start + 1;
        while end < order.len() && values[order[end]] == values[order[start]] {
            end += 1;
        }
        // Average rank over the tied block, 1-based.
        let shared = (start + end + 1) as f64 / 2.0;
        for slot in &order[start..end] {
            ranks[*slot] = shared;
        }
        start = end;
    }
    ranks
}

/// Spearman's rho: Pearson correlation of the fractional ranks.
fn spearman(left: &[f64], right: &[f64]) -> f64 {
    if left.len() < 2 || left.len() != right.len() {
        return f64::NAN;
    }
    let (a, b) = (fractional_ranks(left), fractional_ranks(right));
    let n = a.len() as f64;
    let mean_a = a.iter().sum::<f64>() / n;
    let mean_b = b.iter().sum::<f64>() / n;
    let mut covariance = 0.0;
    let mut var_a = 0.0;
    let mut var_b = 0.0;
    for (x, y) in a.iter().zip(&b) {
        covariance += (x - mean_a) * (y - mean_b);
        var_a += (x - mean_a).powi(2);
        var_b += (y - mean_b).powi(2);
    }
    if var_a == 0.0 || var_b == 0.0 {
        return f64::NAN;
    }
    covariance / (var_a * var_b).sqrt()
}

/// The best Firefox score among the addresses that collapsed onto each page.
/// Firefox scores a place; a page record may own several, and the one a user
/// would have seen ranked is the highest.
fn firefox_by_page(table: &PageTable, firefox: &FirefoxFrecency) -> BTreeMap<PageFingerprint, f64> {
    let mut best: BTreeMap<PageFingerprint, f64> = BTreeMap::new();
    for (raw, fingerprint) in &table.by_address {
        let Some(score) = firefox.get(blake3::hash(raw.as_bytes()).to_hex().as_str()) else {
            continue;
        };
        let slot = best.entry(*fingerprint).or_insert(f64::MIN);
        *slot = slot.max(*score);
    }
    best
}

/// Rank correlation and top-100 overlap of our fold against Firefox's, over the
/// pages both scored.
fn compare_frecency(
    label: &str,
    traces: &[BrowsingTrace],
    table: &PageTable,
    firefox: &BTreeMap<PageFingerprint, f64>,
    now_ms: u64,
    config: &FrecencyConfig,
) {
    let ours = frecency_by_page(traces, now_ms, config, table);
    let mut mine = Vec::new();
    let mut theirs = Vec::new();
    for (fingerprint, score) in &ours {
        if let Some(other) = firefox.get(fingerprint) {
            mine.push(*score);
            theirs.push(*other);
        }
    }
    let top = |scores: &BTreeMap<PageFingerprint, f64>| -> BTreeSet<PageFingerprint> {
        let mut ranked: Vec<(&PageFingerprint, f64)> =
            scores.iter().map(|(key, s)| (key, *s)).collect();
        ranked.sort_by(|l, r| r.1.total_cmp(&l.1).then_with(|| l.0.cmp(r.0)));
        ranked.into_iter().take(100).map(|(key, _)| *key).collect()
    };
    let overlap = top(&ours).intersection(&top(firefox)).count();
    println!(
        "  frecency lane={label} shared_pages={} spearman={:.4} top100_overlap={overlap}",
        mine.len(),
        spearman(&mine, &theirs),
    );
}

fn percentile(sorted: &[u64], percent: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = (sorted.len() * percent / 100).min(sorted.len() - 1);
    sorted[index]
}

fn run_firefox_history_receipt() -> Result<(), String> {
    let export = firefox_env("TURNSTONE_FIREFOX_EXPORT")?;
    let sidecar = firefox_env("TURNSTONE_FIREFOX_FRECENCY")?;
    let items: Vec<mere_import::ImportedHistoryVisitItem> = read_json(&export)?;
    let firefox: FirefoxFrecency = read_json(&sidecar)?;
    let built = Instant::now();
    let traces = mere_import::history_to_traces(&items, "firefox");
    let build_ms = built.elapsed().as_millis();

    // A fresh store, in the same scratch root the export lives under. Written
    // and read back, so the receipt covers the storage path a real import takes
    // rather than an in-memory corpus.
    let scratch = std::env::var_os("TURNSTONE_FIREFOX_TMP")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    fs::create_dir_all(&scratch).map_err(|error| format!("scratch root: {error}"))?;
    let root = tempfile::TempDir::new_in(&scratch).map_err(|error| format!("temp: {error}"))?;
    let memory_root = root.path().join("memory");
    let mut store =
        FjallStore::open(&memory_root).map_err(|error| format!("open store: {error}"))?;
    pollster::block_on(eidetic::bootstrap(&mut store))
        .map_err(|error| format!("bootstrap: {error}"))?;
    pollster::block_on(bootstrap_browsing_schema(&mut store))
        .map_err(|error| format!("bootstrap browsing schema: {error}"))?;
    let wrote = Instant::now();
    for trace in &traces {
        pollster::block_on(eidetic::browsing::save_trace(&mut store, trace, now_ms()))
            .map_err(|error| format!("save trace: {error}"))?;
    }
    let write_ms = wrote.elapsed().as_millis();
    let read = Instant::now();
    let memory = pollster::block_on(BrowsingMemory::load(&mut store, SEGMENT_SIZE))
        .map_err(|error| format!("load memory: {error}"))?;
    let stored: Vec<BrowsingTrace> = memory.traces().cloned().collect();
    let read_ms = read.elapsed().as_millis();
    let texts = stored_page_texts(&store);
    println!(
        "corpus source=firefox_export items={} traces={} stored_traces={} lower_ms={build_ms} store_write_ms={write_ms} store_read_ms={read_ms}",
        items.len(),
        traces.len(),
        stored.len(),
    );

    // (b) what the mapping produced, and what the interaction timer gave dwell.
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut dwells: Vec<u64> = Vec::new();
    for event in stored.iter().flat_map(|trace| trace.events.iter()) {
        *kinds.entry(format!("{:?}", event.transition)).or_insert(0) += 1;
        if let Some(dwell) = event.dwell_ms {
            dwells.push(dwell);
        }
    }
    for (kind, count) in &kinds {
        println!("  transition {kind} {count}");
    }
    dwells.sort_unstable();
    let threshold = FrecencyConfig::default().dwell_threshold_ms;
    println!(
        "  dwell with_dwell={} median_ms={} p90_ms={} over_threshold={} threshold_ms={threshold}",
        dwells.len(),
        percentile(&dwells, 50),
        percentile(&dwells, 90),
        dwells.iter().filter(|d| **d >= threshold).count(),
    );

    // (a) the mint receipt: the lexical and behavioural lanes.
    let index_root =
        tempfile::TempDir::new_in(&scratch).map_err(|error| format!("index temp: {error}"))?;
    let index = RecallIndex::mint(index_root.path(), &stored, &texts)?;
    println!("  mint {:?}", index.receipt);

    // (c) our behavioural fold against Firefox's own frecency.
    let table = page_table_of(&stored, &texts);
    let their_scores = firefox_by_page(&table, &firefox);
    let now = now_ms();
    let no_bonus = FrecencyConfig {
        dwell_bonus: 0.0,
        ..FrecencyConfig::default()
    };
    println!("  firefox_scored_pages={}", their_scores.len());
    compare_frecency(
        "dwell_bonus_on",
        &stored,
        &table,
        &their_scores,
        now,
        &FrecencyConfig::default(),
    );
    compare_frecency(
        "dwell_bonus_off",
        &stored,
        &table,
        &their_scores,
        now,
        &no_bonus,
    );

    // (d) how far the canonical key collapsed the address space.
    let mut per_record: BTreeMap<PageFingerprint, usize> = BTreeMap::new();
    for fingerprint in table.by_address.values() {
        *per_record.entry(*fingerprint).or_insert(0) += 1;
    }
    let canonical: BTreeSet<&String> = table
        .records
        .values()
        .flat_map(|record| record.urls.iter())
        .collect();
    let mut sizes: Vec<usize> = per_record.values().copied().collect();
    sizes.sort_unstable_by(|left, right| right.cmp(left));
    sizes.truncate(5);
    println!(
        "  collapse raw_addresses={} canonical_addresses={} pages={} top5_sizes={sizes:?}",
        table.by_address.len(),
        canonical.len(),
        table.records.len(),
    );

    // (e) typed-prefix queries built from the corpus itself: the first two
    // characters of the host of each of the three most-visited pages. The
    // characters are never printed, only the lengths, labels and orders.
    let mut busiest: Vec<&eidetic::PageRecord> = table.records.values().collect();
    busiest.sort_by(|left, right| {
        right
            .visits
            .cmp(&left.visits)
            .then_with(|| left.last_url.cmp(&right.last_url))
    });
    for record in busiest.iter().take(3) {
        let host = record
            .last_url
            .split("://")
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .unwrap_or(record.last_url.as_str());
        let prefix: String = host.chars().take(2).collect();
        let expected = index.frecency_urls(&prefix, 1).first().cloned();
        for (lane, config) in [
            (
                "without_frecency",
                RecallConfig::default().with_frecency_weight(0.0),
            ),
            ("with_frecency", RecallConfig::default()),
        ] {
            let mut timings = Vec::new();
            let mut first = None;
            for _ in 0..TIMING_ROUNDS {
                let started = Instant::now();
                let hits = index.search(&prefix, 5, config)?;
                timings.push(started.elapsed().as_micros());
                first = hits.first().map(|hit| hit.url.clone());
            }
            timings.sort_unstable();
            println!(
                "  query prefix_chars={} page={} lane={lane} median_us={} top_is_top_frecency={} hit={}",
                prefix.chars().count(),
                page_label(&record.last_url),
                timings[timings.len() / 2],
                first.is_some() && first == expected,
                first.as_deref().map_or("none".to_string(), page_label),
            );
        }
    }
    Ok(())
}

/// W6e's receipt: two years of real Firefox history, lowered through the
/// import mapping and minted. Reads an export named by environment, writes only
/// under the scratch root, and prints counts, durations, correlations and
/// BLAKE3 labels — never an address, a title, or a term.
#[test]
#[ignore = "requires an exported Firefox history corpus"]
fn firefox_history_corpus_receipt() {
    run_firefox_history_receipt().unwrap();
}
