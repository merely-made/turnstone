// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The model path with a stub embedder: no weights, no network. One ignored
//! test runs the real MiniLM folder.

use graphshell_host::goal_surface::fixture::{self, INDEX, OWNER, TRAVEL};
use super::model_path::{Embedder, ModelState, calibrated, real_loader};
use super::*;
use portolan::resource_id;
use cambium::{DomHandle, GenetAppRunner, PointerClick};
use eidetic::models::embedding::{EmbeddingComponentV1, EmbeddingCorpusV1, EmbeddingEnvelopeV1, EmbeddingMetric, EmbeddingSpaceV1};
use eidetic_search::goal::GoalVerdict;
use eidetic_search::goal_model::{EmbedRole, GoalEmbedder, ModelAssessRule, SelectionWinner};
use genet_scripted_dom::{NodeId, ScriptedDom};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{Arc, atomic::{AtomicBool, Ordering}},
    time::Duration,
};

/// Sourdough, travel and oven vocabularies are axes, so "sourdough on
/// holiday" is near "Baking while travelling" without sharing a token.
struct Stub {
    fail: Arc<AtomicBool>,
}
const FAMILIES: [&[&str]; 3] = [
    &["starter", "sourdough", "culture"],
    &["travel", "trip", "holiday", "journey", "flakes", "hotel", "away"],
    &["oven", "steam", "flour"],
];
impl GoalEmbedder for Stub {
    fn envelope(&self) -> EmbeddingEnvelopeV1 {
        let part = EmbeddingComponentV1 { name: "stub".into(), revision: "1".into(), parameters: BTreeMap::new(), artifacts: BTreeMap::new() };
        EmbeddingEnvelopeV1 {
            version: 1,
            space: EmbeddingSpaceV1 {
                producer: part.clone(),
                tokenizer: part.clone(),
                preprocessing: part,
                pooling: "sum".into(),
                query_prefix: String::new(),
                document_prefix: String::new(),
                normalization: "none".into(),
                dimensions: 4,
                metric: EmbeddingMetric::Cosine,
            },
            corpus: EmbeddingCorpusV1 { chunker: "unset".into(), sources: Vec::new() },
        }
    }
    fn embed(&mut self, texts: &[&str], _: EmbedRole) -> Result<Vec<Vec<f32>>, String> {
        if self.fail.load(Ordering::SeqCst) {
            return Err("device lost".into());
        }
        Ok(texts
            .iter()
            .map(|t| {
                let t = t.to_lowercase();
                let mut v: Vec<f32> = FAMILIES.iter().map(|f| f.iter().filter(|w| t.contains(*w)).count() as f32).collect();
                v.push(0.01);
                v
            })
            .collect())
    }
}

const GOAL: &str = "sourdough on holiday";

fn stub_loader(fail: &Arc<AtomicBool>) -> model_path::LoaderFor {
    let fail = fail.clone();
    Box::new(move |_| {
        let fail = fail.clone();
        Box::new(move || Ok(calibrated(Box::new(Stub { fail }) as Embedder, "stub".into(), Vec::new(), 0)))
    })
}

fn surface(goal: &str) -> GoalSurface {
    let pages = Rc::new(pollster::block_on(fixture::pages()));
    let mut s = GoalSurface::new(pages);
    s.goal.insert_str(goal);
    s
}

fn modelled(fail: &Arc<AtomicBool>) -> GoalSurface {
    let mut s = surface(GOAL);
    s.model.dir = Some("stub-model".into());
    s.model.set_loader(stub_loader(fail));
    s.model.toggle();
    settle(&mut s, |s| matches!(s.model.state, ModelState::Ready(_)));
    s
}

