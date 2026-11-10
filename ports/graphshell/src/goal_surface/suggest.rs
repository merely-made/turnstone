// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Tag suggestions over retained pages, kept in Eidetic's suggestion ledger.
//!
//! The producer is Eidetic's lexical `suggest_tags` against the tag concepts
//! the graph already holds. A suggestion is a proposal: it lives in the
//! [`SuggestionLedger`] (journaled in the same store as the graph, so it
//! survives a reopen) and never becomes a personal tag by itself.
//!
//! - **Accept** admits a personal `taggedWith` assertion through the graph
//!   kernel, citing the suggestion, saves the graph, then appends the review.
//!   A failure between the two leaves the tag held and the suggestion still
//!   unreviewed; accepting again finds the tag held and records the review.
//! - **Reject** appends a review and touches no graph state.
//! - A later run is a new suggestion (its id covers the run), so regenerating
//!   cannot rewrite an earlier suggestion, its reviews or any personal tag.
//!
//! Only the writer runs these; a follower reads the ledger through a handle
//! that refuses writes.

use graphshell_host::goal_surface::{GraphFacts, RetainedPages, resource_of, source_id};
use super::model::{CONTEXT_CHARS, Snippet, snippet};
use crate::app::GraphshellApp;
use eidetic::Hash;
use eidetic::models::recall::{RecallSourceId, RetainedSourceText};
use eidetic::records::{
    KeepingTier, ReviewState, SuggestionProducerKind, SuggestionLedger, TagIdentity,
    TagSuggestionReviewV1, TagSuggestionV1, accept, reject, review_state,
};
use eidetic_search::{TagConcept, TagSuggestSettings, suggest_tags};
use mere::kernel::graph::resource_tag_admission::{AcceptedTagSuggestion, TagAdmission};
use muniment::Backend;
use std::collections::BTreeSet;
use uuid::Uuid;

/// Suggestions a run may propose; more is a sign the concepts are too broad.
pub const MAX_PER_RUN: usize = 200;

/// A produced batch, not yet recorded.
#[derive(Clone, Debug, PartialEq)]
pub struct Produced {
    pub run: String,
    pub suggestions: Vec<TagSuggestionV1>,
    pub sources: usize,
    pub concepts: usize,
}

/// Suggest tags for `scope` (every retained page when `None`) from the
/// concepts the graph holds, skipping any page that already has the concept
/// from anyone. Reads only.
///
/// # Errors
/// An empty run id, or the producer's refusal.
pub fn produce(
    pages: &RetainedPages,
    facts: &GraphFacts,
    scope: Option<&BTreeSet<RecallSourceId>>,
    run: &str,
) -> Result<Produced, String> {
    let owner = pages.owner();
    let sources: Vec<RetainedSourceText> = pages
        .sources()
        .into_iter()
        .filter(|(source, _)| scope.is_none_or(|wanted| wanted.contains(source)))
        .filter_map(|(source, page)| {
            Some(RetainedSourceText {
                text: page.versions.get(&page.current)?.clone(),
                version: page.current.clone(),
                title: page.title.clone(),
                source,
            })
        })
        .collect();
    let concepts: Vec<TagConcept> = facts
        .concepts()
        .iter()
        .map(|c| TagConcept {
            tag: TagIdentity {
                owner: c.owner.clone(),
                concept: c.iri.clone(),
            },
            label: c.label.clone(),
        })
        .collect();
    let existing: Vec<(RecallSourceId, TagIdentity)> = facts
        .tagged()
        .iter()
        .filter_map(|(resource, iri)| {
            let concept = facts.concepts().iter().find(|c| &c.iri == iri)?;
            Some((
                source_id(owner, *resource),
                TagIdentity {
                    owner: concept.owner.clone(),
                    concept: iri.clone(),
                },
            ))
        })
        .collect();
    let mut suggestions = suggest_tags(&sources, &concepts, &existing, run, &TagSuggestSettings::default())
        .map_err(|e| format!("{e:?}"))?;
    suggestions.truncate(MAX_PER_RUN);
    Ok(Produced {
        run: run.to_owned(),
        suggestions,
        sources: sources.len(),
        concepts: concepts.len(),
    })
}

