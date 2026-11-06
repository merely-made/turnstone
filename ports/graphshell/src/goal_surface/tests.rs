// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::fixture::{self, HOTEL, INDEX, OWNER, STARTER, STARTER_V1, TRAVEL};
use super::*;
use chartulary::resource_id;
use eidetic::models::recall::{RecallSourceId, RecallSourceKind};
use eidetic_search::goal::GoalVerdict;
use fleece::retained_traversal::{RetainedProvider, RetainedStop};
use genet_scripted_dom::{NodeId, ScriptedDom};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use std::{cell::RefCell, rc::Rc};

fn pages() -> RetainedPages {
    pollster::block_on(fixture::pages())
}
fn id(url: &str) -> RecallSourceId {
    source_id(OWNER, resource_id(url))
}
fn index_of(pages: &RetainedPages, url: &str) -> usize {
    pages.sources().iter().position(|(s, _)| *s == id(url)).unwrap()
}
fn surface(start: Option<&str>, goal: &str) -> GoalSurface {
    let pages = Rc::new(pages());
    let mut s = GoalSurface::new(pages.clone());
    s.start = start.map_or(Start::All, |u| Start::Source(index_of(&pages, u)));
    s.goal.insert_str(goal);
    s
}
const GOAL: &str = "keep a starter alive while travelling";

#[test]
fn identities_map_explicitly_and_versions_are_content_hashes() {
    let pages = pages();
    assert_eq!(pages.sources().len(), 5, "the unretained hotel page is not a source");
    let source = id(STARTER);
    assert_eq!(source.kind, RecallSourceKind::PageText);
    assert_eq!(source.owner.as_deref(), Some(OWNER));
    assert_eq!(source.source, resource_id(STARTER).to_string());
    assert_eq!(resource_of(OWNER, &source), Some(resource_id(STARTER)));
    let page = pages.page(&source).unwrap();
    assert_eq!(page.versions.len(), 2, "earlier version stays addressable");
    let v1 = eidetic::Hash::of(STARTER_V1.as_bytes()).to_hex();
    assert!(page.versions.contains_key(&v1));
    assert_eq!(
        page.current,
        eidetic::Hash::of(page.versions[&page.current].as_bytes()).to_hex()
    );
    assert_ne!(page.current, v1);
}

#[test]
fn provider_reads_held_versions_and_refuses_what_is_not_held() {
    let pages = pages();
    let mut provider = pages.provider();
    let source = id(STARTER);
    let current = provider.retained(&source, None).unwrap();
    assert_eq!(current.text.version, pages.page(&source).unwrap().current);
    assert!(current.text.text.contains("refrigerate"));
    let v1 = eidetic::Hash::of(STARTER_V1.as_bytes()).to_hex();
    let earlier = provider.retained(&source, Some(&v1)).unwrap();
    assert_eq!(earlier.text.text, STARTER_V1, "a held version is read exactly");
    let unknown = provider.retained(&source, Some("0000")).unwrap();
    assert_eq!(unknown.text.version, current.text.version, "never substitutes silently: the caller sees the version it got");
    // Another owner, another kind, a node qualifier or a malformed id resolve to nothing.
    for bad in [
        RecallSourceId { owner: Some("someone-else".into()), ..source.clone() },
        RecallSourceId { owner: None, ..source.clone() },
        RecallSourceId { kind: RecallSourceKind::Clip, ..source.clone() },
        RecallSourceId { node: Some("n".into()), ..source.clone() },
        RecallSourceId { source: "not-a-uuid".into(), ..source.clone() },
        id(HOTEL),
    ] {
        assert!(provider.retained(&bad, None).is_none(), "{bad:?}");
    }
}

