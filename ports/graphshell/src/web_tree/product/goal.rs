// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The goal surface and tag suggestions on the browser mere.
//!
//! A goal reads what this profile's IndexedDB retains (`goal_surface`): page
//! text when something stored it, otherwise the title and address of each
//! captured object. It runs in either view, since it only reads. Tag
//! suggestions are written to the suggestion ledger in the same store by the
//! writer alone; a follower shows them and what was decided, read-only.
use super::*;
use cambium::{button, el, lens};
use graphshell::goal_surface::suggest::{self, Held, Review, SuggestionRow};
use graphshell::goal_surface::{GoalSurface, Snippet};
use graphshell_host::goal_surface::{GraphFacts, Origin, RetainedPages};
use std::collections::BTreeSet;

/// The most retained text one load reads.
const MAX_BYTES: usize = 4 << 20;

pub(in crate::web_tree) const SHEET: &str = "\
    .goal-surface { display:flex; flex-direction:column; gap:8px; margin:0 12px 8px; } \
    .goal-surface label { display:flex; flex-direction:row; align-items:center; gap:8px; } \
    .goal-starts { display:flex; flex-wrap:wrap; gap:4px; } \
    .goal-surface button[aria-pressed=true] { font-weight:bold; border-color:var(--tabard-color-focus, #d4e6b9); } \
    .goal-snippet { white-space:pre-wrap; } \
    .goal-surface mark, .goal-suggestions mark { display:inline-block; background:var(--tabard-color-tertiary, #f0c674); color:#1b1b1b; } \
    .goal-surface ol, .goal-suggestions ol { margin:0; padding-left:22px; } \
    .goal-surface li, .goal-suggestions li { margin-bottom:8px; } \
    .goal-surface p, .goal-suggestions p, .goal-note { margin:2px 0; overflow-wrap:anywhere; } \
    .goal-meta { font-size:12px; color:var(--tabard-color-text, #dce3e8); } \
    .goal-notice { color:var(--tabard-color-tertiary, #f0c674); } \
    .goal-cite { padding:6px; border:1px solid var(--tabard-color-border, #405560); } \
    .goal-suggestions { display:flex; flex-direction:column; gap:8px; margin:8px 12px; } \
    .goal-actions { display:flex; flex-direction:row; flex-wrap:wrap; gap:8px; } \
    .goal-note { margin:2px 12px; }";

enum Done {
    Loaded {
        pages: RetainedPages,
        facts: GraphFacts,
        held: Held,
    },
    Held {
        held: Held,
        note: String,
    },
    Failed(String),
}

pub(super) struct GoalPanel {
    pub(super) open: bool,
    owner: String,
    store: IndexedDbBackend,
    pub(super) surface: Option<GoalSurface>,
    facts: GraphFacts,
    held: Held,
    rows: Vec<SuggestionRow>,
    /// A read of the store is wanted: the panel was opened or refreshed.
    stale: bool,
    loading: bool,
    /// A suggestion write is in flight.
    busy: bool,
    status: String,
    completed: Rc<RefCell<Option<Done>>>,
}

impl GoalPanel {
    pub(super) fn new(store: IndexedDbBackend, owner: String) -> Self {
        Self {
            open: false,
            owner,
            store,
            surface: None,
            facts: GraphFacts::default(),
            held: Held::default(),
            rows: Vec::new(),
            stale: false,
            loading: false,
            busy: false,
            status: String::new(),
            completed: Rc::default(),
        }
    }

    /// Whether the goal fields are on screen.
    pub(super) fn shown(&self) -> bool {
        self.open && self.surface.is_some()
    }

    pub(super) fn input(&self, index: usize) -> &TextInput {
        let surface = self.surface.as_ref().expect("goal surface");
        [&surface.goal, &surface.max_visits, &surface.max_depth][index]
    }