/// What the ledger holds, as plain data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Held {
    pub suggestions: Vec<TagSuggestionV1>,
    pub reviews: Vec<TagSuggestionReviewV1>,
}

/// Read the ledger. Works on a handle that refuses writes.
///
/// # Errors
/// A store failure or a journal that does not replay.
pub async fn read<B: Backend>(backend: B) -> Result<Held, String> {
    let ledger = SuggestionLedger::open(backend).await.map_err(|e| format!("{e:?}"))?;
    Ok(Held {
        suggestions: ledger.suggestions().to_vec(),
        reviews: ledger.reviews().to_vec(),
    })
}

/// Record a produced batch. Returns how many were new.
///
/// # Errors
/// A store failure; entries recorded before it stay recorded.
pub async fn record<B: Backend>(backend: B, batch: Vec<TagSuggestionV1>) -> Result<usize, String> {
    let mut ledger = SuggestionLedger::open(backend).await.map_err(|e| format!("{e:?}"))?;
    let mut new = 0;
    for suggestion in batch {
        let (_, recorded) = ledger.record_suggestion(suggestion).await.map_err(|e| format!("{e:?}"))?;
        new += usize::from(recorded == eidetic::records::Recorded::New);
    }
    Ok(new)
}

/// The writer's agent IRI: the asserter of its own tags, and so the reviewer.
pub fn reviewer<B: Backend>(app: &GraphshellApp<B>) -> String {
    app.host.graph().write_author().asserter_iri()
}

/// Reject a held suggestion: a review only.
///
/// # Errors
/// An id that is not 64 hex digits or not held, or a store failure.
pub async fn reject_one<B: Backend>(
    backend: B,
    id: &str,
    reviewer: &str,
    at_unix_micros: i64,
) -> Result<(), String> {
    let id = Hash::parse(id).map_err(|e| format!("{e:?}"))?;
    let mut ledger = SuggestionLedger::open(backend).await.map_err(|e| format!("{e:?}"))?;
    let held = ledger.get(&id).ok_or("that suggestion is not in the ledger")?.clone();
    let review = reject(&held, reviewer, at_unix_micros, None);
    ledger.record_review(review).await.map_err(|e| format!("{e:?}"))
}

/// What accepting did to the graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accepted {
    Added,
    AlreadyHeld,
}

/// Accept a held suggestion: admit the personal tag, save the graph, then
/// append the review. `owner` is the page owner label the suggestion's subject
/// was minted under. Returns what the graph did and the object (surface) now
/// carrying the tag.
///
/// # Errors
/// An unknown suggestion, a subject or concept the graph no longer holds, or
/// a store failure.
pub async fn accept_one<B: pandect::graph_session::LegacyTransactionBackend>(
    app: &mut GraphshellApp<B>,
    ledger_backend: B,
    owner: &str,
    id: &str,
    at_unix_micros: i64,
    saved_at_secs: u64,
) -> Result<(Accepted, Uuid), String> {
    let id = Hash::parse(id).map_err(|e| format!("{e:?}"))?;
    let mut ledger = SuggestionLedger::open(ledger_backend).await.map_err(|e| format!("{e:?}"))?;
    let held = ledger.get(&id).ok_or("that suggestion is not in the ledger")?.clone();
    let reviewer = reviewer(app);
    let (review, draft) = accept(&held, &reviewer, at_unix_micros);
    let resource = resource_of(owner, &draft.subject).ok_or("the suggestion names a page from another owner")?;
    let facts = GraphFacts::read(app.host.graph());
    let concept = facts
        .concept(&draft.concept.owner, &draft.concept.concept)
        .ok_or("the tag concept is no longer in the graph")?
        .clone();
    let admission = app
        .host
        .mutate_product_graph(|graph| {
            graph.admit_accepted_tag_suggestion(
                resource,
                &AcceptedTagSuggestion {
                    concept_owner_iri: concept.owner,
                    concept_iri: concept.iri,
                    concept_label: Some(concept.label),
                    asserter_iri: draft.asserter.clone(),
                    suggestion_hash_hex: draft.cites_suggestion.to_hex(),
                },
            )
        })
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:?}"))?;
    app.mount_local().map_err(|e| e.to_string())?;
    app.host.persist(saved_at_secs).await.map_err(|e| e.to_string())?;
    ledger.record_review(review).await.map_err(|e| format!("{e:?}"))?;
    let member = app
        .host
        .graph()
        .surface_ids_showing_resource(resource)
        .first()
        .copied()
        .ok_or("no object shows the tagged page")?;
    Ok((
        match admission {
            TagAdmission::Added => Accepted::Added,
            TagAdmission::AlreadyHeld => Accepted::AlreadyHeld,
        },
        member,
    ))
}