#[test]
fn links_come_from_graph_relations_with_labels_and_include_unretained_targets() {
    let pages = pages();
    let observed = pages.provider().retained(&id(INDEX), None).unwrap();
    let targets: Vec<_> = observed.links.iter().map(|l| l.source.clone()).collect();
    for url in [fixture::OVEN, fixture::FLOUR, HOTEL, TRAVEL, STARTER] {
        assert!(targets.contains(&id(url)), "{url}");
    }
    let travel = observed.links.iter().find(|l| l.source == id(TRAVEL)).unwrap();
    assert_eq!(travel.label.as_deref(), Some("Baking while travelling trips"));
    assert_eq!(
        pages.describe(&id(HOTEL)),
        Some((HOTEL.to_owned(), Some("Hotel kitchens".to_owned())))
    );
}

#[test]
fn a_run_over_the_fixture_graph_cites_sources_and_explains_its_choices() {
    let mut s = surface(Some(INDEX), GOAL);
    s.run();
    assert_eq!(s.notice, None);
    let run = s.run.as_ref().unwrap();
    for term in ["keep", "starter", "alive", "travelling"] {
        assert!(run.terms.iter().any(|t| t == term), "{term} in {:?}", run.terms);
    }
    assert!(!run.terms.iter().any(|t| t == "while" || t == "a"), "stop words are not goal terms: {:?}", run.terms);
    assert_eq!(run.stop, RetainedStop::Exhausted);
    let cited: Vec<_> = run.matches().map(|s| s.address.as_str()).collect();
    assert!(cited.contains(&TRAVEL) && cited.contains(&STARTER), "{cited:?}");
    assert!(!cited.contains(&fixture::OVEN));
    let starter = run.steps.iter().find(|s| s.address == STARTER).unwrap();
    let Outcome::Visited(v) = &starter.outcome else { panic!("{starter:?}") };
    assert_eq!(v.retained_version, s.pages().page(&starter.source).unwrap().current, "cites the current version");
    assert_eq!(v.text_hash, v.retained_version, "version is the text's content hash");
    assert!(v.matched_terms.contains(&"alive".to_owned()) && !v.missing_terms.is_empty());
    let pass = v.snippets.iter().find(|p| p.hit.to_lowercase() == "alive").expect("cited passage");
    assert!(pass.before.ends_with("keep a starter ") && pass.after.starts_with(" while"));
    // The cited selector reproduces the highlighted text from the retained text exactly.
    let text = s.pages().text_of(&starter.source, &v.retained_version).unwrap();
    let chars: String = text.chars().skip(pass.start).take(pass.end - pass.start).collect();
    assert_eq!(chars, pass.hit);
    // Traversal explanation: seed, then candidates chosen by label overlap.
    assert_eq!(run.steps[0].address, INDEX);
    assert!(run.steps[0].reason.contains("No pending label"), "{}", run.steps[0].reason);
    let second = &run.steps[1];
    assert!(second.reason.contains("shares goal terms"), "{}", second.reason);
    assert!(second.address == TRAVEL || second.address == STARTER);
    // A linked page nothing is retained for is reported, not fetched.
    let hotel = run.steps.iter().find(|s| s.address == HOTEL).unwrap();
    assert!(matches!(&hotel.outcome, Outcome::Failed(why) if why.contains("not fetched")));
    assert!(run.budgets.max_nodes == 50 && run.requests >= run.steps.len() - 1);
}

#[test]
fn ordinary_path_needs_no_model_and_a_run_changes_nothing() {
    let mut s = surface(None, GOAL);
    let before = s.pages().clone();
    s.run();
    let run = s.run.as_ref().unwrap();
    assert!(run.operation.starts_with("eidetic.goal-assessment") && run.tokenizer.len() > 0);
    assert!(run.matched >= 2);
    assert_eq!(*s.pages(), before, "the snapshot is untouched; a run writes nothing");
}