    pub(super) fn input_mut(&mut self, index: usize) -> &mut TextInput {
        let surface = self.surface.as_mut().expect("goal surface");
        [&mut surface.goal, &mut surface.max_visits, &mut surface.max_depth][index]
    }

    fn rebuild_rows(&mut self) {
        if let Some(surface) = &self.surface {
            self.rows = suggest::rows(&self.held, surface.pages(), &self.facts);
        }
    }

    fn apply(&mut self, done: Done) {
        self.loading = false;
        self.busy = false;
        match done {
            Done::Loaded { pages, facts, held } => {
                let records = pages.sources().iter().filter(|(_, p)| p.origin == Origin::Record).count();
                let total = pages.sources().len();
                let intro = if records == total {
                    format!(
                        "Reads {total} captured records: the title and address of each object this profile kept. \
                         No page text was retained, so a goal can match only those words. \
                         Nothing is fetched and the graph is not changed."
                    )
                } else {
                    format!(
                        "Reads {total} retained pages ({} with stored text, {records} title and address records) \
                         and the links between them. Nothing is fetched and the graph is not changed.",
                        total - records
                    )
                };
                let pages = Rc::new(pages);
                match &mut self.surface {
                    Some(surface) => surface.replace_pages(pages),
                    None => {
                        let mut surface = GoalSurface::new(pages);
                        surface.heading = false;
                        self.surface = Some(surface);
                    },
                }
                if let Some(surface) = &mut self.surface {
                    surface.intro = Some(intro);
                }
                self.facts = facts;
                self.held = held;
                self.rebuild_rows();
                // A decision's note stays on screen across the read it triggers.
                if self.status.starts_with("Reading") {
                    self.status =
                        format!("Read {total} retained records · {} tag suggestion(s) kept", self.rows.len());
                }
            },
            Done::Held { held, note } => {
                self.held = held;
                self.rebuild_rows();
                self.status = note;
            },
            Done::Failed(error) => self.status = error,
        }
    }

    fn answer(completed: &Rc<RefCell<Option<Done>>>, done: Done) {
        *completed.borrow_mut() = Some(done);
    }
}

fn micros() -> i64 {
    (js_sys::Date::now() * 1000.0) as i64
}

impl SavedProduct {
    /// A finished read or write waits, or a refresh can start.
    pub(in crate::web_tree) fn goal_ready(&self) -> bool {
        self.goal.completed.borrow().is_some()
            || (self.goal.open
                && self.goal.stale
                && !self.goal.loading
                && !self.goal.busy
                && self.app.is_some())
    }

    /// Take a finished read or write, or start the read a refresh asked for.
    pub(super) fn goal_poll(&mut self) -> bool {
        let done = self.goal.completed.borrow_mut().take();
        if let Some(done) = done {
            self.goal.apply(done);
            return true;
        }
        if !self.goal.open || !self.goal.stale || self.goal.loading || self.goal.busy {
            return false;
        }
        let Some(app) = self.app.as_ref() else {
            return false;
        };
        let facts = GraphFacts::read(app.host.graph());
        self.goal.stale = false;
        self.goal.loading = true;
        self.goal.status = "Reading what is retained…".into();
        let (owner, store, completed) = (
            self.goal.owner.clone(),
            self.goal.store.clone(),
            self.goal.completed.clone(),
        );
        self.spawn_storage_task(async move {
            let done = match RetainedPages::load_facts(&owner, &store, &facts, MAX_BYTES, true).await {
                Err(error) => Done::Failed(format!("Could not read the retained records · {error}")),
                Ok(pages) => match suggest::read(store).await {
                    Ok(held) => Done::Loaded { pages, facts, held },
                    Err(error) => Done::Failed(format!("Could not read the suggestion ledger · {error}")),
                },
            };
            GoalPanel::answer(&completed, done);
        });
        true
    }