/// Poll the worker until `done`, as a host does on each wake.
fn settle(s: &mut GoalSurface, done: impl Fn(&GoalSurface) -> bool) {
    for _ in 0..1000 {
        s.poll_model();
        if done(s) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the model worker did not answer: {:?} {:?}", s.model.state, s.notice);
}

fn run_with_model(s: &mut GoalSurface) {
    s.run();
    settle(s, |s| !s.model.running && s.model.waiting_is_empty());
}

fn verdicts(run: &GoalRunView) -> Vec<(String, Option<GoalVerdict>)> {
    run.steps
        .iter()
        .map(|s| (s.address.clone(), if let Outcome::Visited(v) = &s.outcome { Some(v.verdict) } else { None }))
        .collect()
}

#[test]
fn with_the_toggle_off_the_run_is_the_ordinary_one() {
    let mut plain = surface(GOAL);
    plain.run();
    let mut off = surface(GOAL);
    off.model.dir = Some("stub-model".into());
    off.run();
    assert!(!off.model.enabled && off.model.state == ModelState::Idle, "a configured folder is not loaded until asked");
    assert_eq!(off.run, plain.run);
    assert_eq!(off.receipt, plain.receipt);
    let (run, receipt) = (off.run.as_ref().unwrap(), off.receipt.as_ref().unwrap());
    assert!(run.model.is_none() && receipt.model.is_none() && run.steps.iter().all(|s| s.model.is_none()));
    assert_eq!(run.matched, 0, "the paraphrase shares no goal term with any page");
}

#[test]
fn the_toggle_is_disabled_with_a_reason_when_no_model_is_configured() {
    let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
    let mut runner = GenetAppRunner::new(dom.clone(), view as fn(&GoalSurface) -> view::Child, surface(GOAL));
    let (toggle, status) = {
        let dom = dom.borrow();
        (find(&dom, runner.root(), "goal-model-toggle").unwrap(), find(&dom, runner.root(), "goal-model-status").unwrap())
    };
    {
        let dom = dom.borrow();
        let attr = |n, name: &str| dom.attribute(n, &Namespace::from(""), &LocalName::from(name)).map(str::to_owned);
        assert!(attr(toggle, "disabled").is_some() && attr(toggle, "aria-pressed").as_deref() == Some("false"));
        let reason = text(&dom, status);
        assert!(reason.contains("No model is configured") && reason.contains("GRAPHSHELL_GOAL_MODEL_DIR"), "{reason}");
    }
    runner.dispatch_click(toggle, PointerClick::at((1., 1.)));
    assert!(!runner.state().model.enabled, "a disabled toggle does nothing");
}

#[test]
fn on_the_model_scores_are_shown_and_a_paraphrase_is_found() {
    let fail = Arc::new(AtomicBool::new(false));
    let mut s = modelled(&fail);
    let ModelState::Ready(loaded) = s.model.state.clone() else { panic!() };
    assert!(loaded.threshold_note.contains("generic default"), "an uncalibrated model says so: {}", loaded.threshold_note);
    assert!(s.model.status().contains("Match threshold 0.500"), "{}", s.model.status());
    run_with_model(&mut s);
    let run = s.run.as_ref().unwrap();
    let info = run.model.as_ref().expect("a model run");
    assert_eq!((info.rule, info.threshold_milli, info.lexical_matched, info.unavailable_steps), (ModelAssessRule::Or, 500, 0, 0));
    assert!(run.matched >= 1, "the model matched what S1 could not: {:?}", verdicts(run));
    let travel = run.steps.iter().find(|s| s.address == TRAVEL).unwrap();
    let Outcome::Visited(v) = &travel.outcome else { panic!() };
    let m = v.model.as_ref().unwrap();
    assert!(m.similarity_milli.is_some_and(|s| s >= 500) && v.verdict != GoalVerdict::Continue);
    assert_eq!(m.s1_verdict, GoalVerdict::Continue);
    assert!(m.decided_by.starts_with("model"), "{}", m.decided_by);
    assert!(travel.model.is_some() && !travel.reason.is_empty());
    assert!(run.steps.iter().any(|s| s.model.as_ref().is_some_and(|p| p.winner == SelectionWinner::Model)));
    let ordinary = {
        let mut p = surface(GOAL);
        p.run();
        p.run.unwrap().steps.len()
    };
    assert!(run.steps.len() >= 1 && ordinary >= 1);
}

#[test]
fn the_view_shows_the_model_beside_the_lexical_verdict() {
    let fail = Arc::new(AtomicBool::new(false));
    let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
    let mut runner = GenetAppRunner::new(dom.clone(), view as fn(&GoalSurface) -> view::Child, modelled(&fail));
    let find_id = |id: &str| find(&dom.borrow(), runner.root(), id).unwrap();
    let toggle = find_id("goal-model-toggle");
    assert_eq!(dom.borrow().attribute(toggle, &Namespace::from(""), &LocalName::from("disabled")), None);
    let run = find_id("goal-run");
    runner.dispatch_click(run, PointerClick::at((1., 1.)));
    for _ in 0..1000 {
        runner.update(|s| {
            s.poll_model();
        });
        if runner.state().run.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let dom = dom.borrow();
    let body = text(&dom, find(&dom, runner.root(), "goal-results").expect("results"));
    for needle in ["Lexical: not a match", "Model similarity ", "(threshold 0.500)", "Decided by: model (the lexical rule found no match)", "Chosen [model]:", "Model path: rule lexical or model"] {
        assert!(body.contains(needle), "missing {needle:?} in {body}");
    }
}

#[test]
fn an_embedder_failure_falls_back_to_the_lexical_answer_step_by_step() {
    let fail = Arc::new(AtomicBool::new(false));
    let mut s = modelled(&fail);
    fail.store(true, Ordering::SeqCst);
    run_with_model(&mut s);
    let run = s.run.clone().unwrap();
    let mut plain = surface(GOAL);
    plain.run();
    let plain = plain.run.unwrap();
    assert_eq!(verdicts(&run), verdicts(&plain), "S1's answer stands where the model has none");
    assert_eq!(run.matched, plain.matched);
    let info = run.model.as_ref().unwrap();
    assert!(info.unavailable_steps > 0);
    let seen = run.steps.iter().find_map(|s| if let Outcome::Visited(v) = &s.outcome { v.model.clone() } else { None }).unwrap();
    assert!(seen.similarity_milli.is_none() && seen.unavailable.as_deref().is_some_and(|w| w.contains("device lost")));
    assert!(seen.decided_by.contains("lexical rule alone"));
    // The seed has no label to score, so only later selections report the outage.
    assert!(run.steps.iter().all(|s| s.model.as_ref().is_some_and(|p| p.winner == SelectionWinner::S1)));
    let down = run.steps.iter().find(|s| s.model.as_ref().is_some_and(|p| p.unavailable.is_some())).expect("an unavailable selection");
    assert!(down.reason.starts_with("Model unavailable here (device lost)"), "{}", down.reason);
}

#[test]
fn a_model_that_cannot_load_leaves_find_on_the_ordinary_path_with_the_reason() {
    let mut s = surface(GOAL);
    s.model.dir = Some("missing".into());
    s.model.set_loader(Box::new(|_| Box::new(|| Err("model.safetensors is not the pinned file".into()))));
    s.model.toggle();
    s.run();
    assert!(s.run.is_none(), "the run waits for the load rather than blocking on it");
    settle(&mut s, |s| s.run.is_some());
    assert!(matches!(&s.model.state, ModelState::Failed(w) if w.contains("pinned")));
    assert!(s.run.as_ref().unwrap().model.is_none() && s.receipt.as_ref().unwrap().model.is_none());
    assert!(s.notice.as_deref().unwrap().contains("unavailable") && s.model.status().contains("Model unavailable"));
    let mut plain = surface(GOAL);
    plain.run();
    assert_eq!(s.run, plain.run);
}

#[test]
fn the_model_record_is_in_the_receipt_and_serializes() {
    let fail = Arc::new(AtomicBool::new(false));
    let mut s = modelled(&fail);
    run_with_model(&mut s);
    let receipt = s.receipt.clone().unwrap();
    let record = receipt.model.as_ref().expect("model record in the receipt");
    assert_eq!(record.operation_version, "eidetic.goal-model/v1");
    assert_eq!(record.settings.match_similarity_milli, 500);
    assert_eq!(record.settings.assess_rule, ModelAssessRule::Or);
    assert_eq!(record.assessments.len(), receipt.visits.len());
    assert_eq!(record.envelope.space.producer.name, "stub");
    assert!(record.matched >= 1 && receipt.goal.matched == 0, "S1's record is unchanged beside the model's");
    let json = serde_json::to_string(&receipt).unwrap();
    assert!(!json.contains("Hotel kitchens rarely"), "retained text stays out");
    let back: RetainedTraversalReceipt = serde_json::from_str(&json).unwrap();
    assert_eq!(back, receipt);
}

#[test]
fn a_second_find_while_the_model_runs_is_refused() {
    let fail = Arc::new(AtomicBool::new(false));
    let mut s = modelled(&fail);
    s.run();
    assert!(s.model.running);
    s.run();
    assert_eq!(s.notice.as_deref(), Some("A model run is still going; wait for it."));
    settle(&mut s, |s| s.run.is_some());
}

fn text(dom: &ScriptedDom, node: NodeId) -> String {
    let mut out = dom.text(node).unwrap_or("").to_owned();
    for child in dom.dom_children(node) {
        out.push_str(&text(dom, child));
    }
    out
}
fn find(dom: &ScriptedDom, node: NodeId, id: &str) -> Option<NodeId> {
    if dom.attribute(node, &Namespace::from(""), &LocalName::from("id")) == Some(id) {
        return Some(node);
    }
    dom.dom_children(node).find_map(|c| find(dom, c, id))
}

/// The real checkpoint: set `GRAPHSHELL_GOAL_MODEL_DIR` (default: the local
/// testing folder). Verifies the pinned digests, loads on a worker, and finds
/// a paraphrase the lexical rule misses at MiniLM's calibrated threshold.
#[test]
#[ignore = "needs the local all-MiniLM-L6-v2 folder (GRAPHSHELL_GOAL_MODEL_DIR)"]
fn the_real_minilm_finds_a_paraphrase_the_lexical_rule_misses() {
    let dir = model_path::dir_from_env()
        .unwrap_or_else(|| "/Users/markik/Code/testing/eidetic/models/all-MiniLM-L6-v2".into());
    let mut s = surface(fixture::GOAL_PARAPHRASE);
    s.model.dir = Some(dir.clone());
    s.model.set_loader(Box::new(real_loader));
    s.model.toggle();
    settle(&mut s, |s| !matches!(s.model.state, ModelState::Loading));
    let ModelState::Ready(loaded) = s.model.state.clone() else { panic!("{:?}", s.model.state) };
    assert_eq!(loaded.threshold_milli, 220, "{}", loaded.threshold_note);
    assert_eq!(loaded.digests.len(), 3);
    s.start = Start::Source(s.pages().sources().iter().position(|(src, _)| *src == source_id(OWNER, resource_id(INDEX))).unwrap());
    run_with_model(&mut s);
    let run = s.run.as_ref().unwrap();
    let info = run.model.as_ref().unwrap();
    assert_eq!(info.lexical_matched, 0, "S1 finds nothing for the paraphrase");
    assert!(run.matched >= 1, "the model finds it: {:?}", verdicts(run));
}