#[test]
fn a_named_version_is_cited_and_an_unheld_one_is_reported_as_mismatch() {
    use fleece::retained_traversal::{RetainedLimits, RetainedSeed, run_retained_traversal};
    let pages = pages();
    let v1 = eidetic::Hash::of(STARTER_V1.as_bytes()).to_hex();
    let run = |version: &str| {
        let report = run_retained_traversal(
            &mut pages.provider(),
            [RetainedSeed { source: id(STARTER), version: Some(version.into()) }],
            GOAL,
            Default::default(),
            Default::default(),
            RetainedLimits::default(),
            || false,
        )
        .unwrap();
        GoalRunView::build(&report, &pages, Budgets { max_nodes: 50, max_depth: 2, max_requests: 50, max_bytes: 1 << 22 })
    };
    let held = run(&v1);
    let Outcome::Visited(v) = &held.steps[0].outcome else { panic!() };
    assert_eq!((v.requested_version.as_deref(), v.retained_version.as_str()), (Some(v1.as_str()), v1.as_str()));
    assert_eq!(v.verdict, GoalVerdict::Continue, "the earlier text says nothing of travel");
    let missing = run(&"0".repeat(64));
    assert!(matches!(&missing.steps[0].outcome, Outcome::Failed(why) if why.contains("different version")));
}

#[test]
fn refusals_leave_the_last_result_and_say_why() {
    let mut s = surface(None, "   ");
    s.run();
    assert!(s.run.is_none() && s.notice.as_deref().unwrap().contains("cannot be searched"));
    s.goal.insert_str("starter");
    s.max_depth = cambium::TextInput::new("deep");
    s.run();
    assert_eq!(s.notice.as_deref(), Some("Depth must be a whole number."));
    s.max_depth = cambium::TextInput::new("1");
    s.max_visits = cambium::TextInput::new("2");
    s.run();
    let run = s.run.as_ref().unwrap();
    assert_eq!(run.stop, RetainedStop::NodeBudget);
    assert_eq!(run.steps.len(), 2, "the visit limit bounds the walk");
    s.max_visits = cambium::TextInput::new("0");
    s.run();
    assert!(s.notice.is_some() && s.run.is_some(), "a refused run keeps the earlier result");
}

#[test]
fn snippets_use_scalar_offsets_and_clamp_at_the_edges() {
    let text = "café au lait, café noir";
    let at = snippet(text, 14, 18, 5).unwrap();
    assert_eq!((at.before.as_str(), at.hit.as_str(), at.after.as_str()), ("ait, ", "café", " noir"));
    let head = snippet(text, 0, 4, 99).unwrap();
    assert_eq!((head.before.as_str(), head.hit.as_str()), ("", "café"));
    assert!(snippet(text, 5, 4, 2).is_none() && snippet(text, 40, 41, 2).is_none());
    assert_eq!(snippet("a\nb", 0, 3, 0).unwrap().hit, "a b");
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
fn marks(dom: &ScriptedDom, node: NodeId, out: &mut Vec<String>) {
    if dom.element_name(node).is_some_and(|n| n.local.to_string() == "mark") {
        out.push(text(dom, node));
    }
    for child in dom.dom_children(node) {
        marks(dom, child, out);
    }
}

#[test]
fn the_view_renders_citations_highlights_and_the_walks_reasons() {
    let dom: cambium::DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
    let mut runner = cambium::GenetAppRunner::new(
        dom.clone(),
        view as fn(&GoalSurface) -> view_child::Child,
        surface(Some(INDEX), GOAL),
    );
    assert!(find(&dom.borrow(), runner.root(), "goal-results").is_none(), "building the view runs nothing");
    let run = find(&dom.borrow(), runner.root(), "goal-run").unwrap();
    runner.dispatch_click(run, cambium::PointerClick::at((1., 1.)));
    let dom = dom.borrow();
    let results = find(&dom, runner.root(), "goal-results").expect("results after Find");
    let body = text(&dom, results);
    for needle in ["Cited results", "Starter care", "Baking while travelling", "version ", "Matched ", "missing ", "How the walk chose", "Chosen: ", "Nothing is retained for this source; it was not fetched.", "Budgets: visits 50, depth 2"] {
        assert!(body.contains(needle), "missing {needle:?} in {body}");
    }
    let mut highlighted = Vec::new();
    marks(&dom, results, &mut highlighted);
    assert!(highlighted.iter().any(|m| m.eq_ignore_ascii_case("alive")), "{highlighted:?}");
    assert!(runner.state().run.is_some() && runner.state().notice.is_none());
}

mod view_child {
    pub use crate::goal_surface::view::Child;
}