    /// **Suggest tags**: propose the graph's existing tag concepts for the
    /// pages the last goal cited, or for every retained page.
    fn goal_suggest(&mut self, cited_only: bool) {
        if let Err(error) = self.view_only() {
            self.goal.status = error;
            return;
        }
        if self.goal.busy || self.goal.loading {
            return;
        }
        let Some(surface) = &self.goal.surface else {
            return;
        };
        let scope: Option<BTreeSet<_>> = if cited_only {
            let cited: BTreeSet<_> = surface
                .run
                .iter()
                .flat_map(|run| run.matches())
                .map(|step| step.source.clone())
                .collect();
            if cited.is_empty() {
                self.goal.status = "Find something first: there is no cited result to suggest tags for".into();
                return;
            }
            Some(cited)
        } else {
            None
        };
        let run = format!("browser-{}", js_sys::Date::now() as u64);
        let produced = match suggest::produce(surface.pages(), &self.goal.facts, scope.as_ref(), &run) {
            Ok(produced) => produced,
            Err(error) => {
                self.goal.status = format!("Suggesting tags failed · {error}");
                return;
            },
        };
        if produced.suggestions.is_empty() {
            self.goal.status = format!(
                "Run {run} proposed nothing: {} page(s) read against {} tag concept(s), and none names a concept the page lacks",
                produced.sources, produced.concepts
            );
            return;
        }
        self.goal.busy = true;
        self.goal.status = format!("Keeping {} suggestion(s)…", produced.suggestions.len());
        let (store, completed) = (self.goal.store.clone(), self.goal.completed.clone());
        self.spawn_storage_task(async move {
            let proposed = produced.suggestions.len();
            let done = match suggest::record(store.clone(), produced.suggestions).await {
                Err(error) => Done::Failed(format!("Suggestions were not kept · {error}")),
                Ok(new) => match suggest::read(store).await {
                    Ok(held) => Done::Held {
                        held,
                        note: format!("Run {run}: {proposed} proposed, {new} new, kept in the ledger"),
                    },
                    Err(error) => Done::Failed(format!("Could not read the suggestion ledger · {error}")),
                },
            };
            GoalPanel::answer(&completed, done);
        });
    }

    /// **Accept** adds your own tag citing the suggestion; **Reject** records
    /// the decision and changes no tag.
    fn goal_review(&mut self, id: String, accept: bool) {
        if let Err(error) = self.view_only() {
            self.goal.status = error;
            return;
        }
        if self.goal.busy || self.goal.loading {
            return;
        }
        let (store, completed, owner) = (
            self.goal.store.clone(),
            self.goal.completed.clone(),
            self.goal.owner.clone(),
        );
        if !accept {
            let Some(app) = self.app.as_ref() else {
                return;
            };
            let reviewer = suggest::reviewer(app);
            self.goal.busy = true;
            self.goal.status = "Recording the rejection…".into();
            self.spawn_storage_task(async move {
                let done = match suggest::reject_one(store.clone(), &id, &reviewer, micros()).await {
                    Err(error) => Done::Failed(format!("Rejection was not recorded · {error}")),
                    Ok(()) => match suggest::read(store).await {
                        Ok(held) => Done::Held {
                            held,
                            note: "Rejected · recorded as a review; no tag changed".into(),
                        },
                        Err(error) => Done::Failed(format!("Could not read the suggestion ledger · {error}")),
                    },
                };
                GoalPanel::answer(&completed, done);
            });
            return;
        }
        if self.selection_locked() {
            self.goal.status = "Another save is pending · try again".into();
            return;
        }
        let Some(mut app) = self.app.take() else {
            return;
        };
        self.goal.busy = true;
        self.saving = true;
        self.save_state = "saving";
        self.status = "Saving an accepted tag…".into();
        let completed_app = self.completed.clone();
        // The task owns the app. No RefCell borrow crosses the IndexedDB await.
        self.spawn_storage_task(async move {
            let accepted = suggest::accept_one(&mut app, store.clone(), &owner, &id, micros(), now_secs()).await;
            let (done, outcome) = match accepted {
                Err(error) => (
                    Done::Failed(format!("Not accepted · {error}")),
                    (Uuid::nil(), Err(error)),
                ),
                Ok((admission, member)) => {
                    let note = match admission {
                        suggest::Accepted::Added => "Accepted · your tag was added, citing the suggestion, and saved",
                        suggest::Accepted::AlreadyHeld => "Accepted · you already held this tag; the review is recorded",
                    };
                    let done = match suggest::read(store).await {
                        Ok(held) => Done::Held { held, note: note.into() },
                        Err(error) => Done::Failed(format!("Could not read the suggestion ledger · {error}")),
                    };
                    (done, (member, metadata(&app, member)))
                },
            };
            GoalPanel::answer(&completed, done);
            *completed_app.borrow_mut() = Some((app, CompletedEdit::Tag(outcome.0), outcome.1));
        });
    }

