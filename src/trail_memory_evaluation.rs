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

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::Path;
use std::time::Instant;

use serde::Deserialize;

const EVALUATION_SCHEMA: &str = "turnstone.recall-eval/v1";
const MIN_CASES_PER_KIND_PER_SPLIT: usize = 5;
const MIN_DISTINCT_DOCUMENTS: usize = MIN_CASES_PER_KIND_PER_SPLIT * 4;
const TIMING_ROUNDS: usize = 11;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationManifest {
    schema: String,
    ranking_k: usize,
    candidate_orders: Vec<u8>,
    candidate_weights: Vec<f32>,
    budgets: EvaluationBudgets,
    cases: Vec<EvaluationCase>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationBudgets {
    max_dense_vector_bytes: usize,
    max_p95_query_us: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationCase {
    split: EvaluationSplit,
    kind: EvaluationKind,
    query: String,
    relevant_urls: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum EvaluationSplit {
    Train,
    HeldOut,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum EvaluationKind {
    Phrase,
    Control,
}

struct EvaluationCorpus {
    traces: Vec<BrowsingTrace>,
    /// The page bodies the session kept (W6c); empty for a corpus captured
    /// before text was stored.
    texts: PageTexts,
    documents: BTreeMap<String, RecallDocument>,
    trace_count: usize,
    traversal_count: usize,
    titled_documents: usize,
    digest: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct RankingMetrics {
    cases: usize,
    deterministic_at_one: usize,
    unique_at_one: usize,
    recall_at_k_sum: f64,
    reciprocal_rank_sum: f64,
}

impl RankingMetrics {
    fn recall_at_k(self) -> f64 {
        if self.cases == 0 {
            0.0
        } else {
            self.recall_at_k_sum / self.cases as f64
        }
    }

    fn mean_reciprocal_rank(self) -> f64 {
        if self.cases == 0 {
            0.0
        } else {
            self.reciprocal_rank_sum / self.cases as f64
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct MetricsByKind {
    all: RankingMetrics,
    phrase: RankingMetrics,
    control: RankingMetrics,
}

#[derive(Clone, Debug)]
struct ScoredUrl {
    url: String,
    score: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CandidateConfig {
    order: u8,
    weight: f32,
}

#[derive(Clone, Copy, Debug)]
struct CandidateScore {
    config: CandidateConfig,
    metrics: MetricsByKind,
    within_training_budgets: bool,
    preserves_training_controls: bool,
}

struct BuiltOrder {
    order: u8,
    index: RecallIndex,
    build_us: u128,
    dense_vector_bytes: usize,
    vector_json_bytes: usize,
    training_p95_query_us: u128,
    _directory: tempfile::TempDir,
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
        .filter(|document| document.hit.title.is_some())
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

fn normalized_query(query: &str) -> String {
    query
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn validate_manifest(
    manifest: &EvaluationManifest,
    documents: &BTreeMap<String, RecallDocument>,
    minimum_per_kind: usize,
) -> Result<(), String> {
    if manifest.schema != EVALUATION_SCHEMA {
        return Err(format!("evaluation schema must be {EVALUATION_SCHEMA:?}"));
    }
    if manifest.ranking_k < 3 || manifest.ranking_k > documents.len() {
        return Err("ranking_k must be between 3 and the corpus size".to_string());
    }
    if manifest.budgets.max_dense_vector_bytes == 0 || manifest.budgets.max_p95_query_us == 0 {
        return Err("evaluation budgets must both be positive".to_string());
    }

    let orders = manifest
        .candidate_orders
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if orders.len() != manifest.candidate_orders.len()
        || orders.is_empty()
        || orders.iter().any(|order| !(1..=3).contains(order))
    {
        return Err("candidate_orders must be unique values from 1 through 3".to_string());
    }
    let mut weights = manifest.candidate_weights.clone();
    weights.sort_by(f32::total_cmp);
    let original_weight_count = weights.len();
    weights.dedup_by(|left, right| left.to_bits() == right.to_bits());
    if weights.is_empty()
        || weights.len() != original_weight_count
        || weights
            .iter()
            .any(|weight| !weight.is_finite() || *weight <= 0.0 || *weight > 4.0)
    {
        return Err("candidate_weights must be unique finite values in (0, 4]".to_string());
    }

    let mut queries = HashSet::new();
    let mut train_targets = HashSet::new();
    let mut held_out_targets = HashSet::new();
    let mut distinct = [
        HashSet::new(),
        HashSet::new(),
        HashSet::new(),
        HashSet::new(),
    ];
    for case in &manifest.cases {
        let query = normalized_query(&case.query);
        let query_tokens = query.split_whitespace().count();
        if query.is_empty() || !queries.insert(query) {
            return Err(
                "queries must be non-empty and unique after lexical normalization".to_string(),
            );
        }
        if case.kind == EvaluationKind::Phrase && query_tokens < 2 {
            return Err("phrase queries must contain at least two lexical tokens".to_string());
        }
        if case.relevant_urls.is_empty() {
            return Err("each case must name at least one relevant URL".to_string());
        }
        let unique_relevant = case.relevant_urls.iter().collect::<HashSet<_>>();
        if unique_relevant.len() != case.relevant_urls.len() {
            return Err("a case contains a duplicate relevant URL".to_string());
        }
        for url in &case.relevant_urls {
            let document = documents
                .get(url)
                .ok_or_else(|| "a relevant URL is absent from the captured corpus".to_string())?;
            if case.kind == EvaluationKind::Phrase
                && document
                    .hit
                    .title
                    .as_deref()
                    .is_none_or(|title| title.split_whitespace().count() < 2)
            {
                return Err(
                    "phrase cases require multi-token projected titles in the corpus".to_string(),
                );
            }
            match case.split {
                EvaluationSplit::Train => {
                    train_targets.insert(url.as_str());
                },
                EvaluationSplit::HeldOut => {
                    held_out_targets.insert(url.as_str());
                },
            }
            let bucket = match (case.split, case.kind) {
                (EvaluationSplit::Train, EvaluationKind::Phrase) => 0,
                (EvaluationSplit::Train, EvaluationKind::Control) => 1,
                (EvaluationSplit::HeldOut, EvaluationKind::Phrase) => 2,
                (EvaluationSplit::HeldOut, EvaluationKind::Control) => 3,
            };
            distinct[bucket].insert(url.as_str());
        }
    }
    if !train_targets.is_disjoint(&held_out_targets) {
        return Err("training and held-out relevant URLs must be disjoint".to_string());
    }
    if distinct
        .iter()
        .any(|targets| targets.len() < minimum_per_kind)
    {
        return Err(format!(
            "each split and kind needs at least {minimum_per_kind} distinct relevant URLs"
        ));
    }
    Ok(())
}

fn cases_for(manifest: &EvaluationManifest, split: EvaluationSplit) -> Vec<&EvaluationCase> {
    manifest
        .cases
        .iter()
        .filter(|case| case.split == split)
        .collect()
}

fn scored_search(
    index: &RecallIndex,
    config: CandidateConfig,
    query: &str,
    limit: usize,
) -> Result<Vec<ScoredUrl>, String> {
    // The behavioural lane is held off here: this harness is W6a's phrase-vector
    // experiment, and its candidate set is not a typed prefix.
    let recall = RecallConfig::new(config.order, config.weight).with_frecency_weight(0.0);
    if !recall.vector_enabled() {
        return index
            .lexical
            .search(query, limit)
            .map_err(|error| format!("BM25 evaluation query: {error}"))
            .map(|hits| {
                hits.into_iter()
                    .map(|hit| ScoredUrl {
                        url: hit.url,
                        score: f64::from(hit.score),
                    })
                    .collect()
            });
    }
    index.fused_hits(query, limit, recall).map(|hits| {
        hits.into_iter()
            .map(|hit| ScoredUrl {
                url: hit.url,
                score: hit.score,
            })
            .collect()
    })
}

fn record_metrics(metrics: &mut RankingMetrics, case: &EvaluationCase, hits: &[ScoredUrl]) {
    metrics.cases += 1;
    let relevant = case
        .relevant_urls
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let first_rank = hits
        .iter()
        .position(|hit| relevant.contains(hit.url.as_str()));
    metrics.deterministic_at_one += usize::from(first_rank == Some(0));
    if let Some(rank) = first_rank {
        metrics.reciprocal_rank_sum += 1.0 / (rank + 1) as f64;
    }

    let retrieved = hits
        .iter()
        .filter_map(|hit| {
            relevant
                .contains(hit.url.as_str())
                .then_some(hit.url.as_str())
        })
        .collect::<HashSet<_>>();
    metrics.recall_at_k_sum += retrieved.len() as f64 / relevant.len() as f64;

    if let Some(first) = hits.first() {
        let top_urls = hits
            .iter()
            .take_while(|hit| (hit.score - first.score).abs() <= f64::EPSILON)
            .map(|hit| hit.url.as_str())
            .collect::<HashSet<_>>();
        metrics.unique_at_one +=
            usize::from(top_urls.len() == 1 && top_urls.iter().any(|url| relevant.contains(*url)));
    }
}

fn evaluate(
    index: &RecallIndex,
    config: CandidateConfig,
    cases: &[&EvaluationCase],
    ranking_k: usize,
) -> Result<MetricsByKind, String> {
    let mut metrics = MetricsByKind::default();
    for case in cases {
        let hits = scored_search(index, config, &case.query, ranking_k)?;
        record_metrics(&mut metrics.all, case, &hits);
        match case.kind {
            EvaluationKind::Phrase => record_metrics(&mut metrics.phrase, case, &hits),
            EvaluationKind::Control => record_metrics(&mut metrics.control, case, &hits),
        }
    }
    Ok(metrics)
}

fn p95_query_us(
    index: &RecallIndex,
    config: CandidateConfig,
    cases: &[&EvaluationCase],
    ranking_k: usize,
    rounds: usize,
) -> Result<u128, String> {
    for case in cases {
        std::hint::black_box(scored_search(index, config, &case.query, ranking_k)?);
    }
    let mut samples = Vec::with_capacity(cases.len() * rounds);
    for round in 0..rounds {
        for offset in 0..cases.len() {
            let case = cases[(round + offset) % cases.len()];
            let started = Instant::now();
            std::hint::black_box(scored_search(index, config, &case.query, ranking_k)?);
            samples.push(started.elapsed().as_micros());
        }
    }
    samples.sort_unstable();
    let position = (samples.len() * 95).div_ceil(100).saturating_sub(1);
    samples
        .get(position)
        .copied()
        .ok_or_else(|| "latency sample set is empty".to_string())
}

fn build_orders(
    corpus: &EvaluationCorpus,
    manifest: &EvaluationManifest,
    training: &[&EvaluationCase],
) -> Result<Vec<BuiltOrder>, String> {
    let mut built = Vec::with_capacity(manifest.candidate_orders.len());
    for &order in &manifest.candidate_orders {
        let directory = tempfile::tempdir().map_err(|error| format!("index tempdir: {error}"))?;
        let started = Instant::now();
        let index = RecallIndex::mint(
            &directory.path().join("memory"),
            &corpus.traces,
            &corpus.texts,
            RecallConfig::new(order, 1.0),
        )?;
        let build_us = started.elapsed().as_micros();
        let vector = index
            .vector
            .as_ref()
            .ok_or_else(|| "candidate order did not mint a vector index".to_string())?;
        let dense_vector_bytes = vector
            .len()
            .checked_mul(PHRASE_VECTOR_DIMENSIONS)
            .and_then(|values| values.checked_mul(std::mem::size_of::<f32>()))
            .ok_or_else(|| "dense vector byte count overflow".to_string())?;
        let vector_json_bytes = serde_json::to_vec(vector.index())
            .map_err(|error| format!("serialize vector footprint: {error}"))?
            .len();
        let training_p95_query_us = p95_query_us(
            &index,
            CandidateConfig { order, weight: 1.0 },
            training,
            manifest.ranking_k,
            TIMING_ROUNDS,
        )?;
        built.push(BuiltOrder {
            order,
            index,
            build_us,
            dense_vector_bytes,
            vector_json_bytes,
            training_p95_query_us,
            _directory: directory,
        });
    }
    Ok(built)
}

fn candidate_is_better(candidate: &CandidateScore, incumbent: &CandidateScore) -> bool {
    candidate
        .metrics
        .all
        .unique_at_one
        .cmp(&incumbent.metrics.all.unique_at_one)
        .then_with(|| {
            candidate
                .metrics
                .phrase
                .unique_at_one
                .cmp(&incumbent.metrics.phrase.unique_at_one)
        })
        .then_with(|| {
            candidate
                .metrics
                .all
                .deterministic_at_one
                .cmp(&incumbent.metrics.all.deterministic_at_one)
        })
        .then_with(|| {
            candidate
                .metrics
                .all
                .recall_at_k()
                .total_cmp(&incumbent.metrics.all.recall_at_k())
        })
        .then_with(|| {
            candidate
                .metrics
                .all
                .mean_reciprocal_rank()
                .total_cmp(&incumbent.metrics.all.mean_reciprocal_rank())
        })
        .then_with(|| incumbent.config.weight.total_cmp(&candidate.config.weight))
        .then_with(|| incumbent.config.order.cmp(&candidate.config.order))
        .is_gt()
}

fn select_on_training(
    built: &[BuiltOrder],
    manifest: &EvaluationManifest,
    training: &[&EvaluationCase],
) -> Result<(CandidateScore, Vec<CandidateScore>, u128), String> {
    let baseline_config = CandidateConfig {
        order: 2,
        weight: 0.0,
    };
    let baseline_index = &built
        .first()
        .ok_or_else(|| "candidate order set is empty".to_string())?
        .index;
    let baseline_metrics = evaluate(
        baseline_index,
        baseline_config,
        training,
        manifest.ranking_k,
    )?;
    let baseline_p95 = p95_query_us(
        baseline_index,
        baseline_config,
        training,
        manifest.ranking_k,
        TIMING_ROUNDS,
    )?;
    let mut best = CandidateScore {
        config: baseline_config,
        metrics: baseline_metrics,
        within_training_budgets: baseline_p95 <= u128::from(manifest.budgets.max_p95_query_us),
        preserves_training_controls: true,
    };
    let mut candidates = Vec::new();
    for order in built {
        let within_training_budgets = order.dense_vector_bytes
            <= manifest.budgets.max_dense_vector_bytes
            && order.training_p95_query_us <= u128::from(manifest.budgets.max_p95_query_us);
        for &weight in &manifest.candidate_weights {
            let config = CandidateConfig {
                order: order.order,
                weight,
            };
            let metrics = evaluate(&order.index, config, training, manifest.ranking_k)?;
            let candidate = CandidateScore {
                config,
                metrics,
                within_training_budgets,
                preserves_training_controls: metrics.control.unique_at_one
                    >= baseline_metrics.control.unique_at_one,
            };
            if candidate.within_training_budgets
                && candidate.preserves_training_controls
                && candidate_is_better(&candidate, &best)
            {
                best = candidate;
            }
            candidates.push(candidate);
        }
    }
    Ok((best, candidates, baseline_p95))
}

fn print_metrics(label: &str, metrics: MetricsByKind, ranking_k: usize) {
    for (kind, values) in [
        ("all", metrics.all),
        ("phrase", metrics.phrase),
        ("control", metrics.control),
    ] {
        println!(
            "metrics label={label} kind={kind} cases={} deterministic_at_1={} unique_at_1={} recall_at_{ranking_k}={:.4} mrr_at_{ranking_k}={:.4}",
            values.cases,
            values.deterministic_at_one,
            values.unique_at_one,
            values.recall_at_k(),
            values.mean_reciprocal_rank(),
        );
    }
}

fn run_captured_trail_receipt() -> Result<(), String> {
    let session = std::env::var_os("TURNSTONE_RECALL_EVAL_SESSION")
        .map(PathBuf::from)
        .ok_or_else(|| "TURNSTONE_RECALL_EVAL_SESSION is required".to_string())?;
    let corpus = load_evaluation_corpus(&session)?;
    println!(
        "corpus schema={EVALUATION_SCHEMA} digest={} traces={} traversals={} documents={} titled_documents={}",
        corpus.digest,
        corpus.trace_count,
        corpus.traversal_count,
        corpus.documents.len(),
        corpus.titled_documents,
    );
    if corpus.documents.len() < MIN_DISTINCT_DOCUMENTS {
        println!(
            "verdict=insufficient_corpus found_documents={} required_documents={MIN_DISTINCT_DOCUMENTS}",
            corpus.documents.len()
        );
        return Ok(());
    }

    let manifest_path = std::env::var_os("TURNSTONE_RECALL_EVAL_MANIFEST")
        .map(PathBuf::from)
        .ok_or_else(|| "TURNSTONE_RECALL_EVAL_MANIFEST is required".to_string())?;
    let manifest_bytes =
        fs::read(&manifest_path).map_err(|error| format!("read evaluation manifest: {error}"))?;
    let manifest: EvaluationManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("parse evaluation manifest: {error}"))?;
    validate_manifest(&manifest, &corpus.documents, MIN_CASES_PER_KIND_PER_SPLIT)?;
    println!(
        "manifest digest={} candidate_orders={:?} candidate_weights={:?} ranking_k={} max_dense_vector_bytes={} max_p95_query_us={}",
        blake3::hash(&manifest_bytes).to_hex(),
        manifest.candidate_orders,
        manifest.candidate_weights,
        manifest.ranking_k,
        manifest.budgets.max_dense_vector_bytes,
        manifest.budgets.max_p95_query_us,
    );

    let training = cases_for(&manifest, EvaluationSplit::Train);
    let held_out = cases_for(&manifest, EvaluationSplit::HeldOut);
    let built = build_orders(&corpus, &manifest, &training)?;
    let (selected, candidates, baseline_training_p95) =
        select_on_training(&built, &manifest, &training)?;
    let baseline_config = CandidateConfig {
        order: 2,
        weight: 0.0,
    };
    let baseline_index = &built[0].index;
    let baseline_training = evaluate(
        baseline_index,
        baseline_config,
        &training,
        manifest.ranking_k,
    )?;
    print_metrics("train_bm25", baseline_training, manifest.ranking_k);
    for candidate in &candidates {
        println!(
            "candidate order={} weight={:.2} within_training_budgets={} preserves_training_controls={}",
            candidate.config.order,
            candidate.config.weight,
            candidate.within_training_budgets,
            candidate.preserves_training_controls,
        );
        print_metrics("train_candidate", candidate.metrics, manifest.ranking_k);
    }
    println!(
        "selection source=train order={} weight={:.2} baseline_p95_query_us={baseline_training_p95}",
        selected.config.order, selected.config.weight
    );

    let selected_order = built
        .iter()
        .find(|order| order.order == selected.config.order)
        .unwrap_or(&built[0]);
    let held_out_baseline = evaluate(
        baseline_index,
        baseline_config,
        &held_out,
        manifest.ranking_k,
    )?;
    let held_out_selected = evaluate(
        &selected_order.index,
        selected.config,
        &held_out,
        manifest.ranking_k,
    )?;
    let held_out_p95 = p95_query_us(
        &selected_order.index,
        selected.config,
        &held_out,
        manifest.ranking_k,
        TIMING_ROUNDS,
    )?;
    let (dense_vector_bytes, vector_json_bytes, build_us) = if selected.config.weight > 0.0 {
        (
            selected_order.dense_vector_bytes,
            selected_order.vector_json_bytes,
            selected_order.build_us,
        )
    } else {
        (0, 0, 0)
    };
    let within_held_out_budgets = dense_vector_bytes <= manifest.budgets.max_dense_vector_bytes
        && held_out_p95 <= u128::from(manifest.budgets.max_p95_query_us);
    print_metrics("held_out_bm25", held_out_baseline, manifest.ranking_k);
    print_metrics("held_out_selected", held_out_selected, manifest.ranking_k);
    println!(
        "cost selected_order={} selected_weight={:.2} build_us={build_us} dense_vector_bytes={dense_vector_bytes} vector_json_bytes={vector_json_bytes} held_out_p95_query_us={held_out_p95} within_held_out_budgets={within_held_out_budgets}",
        selected.config.order, selected.config.weight
    );

    let phrase_gain =
        held_out_selected.phrase.unique_at_one > held_out_baseline.phrase.unique_at_one;
    let overall_preserved = held_out_selected.all.unique_at_one
        >= held_out_baseline.all.unique_at_one
        && held_out_selected.all.recall_at_k() >= held_out_baseline.all.recall_at_k();
    let controls_preserved =
        held_out_selected.control.unique_at_one >= held_out_baseline.control.unique_at_one;
    let promotion_candidate = selected.config.weight > 0.0
        && phrase_gain
        && overall_preserved
        && controls_preserved
        && within_held_out_budgets;
    println!(
        "verdict={} phrase_gain={} overall_preserved={} controls_preserved={} budgets_passed={}",
        if promotion_candidate {
            "promotion_candidate"
        } else {
            "keep_bm25"
        },
        phrase_gain,
        overall_preserved,
        controls_preserved,
        within_held_out_budgets,
    );
    Ok(())
}

#[test]
fn training_selection_is_held_out_and_tie_aware() {
    let records = [
        ("https://a.example/train-reversed", "Folder Downloads Open"),
        ("https://z.example/train-ordered", "Open Downloads Folder"),
        ("https://c.example/train-control", "Rust Handbook"),
        ("https://a.example/held-reversed", "Nodes Query Graph"),
        ("https://z.example/held-ordered", "Graph Query Nodes"),
        ("https://c.example/held-control", "Cargo Book"),
    ];
    let events = records
        .iter()
        .enumerate()
        .map(|(index, (url, title))| TraceEvent {
            from: None,
            to: PageRef {
                url: (*url).to_string(),
                title: Some((*title).to_string()),
            },
            transition: TraceTransition::Imported,
            at_ms: index as u64 + 1,
            dwell_ms: None,
            candidates: Vec::new(),
        })
        .collect();
    let traces = vec![BrowsingTrace::from_events("evaluation-test", events)];
    let documents = recall_documents(&page_table_of(&traces, &PageTexts::default()));
    let manifest = EvaluationManifest {
        schema: EVALUATION_SCHEMA.to_string(),
        ranking_k: 3,
        candidate_orders: vec![1, 2],
        candidate_weights: vec![1.0, 2.0],
        budgets: EvaluationBudgets {
            max_dense_vector_bytes: usize::MAX,
            max_p95_query_us: u64::MAX,
        },
        cases: vec![
            EvaluationCase {
                split: EvaluationSplit::Train,
                kind: EvaluationKind::Phrase,
                query: "open downloads folder".to_string(),
                relevant_urls: vec!["https://z.example/train-ordered".to_string()],
            },
            EvaluationCase {
                split: EvaluationSplit::Train,
                kind: EvaluationKind::Control,
                query: "rust handbook".to_string(),
                relevant_urls: vec!["https://c.example/train-control".to_string()],
            },
            EvaluationCase {
                split: EvaluationSplit::HeldOut,
                kind: EvaluationKind::Phrase,
                query: "graph query nodes".to_string(),
                relevant_urls: vec!["https://z.example/held-ordered".to_string()],
            },
            EvaluationCase {
                split: EvaluationSplit::HeldOut,
                kind: EvaluationKind::Control,
                query: "cargo book".to_string(),
                relevant_urls: vec!["https://c.example/held-control".to_string()],
            },
        ],
    };
    validate_manifest(&manifest, &documents, 1).unwrap();
    let corpus = EvaluationCorpus {
        traces,
        texts: PageTexts::default(),
        documents,
        trace_count: 1,
        traversal_count: records.len(),
        titled_documents: records.len(),
        digest: String::new(),
    };
    let training = cases_for(&manifest, EvaluationSplit::Train);
    let held_out = cases_for(&manifest, EvaluationSplit::HeldOut);
    let built = build_orders(&corpus, &manifest, &training).unwrap();
    let (selected, _, _) = select_on_training(&built, &manifest, &training).unwrap();
    assert_eq!(
        selected.config,
        CandidateConfig {
            order: 2,
            weight: 2.0,
        }
    );

    let baseline = evaluate(
        &built[0].index,
        CandidateConfig {
            order: 2,
            weight: 0.0,
        },
        &held_out,
        manifest.ranking_k,
    )
    .unwrap();
    let selected_order = built
        .iter()
        .find(|order| order.order == selected.config.order)
        .unwrap();
    let selected_metrics = evaluate(
        &selected_order.index,
        selected.config,
        &held_out,
        manifest.ranking_k,
    )
    .unwrap();
    assert_eq!(baseline.phrase.unique_at_one, 0);
    assert_eq!(selected_metrics.phrase.unique_at_one, 1);
    assert_eq!(selected_metrics.control.unique_at_one, 1);
}

/// Run explicitly against a real session. The harness copies it before opening
/// any store and prints only aggregate counts, digests, metrics, and costs.
#[test]
#[ignore = "requires an explicit private Turnstone session and evaluation manifest"]
fn captured_trail_recall_receipt() {
    run_captured_trail_receipt().unwrap();
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
    let index = RecallIndex::mint(
        index_root.path(),
        &corpus.traces,
        &corpus.texts,
        RecallConfig::default(),
    )
    .unwrap();
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
    println!("corpus digest={} source=captured", corpus.digest);
    let index_root = tempfile::tempdir().expect("mint receipt index root");
    for config in [RecallConfig::default(), RecallConfig::new(2, 2.0)] {
        let index =
            RecallIndex::mint(index_root.path(), &corpus.traces, &corpus.texts, config).unwrap();
        println!("captured {config:?} {:?}", index.receipt);
    }
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

    // (a) the mint receipt, behavioural lane on, vector lane off.
    let index_root =
        tempfile::TempDir::new_in(&scratch).map_err(|error| format!("index temp: {error}"))?;
    let index = RecallIndex::mint(index_root.path(), &stored, &texts, RecallConfig::default())?;
    println!("  mint vector=off {:?}", index.receipt);

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

    // The vector lane last: 4,096 dims per page is the memory risk on a corpus
    // this wide, and every other number is already printed if it falls over.
    let vector = RecallIndex::mint(index_root.path(), &stored, &texts, RecallConfig::new(2, 2.0))?;
    println!("  mint vector=on {:?}", vector.receipt);
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