/// One suggestion as the panel shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct SuggestionRow {
    /// The suggestion's content hash, 64 hex digits.
    pub id: String,
    pub address: String,
    pub title: Option<String>,
    pub concept: String,
    pub producer: String,
    pub run: String,
    pub version: String,
    pub keeping: &'static str,
    pub score_permille: Option<u16>,
    pub evidence: Vec<Snippet>,
    pub review: Review,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Review {
    Unreviewed,
    Accepted { by: String },
    Rejected { by: String },
}

impl Review {
    pub fn label(&self) -> String {
        match self {
            Self::Unreviewed => "Not reviewed".into(),
            Self::Accepted { by } => format!("Accepted by {by}"),
            Self::Rejected { by } => format!("Rejected by {by}"),
        }
    }
}

fn keeping_label(tier: KeepingTier) -> &'static str {
    match tier {
        KeepingTier::Runtime => "runtime",
        KeepingTier::Rebuildable => "rebuildable",
        KeepingTier::Retained => "retained",
    }
}

/// The ledger's suggestions with their review state, newest run last. A
/// suggestion for a page that is no longer retained shows its address as
/// `unknown` and no evidence rather than disappearing.
pub fn rows(held: &Held, pages: &RetainedPages, facts: &GraphFacts) -> Vec<SuggestionRow> {
    held.suggestions
        .iter()
        .map(|s| {
            let described = pages.describe(&s.subject);
            let text = pages.text_of(&s.subject, &s.subject_version);
            let label = facts
                .concepts()
                .iter()
                .find(|c| c.iri == s.concept.concept && c.owner == s.concept.owner)
                .map_or_else(|| s.concept.concept.clone(), |c| c.label.clone());
            let id = s.id();
            SuggestionRow {
                id: id.to_hex(),
                address: described.as_ref().map_or_else(|| "unknown page".into(), |d| d.0.clone()),
                title: described.and_then(|d| d.1),
                concept: label,
                producer: match &s.producer.kind {
                    SuggestionProducerKind::Rule { name } => format!("{name} {}", s.producer.operation_version),
                    SuggestionProducerKind::Lexical => format!("lexical {}", s.producer.operation_version),
                    SuggestionProducerKind::Model { model, revision } => format!("{model}@{revision}"),
                },
                run: s.run.clone(),
                version: s.subject_version.chars().take(12).collect(),
                keeping: keeping_label(s.keeping),
                score_permille: s.score.map(|score| (score * 1000.0).round() as u16),
                evidence: text
                    .map(|text| {
                        s.evidence
                            .iter()
                            .filter_map(|e| snippet(text, e.start, e.end, CONTEXT_CHARS))
                            .collect()
                    })
                    .unwrap_or_default(),
                review: match review_state(&id, &held.reviews) {
                    ReviewState::Unreviewed => Review::Unreviewed,
                    ReviewState::Accepted { reviewer, .. } => Review::Accepted { by: reviewer },
                    ReviewState::Rejected { reviewer, .. } => Review::Rejected { by: reviewer },
                },
            }
        })
        .collect()
}