    /// An accepted tag has been stored, or has failed.
    pub(super) fn finish_tag(
        &mut self,
        canvas: &mut Canvas,
        member: Uuid,
        result: Result<NodeMetadata, String>,
    ) {
        match result {
            Ok(saved) => {
                let refreshed = sync_canvas_metadata_from_graph(
                    canvas,
                    self.app.as_ref().expect("completed app").host.graph(),
                    &saved,
                );
                if self.selected == Some(member) {
                    self.title = TextInput::new(saved.title);
                    self.tags = TextInput::new(saved.tags.join(", "));
                }
                self.status = match refreshed {
                    Ok(()) => "Tag added from a suggestion · saved".into(),
                    Err(error) => format!("Tag saved · canvas refresh failed: {error}"),
                };
                self.save_state = "saved";
            },
            Err(error) => {
                self.status = format!("Suggestion not accepted · {error}");
                self.save_state = "error";
            },
        }
    }

    /// The writer saved elsewhere or a follower reread: read the store again.
    pub(in crate::web_tree) fn goal_stale(&mut self) {
        self.goal.stale = true;
    }
}

fn mark(before: &str, hit: &str, after: &str) -> Child {
    Box::new(
        el(
            "p",
            (
                el("span", format!("…{before}")),
                el("mark", hit.to_owned()),
                el("span", format!("{after}…")),
            ),
        )
        .attr("class", "goal-snippet"),
    )
}

fn evidence(snippets: &[Snippet]) -> Vec<Child> {
    snippets.iter().map(|s| mark(&s.before, &s.hit, &s.after)).collect()
}

fn row(r: &SuggestionRow, writer: bool) -> Child {
    let name = r.title.clone().unwrap_or_else(|| r.address.clone());
    let score = r.score_permille.map_or(String::new(), |s| format!(" · match {}%", f64::from(s) / 10.0));
    let mut children: Vec<Child> = vec![
        Box::new(el("p", format!("Tag \"{}\" for {name}", r.concept))),
        Box::new(
            el(
                "p",
                format!(
                    "{} · version {} · producer {} · run {} · kept {}{score}",
                    r.address, r.version, r.producer, r.run, r.keeping
                ),
            )
            .attr("class", "goal-meta"),
        ),
    ];
    children.extend(evidence(&r.evidence));
    children.push(Box::new(el("p", r.review.label()).attr("class", "goal-review")));
    if writer && r.review == Review::Unreviewed {
        let (accept_id, reject_id) = (r.id.clone(), r.id.clone());
        children.push(Box::new(
            el(
                "div",
                (
                    button("Accept", move |page: &mut TreePage, _| {
                        if let Some(product) = &mut page.product {
                            product.goal_review(accept_id.clone(), true);
                        }
                        page.shared.dirty.set(true);
                    })
                    .attr("aria-label", format!("Accept {} for {name}", r.concept)),
                    button("Reject", move |page: &mut TreePage, _| {
                        if let Some(product) = &mut page.product {
                            product.goal_review(reject_id.clone(), false);
                        }
                        page.shared.dirty.set(true);
                    })
                    .attr("aria-label", format!("Reject {} for {name}", r.concept)),
                ),
            )
            .attr("class", "goal-actions"),
        ));
    }
    Box::new(
        el("li", children)
            .attr("class", "goal-cite")
            .attr("role", "group")
            .attr(
                "aria-label",
                format!("Suggestion · {} · {name} · {} · run {}", r.concept, r.review.label(), r.run),
            )
            .attr("id", format!("goal-suggestion-{}", &r.id[..12])),
    )
}

fn suggestions(product: &SavedProduct) -> Child {
    let writer = !product.following();
    let goal = &product.goal;
    let mut children: Vec<Child> = vec![
        Box::new(el("h3", "Tag suggestions")),
        Box::new(
            el(
                "p",
                "Proposals from the tag concepts this graph already holds. Nothing is tagged until you accept one; \
                 accepting adds your own tag citing it, and rejecting records the decision only.",
            )
            .attr("class", "goal-meta"),
        ),
    ];
    if writer {
        children.push(Box::new(
            el(
                "div",
                (
                    button("Suggest tags for cited results", |page: &mut TreePage, _| {
                        if let Some(product) = &mut page.product {
                            product.goal_suggest(true);
                        }
                        page.shared.dirty.set(true);
                    }),
                    button("Suggest tags for all retained pages", |page: &mut TreePage, _| {
                        if let Some(product) = &mut page.product {
                            product.goal_suggest(false);
                        }
                        page.shared.dirty.set(true);
                    }),
                ),
            )
            .attr("class", "goal-actions"),
        ));
    } else {
        children.push(Box::new(
            el("p", "Viewing only · suggestions are made and reviewed in the view that edits.")
                .attr("class", "goal-meta"),
        ));
    }
    if goal.rows.is_empty() {
        children.push(Box::new(el("p", "No suggestions are kept yet.").attr("id", "goal-no-suggestions")));
    } else {
        children.push(Box::new(
            el("ol", goal.rows.iter().map(|r| row(r, writer)).collect::<Vec<_>>())
                .attr("id", "goal-tag-suggestions")
                .attr("aria-label", "Tag suggestions"),
        ));
    }
    Box::new(el("div", children).attr("class", "goal-suggestions"))
}

pub(in crate::web_tree) fn section(page: &TreePage) -> Child {
    let Some(product) = &page.product else {
        return Box::new(el("div", ()));
    };
    let goal = &product.goal;
    let mut children: Vec<Child> = vec![Box::new(disclosure_trigger(
        "Find in what I kept",
        goal.open,
        |page: &mut TreePage| {
            if let Some(product) = &mut page.product {
                product.goal.open = !product.goal.open;
                product.goal.stale |= product.goal.open && product.goal.surface.is_none();
            }
            page.shared.dirty.set(true);
        },
    ))];
    if goal.open {
        if goal.surface.is_some() {
            children.push(Box::new(lens(
                |surface: &mut GoalSurface| graphshell::goal_surface::view(surface),
                |page: &mut TreePage| {
                    page.product
                        .as_mut()
                        .and_then(|product| product.goal.surface.as_mut())
                        .expect("goal surface")
                },
            )));
        }
        children.push(Box::new(
            el("div", button("Read retained records again", |page: &mut TreePage, _| {
                if let Some(product) = &mut page.product {
                    product.goal.stale = true;
                }
                page.shared.dirty.set(true);
            }))
            .attr("class", "goal-actions"),
        ));
        if !goal.status.is_empty() {
            children.push(Box::new(
                el("p", goal.status.clone())
                    .attr("class", "goal-note")
                    .attr("id", "goal-status")
                    .attr("role", "status"),
            ));
        }
        if goal.surface.is_some() {
            children.push(suggestions(product));
        }
    }
    Box::new(el("section", children).attr("aria-label", "Find in what I kept"))
}
