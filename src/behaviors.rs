// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The behavior drain: turning committed deltas into wakes, and running them.
//!
//! `servitor::watch` decides *who* wakes and `servitor::cascade` decides *how
//! far* a wake travels. Neither knows what a mere graph is. This module is the
//! adapter between them and this application: it reads the journal tail, gives
//! each entry the scopes it touched, and runs the woken bodies through the
//! ordinary `RunDenizen` lane.
//!
//! **A node's scope is its containment ancestry** (the region vocabulary ruled
//! 2026-08-13). Mere's nodes are keyed by `Uuid`, which is one opaque segment
//! and so cannot nest on its own, but `EdgeFamily::Containment` already relates
//! them: a node under a folder, a URL path, a domain, a collection. Writing
//! that ancestry as a `ScopePath` of ids (`container/member`) makes
//! segment-prefix coverage mean what it should, with no change to `Cap`.
//!
//! Three properties of that walk, each of which surprised the reading:
//!
//! - **Containment points from the member to the container.** The kernel
//!   asserts `assert_relation(child, parent, Containment)` (see
//!   `rebuild_derived_containment`), so ancestry follows *outgoing* edges.
//!   Walking incoming ones would build the tree upside down and quietly invert
//!   every watch.
//! - **A node has several ancestries, not one.** The same node is related to
//!   both its URL-path parent and its domain anchor, so it belongs to more than
//!   one region at once. Each is its own path, which is why a `WatchEvent`
//!   carries a slice of scopes.
//! - **A container is addressed in directory form.** The kernel's rule names a
//!   parent as `https://host/inbox/`, with the trailing slash, so a folder node
//!   stored as `https://host/inbox` is a *different address* and nothing is
//!   ever contained by it. A watch on the slashless form is inert and says
//!   nothing about why, which is what it cost to find. First thing to check
//!   when a watch looks asleep.
//! - **A removed node has no ancestry left to walk.** Its containment edges
//!   went with it, so it falls back to its bare id: one segment, matching an
//!   exact-node watch or the root and nothing else. Stated rather than hidden,
//!   because "delete stopped waking the folder's watcher" is otherwise a
//!   mystery.

use std::collections::{HashMap, HashSet};

use mere::kernel::graph::Graph;
use mere::kernel::graph::capture::CapturedDelta;
use servitor::cascade::{Cascade, CascadeBudget, CascadeOutcome, CommittedEntry, run_cascade};
use servitor::{AuthorityProvider, Mode, ScopePath, Subject};

use crate::action::Effect;
use crate::app::App;
use crate::observe::AppEvent;

/// What woke a body, handed to it as its run's context.
///
/// A digest of the matched entries rather than the deltas themselves: a
/// behavior needs to know *which nodes moved under its watch*, and handing it
/// the raw delta vocabulary would couple every body to the kernel's 44
/// variants and to their evolution. Nodes and attribution are the durable
/// part of the answer.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct TriggerContext {
    /// The entries that matched this body's watch, in journal order.
    pub woken_by: Vec<TriggerEntry>,
}

/// One matched entry, as a body sees it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct TriggerEntry {
    /// Journal position.
    pub seq: u64,
    /// Who committed it. A body can tell a user's edit from another
    /// behavior's, which is what makes "answer people, ignore machines"
    /// expressible.
    pub author: String,
    /// The node ids the entry touched.
    pub nodes: Vec<String>,
}

impl TriggerContext {
    /// Whether anything woke this run. A manually invoked body has an empty
    /// context rather than a missing one, so a script can always ask.
    pub fn is_empty(&self) -> bool {
        self.woken_by.is_empty()
    }

    /// The wire form handed to a body, mirroring how the snapshot travels.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"woken_by\":[]}".to_string())
    }
}

/// How deep a containment walk may go before it stops looking.
///
/// Containment is asserted per relation and nothing forbids a cycle, so the
/// walk carries a visited set; this bound is the second belt, against a chain
/// so long that building its scope costs more than the match is worth.
const MAX_ANCESTRY_DEPTH: usize = 32;

/// Surface ids named directly by a delta. Resource ids need graph projection.
///
/// Exhaustive by construction: a new `CapturedDelta` variant fails to compile
/// here until it is classified, which is the same discipline `ring_of` uses
/// for actions. A variant that silently touched nothing would be a behavior
/// that silently stopped waking.
pub fn touched_ids(delta: &CapturedDelta) -> Vec<&str> {
    use CapturedDelta as D;
    match delta {
        D::ReplayAddNodeWithIdIfMissing { id, .. } => vec![id.as_str()],
        D::ReplaySetShownResourceById { surface_id, .. } => vec![surface_id.as_str()],

        // Resource UUIDs are not Surface UUIDs and must not become bare scopes.
        // touched_surface_ids resolves these through the graph explicitly.
        D::ReplaySetResourceRecordById { .. } | D::ReplaySetResourceEdgesByIds { .. } => Vec::new(),

        D::ReplayRemoveNodeById { node_id, .. }
        | D::ReplaySetNodeTitleById { node_id, .. }
        | D::ReplaySetNodeUrlById { node_id, .. }
        | D::ReplaySetNodeImageById { node_id, .. }
        | D::ReplaySetNodeThumbnailById { node_id, .. }
        | D::ReplaySetNodeFaviconById { node_id, .. }
        | D::ReplaySetNodeMimeHintById { node_id, .. }
        | D::ReplaySetNodeContentById { node_id, .. }
        | D::ReplaySetNodeNestedById { node_id, .. }
        | D::ReplaySetNodePinnedById { node_id, .. }
        | D::ReplaySetNodeFacetById { node_id, .. }
        | D::ReplayRemoveNodeFacetById { node_id, .. }
        | D::ReplayInsertNodeTagById { node_id, .. }
        | D::ReplayRemoveNodeTagById { node_id, .. }
        | D::ReplaySetNodeBodyById { node_id, .. }
        | D::ReplayNavigateNodeById { node_id, .. }
        | D::ReplayNodeHistoryBackById { node_id, .. }
        | D::ReplayNodeHistoryForwardById { node_id, .. }
        | D::ReplayAppendNodePropertyById { node_id, .. }
        | D::ReplayAddNodeClassificationById { node_id, .. }
        | D::ReplayRemoveNodeClassificationById { node_id, .. }
        | D::ReplaySetNodeClassificationStatusById { node_id, .. }
        | D::ReplaySetNodePrimaryClassificationById { node_id, .. }
        | D::ReplayRecordNodeDerivationById { node_id, .. }
        | D::ReplaySetNodeTagIconOverrideById { node_id, .. }
        | D::ReplayAppendFrameLayoutHintById { node_id, .. }
        | D::ReplayRemoveFrameLayoutHintById { node_id, .. }
        | D::ReplayMoveFrameLayoutHintById { node_id, .. }
        | D::ReplaySetFrameSplitOfferSuppressedById { node_id, .. }
        | D::ReplayUpdateNodeHistoryById { node_id, .. }
        | D::ReplayTouchNodeLastVisitedById { node_id, .. } => vec![node_id.as_str()],

        D::ReplayAssertRelationByIds { from_id, to_id, .. }
        | D::ReplayRetractRelationsByIds { from_id, to_id, .. }
        | D::ReplayAppendTraversalByIds { from_id, to_id, .. }
        | D::ReplaySetEdgesByIds { from_id, to_id, .. }
        | D::ReplaySetEdgeSemanticPredicateByIds { from_id, to_id, .. }
        | D::ReplayAssertSemanticPredicateByIds { from_id, to_id, .. } => {
            vec![from_id.as_str(), to_id.as_str()]
        },

        D::ReplayBranchHistoryByIds {
            child_id,
            parent_id,
        } => vec![child_id.as_str(), parent_id.as_str()],

        // Session-level and field/coupling deltas name no node. The physics
        // tier is deliberately outside this vocabulary: spatial influence
        // reaches the graph through fields, never through a petition, so a
        // field change is not a thing a behavior is woken by.
        D::ReplaySetImportRecords { .. }
        | D::ReplayAddField { .. }
        | D::ReplayRetireFieldById { .. }
        | D::ReplayRemoveFieldById { .. }
        | D::ReplayAddCoupling { .. }
        | D::ReplaySetFieldCouplingStrengthByFieldId { .. }
        | D::ReplayActivateFieldById { .. }
        | D::ReplayRetractCouplingById { .. } => Vec::new(),
    }
}

/// Surfaces affected in the graph state visible to the after-dispatch drain.
///
/// Record writes reach every current view of the resource. A tag concept's
/// record also changes the tag label projected onto the resources that point
/// to it with taggedWith. Only that exact predicate adds this dependency;
/// arbitrary incoming semantic relations do not propagate content changes.
///
/// Pair replacements name both resource endpoints even after retraction has
/// removed the relation. No Resource UUID is used as a Surface scope fallback.
/// This does not reconstruct mappings or ancestry from before the dispatch.
fn touched_surface_ids(graph: &Graph, delta: &CapturedDelta) -> Vec<String> {
    use CapturedDelta as D;
    use mere::kernel::graph::resource::TAGGED_WITH_IRI;

    let mut resources = HashSet::new();
    match delta {
        D::ReplaySetResourceRecordById { resource_id, .. } => {
            if let Ok(id) = resource_id.parse::<uuid::Uuid>() {
                resources.insert(id);
                // Test the relation itself, not the concept's current facet:
                // removing the facet also changes its incoming tag projections.
                for (from, to, payload) in graph.resource_edges() {
                    if to.id() == id
                        && payload
                            .semantic_statements()
                            .iter()
                            .any(|statement| statement.predicate == TAGGED_WITH_IRI)
                    {
                        resources.insert(from.id());
                    }
                }
            }
        },
        D::ReplaySetResourceEdgesByIds {
            from_resource_id,
            to_resource_id,
            ..
        } => {
            for id in [from_resource_id, to_resource_id] {
                if let Ok(id) = id.parse::<uuid::Uuid>() {
                    resources.insert(id);
                }
            }
        },
        // Exhaustive classification is retained in touched_ids above.
        _ => {},
    }
    let mut surfaces: Vec<String> = touched_ids(delta)
        .into_iter()
        .map(str::to_owned)
        .chain(resources.into_iter().flat_map(|id| {
            graph
                .surface_ids_showing_resource(id)
                .into_iter()
                .map(|id| id.to_string())
        }))
        .collect();
    surfaces.sort_unstable();
    surfaces.dedup();
    surfaces
}

/// Every containment ancestry of `id`, each as a root-first scope path ending
/// in the node itself.
///
/// A node with no container is its own single-segment scope, which is also
/// what a node the graph no longer holds falls back to.
pub fn ancestry_scopes(graph: &Graph, id: &str) -> Vec<ScopePath> {
    let Ok(uuid) = id.parse::<uuid::Uuid>() else {
        return Vec::new();
    };
    let Some(key) = graph.get_node_key_by_id(uuid) else {
        // Removed, or never present: the bare id is all there is to say.
        return ScopePath::parse(id).into_iter().collect();
    };

    // Containment runs member -> container, so a node's containers are its
    // outgoing containment targets.
    let mut paths: Vec<Vec<String>> = Vec::new();
    let mut frontier: Vec<(Vec<String>, mere::kernel::graph::NodeKey, HashSet<String>)> = {
        let mut seen = HashSet::new();
        seen.insert(id.to_string());
        vec![(vec![id.to_string()], key, seen)]
    };

    while let Some((path, at, seen)) = frontier.pop() {
        // URL/domain/filesystem containment belongs to Resource relations;
        // user folders and collections remain Surface relations. Project both
        // into the Surface vocabulary watches already use.
        let mut containers: Vec<_> = graph
            .projected_outgoing_relations(at)
            .filter(|(_, _, payload)| {
                payload
                    .containment_data()
                    .is_some_and(|data| !data.sub_kinds.is_empty())
            })
            .map(|(container, _, _)| container)
            .collect();
        containers.sort_unstable_by_key(|key| graph.get_node(*key).map(|node| node.id));
        containers.dedup();
        let mut grew = false;
        for container in containers {
            let Some(node) = graph.get_node(container) else {
                continue;
            };
            let container_id = node.id.to_string();
            if seen.contains(&container_id) || path.len() >= MAX_ANCESTRY_DEPTH {
                // A cycle, or deeper than a scope is worth. The path so far is
                // still a real region, so it is kept rather than dropped.
                continue;
            }
            let mut next = path.clone();
            next.push(container_id.clone());
            let mut seen = seen.clone();
            seen.insert(container_id);
            frontier.push((next, container, seen));
            grew = true;
        }
        if !grew {
            paths.push(path);
        }
    }

    // Built leaf-first while walking; a scope reads outermost-first.
    paths
        .into_iter()
        .filter_map(|mut path| {
            path.reverse();
            ScopePath::parse(&path.join("/")).ok()
        })
        .collect()
}

/// The scope prefix every app-tier watch sits under.
///
/// `app/<event-name>`, so `app` alone covers every app event and a name covers
/// one. It cannot collide with a graph scope: those are UUID segments, and no
/// UUID is the literal string `app`.
pub const APP_SCOPE_ROOT: &str = "app";

/// The scope an app event answers to: `app/<its kebab name>`.
///
/// The name is `describe`'s first token, which is that method's shape for all
/// 52 variants. Read from it rather than duplicated into a second 52-arm match
/// that could disagree with the one the transcript already shows the user.
pub fn app_event_scope(event: &AppEvent) -> Option<ScopePath> {
    let described = event.describe();
    let name = described.split_whitespace().next()?;
    ScopePath::parse(&format!("{APP_SCOPE_ROOT}/{name}")).ok()
}

/// The app events the drain has not considered, as cascade inputs.
///
/// Their sequence is the queue ordinal, a different counter from the journal's,
/// which is why the app tier has its own [`WatchTable`](servitor::WatchTable).
/// `author_from` marks where a woken body's own events begin, so a behavior
/// cannot be woken by the events it just caused.
fn app_entries(app: &App, author_from: Option<(usize, &str)>) -> Vec<CommittedEntry> {
    let base = app.events_len() - app.unseen_events().len();
    app.unseen_events()
        .iter()
        .enumerate()
        .filter_map(|(offset, event)| {
            let index = base + offset;
            let author = match author_from {
                Some((from, subject)) if index >= from => subject,
                _ => mere::kernel::graph::USER_AUTHOR,
            };
            let scope = app_event_scope(event)?;
            let ordinal = app.events_base() + index as u64 + 1;
            Some(CommittedEntry::new(ordinal, author, vec![scope]))
        })
        .collect()
}

/// Snapshot the complete tail boundary, projecting only the loaded runtime.
fn projected_tail(
    app: &App,
    origin: crate::host_journal::RuntimeOrigin,
    cursor: u64,
) -> Option<(u64, Vec<CommittedEntry>)> {
    if app.behavior_origin() != Some(origin) {
        return None;
    }
    let tail = app.journal.lock().ok()?.tail(origin, cursor);
    let graph = app.graph_runtimes.canvas(origin.graph)?.graph();
    let entries = tail
        .entries
        .iter()
        .map(|entry| {
            let scopes = touched_surface_ids(graph, &entry.delta)
                .into_iter()
                .flat_map(|id| ancestry_scopes(graph, &id))
                .collect();

            CommittedEntry::new(entry.seq, entry.author.id.clone(), scopes)
        })
        .collect();
    Some((tail.high_water, entries))
}

/// The entries after `cursor` belonging to the loaded authority context.
/// Focus and the legacy active-canvas cursor cannot supply capture origin.
pub fn entries_since(app: &App, cursor: u64) -> Vec<CommittedEntry> {
    let Some(origin) = app.behavior_origin() else {
        return Vec::new();
    };
    projected_tail(app, origin, cursor)
        .map(|(_, entries)| entries)
        .unwrap_or_default()
}

fn binding_matches(app: &App, origin: crate::host_journal::RuntimeOrigin) -> bool {
    app.behavior_execution_origin() == Some(origin)
}

fn routing_refused(app: &mut App) {
    app.refuse_behavior("automatic participant run refused: loaded session/runtime binding changed or is unavailable".into());
}

/// Run the behavior cascade for whatever has been committed since last time.
///
/// The after-dispatch drain: called once per action, after the action's own
/// effects are decided, so a woken body sees the world the action left rather
/// than the one it found.
pub fn drain(app: &mut App) -> Vec<Effect> {
    if app.session_load_refused() || app.draining {
        return Vec::new();
    }
    let error = match app.journal.lock() {
        Ok(journal) => journal.execution_error().map(|error| error.to_string()),
        Err(_) => Some("behavior journal capture lock poisoned".into()),
    };
    if let Some(error) = error {
        app.refuse_behavior(error);
        return vec![Effect::Redraw];
    }
    if app.denizens.is_empty() {
        return Vec::new();
    }
    let Some(origin) = app.behavior_execution_origin() else {
        routing_refused(app);
        return vec![Effect::Redraw];
    };
    app.denizens.authority.set_now(crate::denizen::now_ms());
    app.draining = true;
    let mut effects = drain_time_tier(app, origin);
    if binding_matches(app, origin) {
        effects.extend(drain_app_tier(app, origin));
    }
    if binding_matches(app, origin) && !app.watches.is_empty() {
        effects.extend(drain_graph_tier(app, origin));
    }
    app.draining = false;
    if binding_matches(app, origin) && !effects.is_empty() {
        crate::denizen::save_watches(
            &app.session_dir(),
            &app.watches,
            &app.app_watches,
            &app.time_watches,
            &app.deadbands,
        );
    }
    effects
}

/// The clock tier: wake whatever is due.
///
/// No cascade here. A tick is caused by the clock rather than by a commit, so
/// there is nothing for a woken body to feed back into: what it *writes* goes
/// to the journal, which the graph drain picks up in the same beat. One pass,
/// stable order, nothing to bound.
fn drain_time_tier(app: &mut App, origin: crate::host_journal::RuntimeOrigin) -> Vec<Effect> {
    if app.time_watches.is_empty() {
        return Vec::new();
    }
    // A host with no clock fires nothing, rather than treating "no time" as
    // time zero and running every schedule at once.
    let Some(now_ms) = app.now_ms else {
        return Vec::new();
    };
    let due = app.time_watches.due(now_ms);
    let mut effects = Vec::new();
    for subject in due {
        if !binding_matches(app, origin) {
            routing_refused(app);
            effects.push(Effect::Redraw);
            break;
        }
        app.denizens.authority.set_now(crate::denizen::now_ms());
        let Some(member) = member_of(app, subject, &[]) else {
            app.record_event(AppEvent::DenizenRefused(
                "clock behavior wake refused by current subject routing or read authority".into(),
            ));
            effects.push(Effect::Redraw);
            continue;
        };
        // Nothing woke it but the clock, so its context is empty: a scheduled
        // body has no matched entries to read, and saying so is truer than
        // handing it the last thing that happened to change.
        effects.extend(app.run_denizen_for_clock(member, now_ms));
        if !binding_matches(app, origin) {
            routing_refused(app);
            effects.push(Effect::Redraw);
            break;
        }
    }
    effects
}

/// The app tier: wake on what the application did, rather than on what the
/// graph recorded. Runs first, so anything a woken body writes to the graph is
/// picked up by the graph drain in the same beat.
fn drain_app_tier(app: &mut App, origin: crate::host_journal::RuntimeOrigin) -> Vec<Effect> {
    if app.app_watches.is_empty() {
        app.mark_events_seen();
        return Vec::new();
    }
    let entries = app_entries(app, None);
    app.mark_events_seen();
    if entries.is_empty() {
        return Vec::new();
    }
    let budget = CascadeBudget::new(app.cascade_budget);
    let mut effects: Vec<Effect> = Vec::new();
    let mut watches = std::mem::take(&mut app.app_watches);
    let scoped_reads = scoped_reads(&watches);
    let mut round_entries = entries.clone();
    let cascade = run_cascade(&mut watches, budget, entries, |wakes| {
        let mut produced = Vec::new();
        for wake in wakes {
            if !binding_matches(app, origin) {
                routing_refused(app);
                effects.push(Effect::Redraw);
                produced.clear();
                break;
            }
            let required = scoped_reads
                .get(&wake.subject)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            app.denizens.authority.set_now(crate::denizen::now_ms());
            let Some(member) = member_of(app, wake.subject, required) else {
                app.record_event(AppEvent::DenizenRefused(
                    "behavior wake refused by current subject routing or read authority".into(),
                ));
                effects.push(Effect::Redraw);
                continue;
            };
            let context = context_for(&round_entries, wake);
            let hex = wake.subject.to_hex();
            let before = app.events_len();
            effects.extend(app.run_denizen_for_cascade(member, &context, required));
            if !binding_matches(app, origin) {
                routing_refused(app);
                effects.push(Effect::Redraw);
                produced.clear();
                break;
            }
            // Whatever the body just caused is attributed to it, so it cannot
            // be woken by its own noise.
            produced.extend(app_entries(app, Some((before, &hex))));
            app.mark_events_seen();
        }
        round_entries = produced.clone();
        produced
    });
    let current = app.behavior_execution_origin();
    if crate::host_journal::restore_table(&mut app.app_watches, watches, origin, current) {
        report(app, &cascade);
    }
    effects
}

fn drain_graph_tier(app: &mut App, origin: crate::host_journal::RuntimeOrigin) -> Vec<Effect> {
    let Some((boundary, entries)) = projected_tail(app, origin, app.behavior_cursor) else {
        return Vec::new();
    };
    // A foreign-only tail is consumed without constructing a local trigger.
    app.behavior_cursor = boundary;
    if entries.is_empty() {
        return Vec::new();
    }

    let budget = CascadeBudget::new(app.cascade_budget);
    let mut effects = Vec::new();
    let mut watches = std::mem::take(&mut app.watches);
    let scoped_reads = scoped_reads(&watches);
    let mut round_entries = entries.clone();
    let cascade = run_cascade(&mut watches, budget, entries, |wakes| {
        let mut produced = Vec::new();
        for wake in wakes {
            if !binding_matches(app, origin) {
                routing_refused(app);
                effects.push(Effect::Redraw);
                produced.clear();
                break;
            }
            let required = scoped_reads
                .get(&wake.subject)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            app.denizens.authority.set_now(crate::denizen::now_ms());
            let Some(member) = member_of(app, wake.subject, required) else {
                app.record_event(AppEvent::DenizenRefused(
                    "behavior wake refused by current subject routing or read authority".into(),
                ));
                effects.push(Effect::Redraw);
                continue;
            };
            let context = context_for(&round_entries, wake);
            let before = journal_high_water(app);
            effects.extend(app.run_denizen_for_cascade(member, &context, required));
            if !binding_matches(app, origin) {
                routing_refused(app);
                effects.push(Effect::Redraw);
                produced.clear();
                break;
            }

            if let Some((boundary, entries)) = projected_tail(app, origin, before) {
                app.behavior_cursor = boundary;
                produced.extend(entries);
            }
        }
        round_entries = produced.clone();
        produced
    });
    // A body may have loaded another session or replaced this same graph.
    // Its newly loaded table is authoritative; never put the old one over it.
    let current = app.behavior_execution_origin();
    if crate::host_journal::restore_table(&mut app.watches, watches, origin, current) {
        report(app, &cascade);
    }
    effects
}

/// The digest of what woke one body: the entries its wake named, in order.
pub fn context_for(entries: &[CommittedEntry], wake: &servitor::Wake) -> TriggerContext {
    let woken_by = wake
        .matched
        .iter()
        .filter_map(|seq| entries.iter().find(|entry| entry.seq == *seq))
        .map(|entry| TriggerEntry {
            seq: entry.seq,
            author: entry.author.clone(),
            // The scope's last segment is the node itself: ancestry is written
            // outermost-first, so the tail is what actually changed.
            nodes: entry
                .scopes
                .iter()
                .filter_map(|scope| scope.segments().last().cloned())
                .collect(),
        })
        .collect();
    TriggerContext { woken_by }
}

/// Which resident node holds `subject`.
///
/// The mapping is deliberately unique. A duplicate subject is malformed
/// session state, not an invitation to let hash iteration choose which body
/// receives a journal payload. Read authority is checked before that payload
/// is constructed, then the run lane adds world-write admission.
fn member_of(
    app: &App,
    subject: Subject,
    scoped_reads: &[(servitor::Cap, Mode)],
) -> Option<uuid::Uuid> {
    let members: Vec<_> = app
        .denizens
        .residents
        .iter()
        .filter(|(_, resident)| resident.subject == subject)
        .map(|(member, _)| *member)
        .collect();
    let readable = app
        .denizens
        .authority
        .covers(subject, &crate::denizen::read_cap(), Mode::Read)
        && scoped_reads
            .iter()
            .all(|(cap, mode)| app.denizens.authority.covers(subject, cap, *mode));
    if members.len() != 1 || !readable {
        tracing::warn!(
            subject = %subject.to_hex(),
            count = members.len(),
            "behavior subject routing or live read authority refused"
        );
        return None;
    }
    members.into_iter().next()
}

fn scoped_reads(table: &servitor::WatchTable) -> HashMap<Subject, Vec<(servitor::Cap, Mode)>> {
    let mut reads = HashMap::new();
    for watch in table.watches() {
        reads
            .entry(watch.subject)
            .or_insert_with(Vec::new)
            .push((servitor::Cap::Scope(watch.scope.clone()), Mode::Read));
    }
    reads
}

fn journal_high_water(app: &App) -> u64 {
    match app.journal.lock() {
        Ok(journal) => journal.high_water(),
        Err(poisoned) => poisoned.into_inner().high_water(),
    }
}

/// Say what the cascade did, loudly when it hit the budget.
fn report(app: &mut App, cascade: &Cascade) {
    app.denizens.authority.set_now(crate::denizen::now_ms());
    if let CascadeOutcome::BudgetExhausted { still_waking } = &cascade.outcome {
        let names: Vec<String> = still_waking
            .iter()
            .filter_map(|subject| member_of(app, *subject, &[]))
            .filter_map(|member| {
                app.denizens
                    .residents
                    .get(&member)
                    .map(|resident| resident.label.clone())
            })
            .collect();
        let named = if names.is_empty() {
            "unnamed behaviors".to_string()
        } else {
            names.join(", ")
        };
        tracing::warn!(
            rounds = cascade.rounds.len(),
            %named,
            "behavior cascade hit its budget"
        );
        app.record_event(AppEvent::CascadeExhausted(named));
    } else if !cascade.rounds.is_empty() {
        tracing::debug!(rounds = cascade.rounds.len(), "behavior cascade settled");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_journal_authors_preserve_no_self_wake_subject_matching() {
        use mere::kernel::graph::Author;
        let app = App::test_stub();
        let subject = Subject::new([0x31; 32]);
        let node = uuid::Uuid::new_v4().to_string();
        let body_author = Author::script(subject.to_hex(), "body-1").via("turnstone");
        {
            let mut journal = app.journal.lock().unwrap();
            for author in [Author::user(), body_author.clone()] {
                journal
                    .record_as(
                        app.behavior_binding.unwrap(),
                        author,
                        CapturedDelta::ReplaySetNodeTitleById {
                            node_id: node.clone(),
                            title: "changed".into(),
                        },
                    )
                    .unwrap();
            }
        }
        let entries = entries_since(&app, 0);
        let watch = servitor::Watch {
            subject,
            scope: ScopePath::parse(&node).unwrap(),
            self_author: subject.to_hex(),
            cursor: 0,
        };
        assert!(
            watch.matches(&entries[0].as_event()),
            "the human edit wakes the body"
        );
        assert!(
            !watch.matches(&entries[1].as_event()),
            "the body's own edit never wakes itself"
        );
        assert_eq!(entries[1].author, subject.to_hex());
        assert_eq!(
            app.journal.lock().unwrap().entries()[1].author,
            body_author,
            "the string matcher projection leaves full attribution in the journal"
        );
    }

    #[test]
    fn a_delta_naming_one_node_yields_that_node() {
        let delta = CapturedDelta::ReplaySetNodeTitleById {
            node_id: "n1".into(),
            title: "t".into(),
        };
        assert_eq!(touched_ids(&delta), vec!["n1"]);
    }

    #[test]
    fn a_relation_delta_yields_both_ends() {
        let delta = CapturedDelta::ReplayBranchHistoryByIds {
            child_id: "child".into(),
            parent_id: "parent".into(),
        };
        assert_eq!(touched_ids(&delta), vec!["child", "parent"]);
        let undo = CapturedDelta::ReplaySetEdgesByIds {
            from_id: "child".into(),
            to_id: "parent".into(),
            edges: Vec::new(),
        };
        assert_eq!(touched_ids(&undo), vec!["child", "parent"]);
    }

    #[test]
    fn a_field_delta_names_no_node() {
        // The physics tier is outside the behavior vocabulary on purpose.
        let delta = CapturedDelta::ReplaySetImportRecords {
            import_records: Vec::new(),
        };
        assert!(touched_ids(&delta).is_empty());
        assert!(
            touched_ids(&CapturedDelta::ReplayRemoveFieldById {
                field_id: "field".into(),
            })
            .is_empty()
        );
    }

    #[test]
    fn a_node_the_graph_does_not_hold_falls_back_to_its_bare_id() {
        let graph = Graph::new();
        let id = uuid::Uuid::new_v4().to_string();
        let scopes = ancestry_scopes(&graph, &id);
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].segments(), &[id]);
    }

    #[test]
    fn a_digest_names_the_nodes_that_changed_not_their_containers() {
        // Ancestry is written outermost-first, so the tail of each scope is
        // the node that actually moved. A body wants that, not the folder.
        let entries = vec![
            CommittedEntry::new(4, "user", vec![ScopePath::parse("folder/leaf").unwrap()]),
            CommittedEntry::new(5, "user", vec![ScopePath::parse("elsewhere").unwrap()]),
        ];
        let wake = servitor::Wake {
            subject: Subject::new([1; 32]),
            matched: vec![4],
        };
        let context = context_for(&entries, &wake);
        assert_eq!(context.woken_by.len(), 1, "only the matched entry");
        assert_eq!(context.woken_by[0].seq, 4);
        assert_eq!(context.woken_by[0].nodes, vec!["leaf".to_string()]);
        assert_eq!(context.woken_by[0].author, "user");
    }

    #[test]
    fn an_unwoken_context_is_empty_rather_than_absent() {
        let context = TriggerContext::default();
        assert!(context.is_empty());
        assert_eq!(context.to_json(), r#"{"woken_by":[]}"#);
    }

    #[test]
    fn a_malformed_id_yields_no_scope_rather_than_a_bogus_one() {
        let graph = Graph::new();
        assert!(ancestry_scopes(&graph, "not-a-uuid").is_empty());
    }

    fn resource_views() -> (
        Graph,
        mere::kernel::graph::NodeKey,
        mere::kernel::graph::NodeKey,
    ) {
        use mere::kernel::graph::apply::add_node;
        let mut graph = Graph::new();
        let a = add_node(
            &mut graph,
            None,
            "https://example.test/page#one".into(),
            Default::default(),
        );
        let b = add_node(
            &mut graph,
            None,
            "https://example.test/page#two".into(),
            Default::default(),
        );
        assert_eq!(graph.shown_resource_id(a), graph.shown_resource_id(b));
        (graph, a, b)
    }

    fn surface_names(graph: &Graph, keys: &[mere::kernel::graph::NodeKey]) -> Vec<String> {
        let mut ids: Vec<_> = keys
            .iter()
            .map(|key| graph.get_node(*key).unwrap().id.to_string())
            .collect();
        ids.sort_unstable();
        ids
    }

    #[test]
    fn resource_record_wakes_each_current_view_without_resource_uuid_scopes() {
        let (graph, a, b) = resource_views();
        let resource = graph.shown_resource_id(a).unwrap();
        let delta = CapturedDelta::ReplaySetResourceRecordById {
            resource_id: resource.to_string(),
            record: None,
        };
        assert!(touched_ids(&delta).is_empty());
        assert_eq!(
            touched_surface_ids(&graph, &delta),
            surface_names(&graph, &[a, b])
        );
        assert!(!touched_surface_ids(&graph, &delta).contains(&resource.to_string()));
        assert!(
            touched_surface_ids(&Graph::new(), &delta).is_empty(),
            "a detached Resource never becomes a bare Surface watch scope"
        );
    }

    #[test]
    fn concept_record_changes_wake_incoming_tag_sources_even_after_facet_removal() {
        use mere::kernel::graph::apply::{
            GraphDelta, add_node, apply_graph_delta, assert_semantic_predicate_in_scope,
        };
        use mere::kernel::graph::resource::ResourceNode;
        use mere::kernel::graph::resource_tags::TagConcept;
        use mere::kernel::persistence::PersistedResourceRecord;
        let (mut graph, a, b) = resource_views();
        let resource = graph.shown_resource_id(a).unwrap();
        let concept_iri = "https://vocabulary.test/tags#Research";
        assert!(
            graph
                .tag_resource_with_concept(
                    resource,
                    concept_iri,
                    TagConcept {
                        owner_iri: "urn:author:alice".into(),
                        label: "Research".into(),
                    }
                )
                .unwrap()
        );
        let concept = ResourceNode::for_term(concept_iri).id();
        let concept_view = add_node(
            &mut graph,
            None,
            "https://example.test/concept".into(),
            Default::default(),
        );
        let concept_surface = graph.get_node(concept_view).unwrap().id;
        apply_graph_delta(
            &mut graph,
            GraphDelta::ReplaySetShownResourceById {
                surface_id: concept_surface,
                resource_id: Some(concept),
            },
        );
        let unrelated = add_node(
            &mut graph,
            None,
            "https://unrelated.test/page".into(),
            Default::default(),
        );
        assert!(
            assert_semantic_predicate_in_scope(
                &mut graph,
                unrelated,
                concept_view,
                "https://unrelated.test/taggedWith".into(),
                Default::default()
            )
            .is_some()
        );
        let record = PersistedResourceRecord {
            canonical_iri: concept_iri.into(),
            facets: Vec::new(),
        };
        apply_graph_delta(
            &mut graph,
            GraphDelta::ReplaySetResourceRecordById {
                resource_id: concept,
                record: Some(record.clone()),
            },
        );
        assert!(graph.resource_tag_concept(concept).is_none());
        assert!(graph.node_content_tags(a).unwrap().is_empty());
        let delta = CapturedDelta::ReplaySetResourceRecordById {
            resource_id: concept.to_string(),
            record: Some(record),
        };
        assert_eq!(
            touched_surface_ids(&graph, &delta),
            surface_names(&graph, &[a, b, concept_view]),
            "incoming exact taggedWith sources and the concept view wake; arbitrary incoming edges do not"
        );
    }

    #[test]
    fn retracted_resource_pair_still_wakes_both_endpoint_views_once() {
        use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta};
        use mere::kernel::graph::resource::ResourceNode;
        use mere::kernel::graph::resource_tags::TagConcept;
        let (mut graph, a, b) = resource_views();
        let resource = graph.shown_resource_id(a).unwrap();
        let concept_iri = "https://vocabulary.test/tags#Research";
        graph
            .tag_resource_with_concept(
                resource,
                concept_iri,
                TagConcept {
                    owner_iri: "urn:author:alice".into(),
                    label: "Research".into(),
                },
            )
            .unwrap();
        let concept = ResourceNode::for_term(concept_iri).id();
        let c = add_node(
            &mut graph,
            None,
            "https://example.test/concept-view".into(),
            Default::default(),
        );
        let c_id = graph.get_node(c).unwrap().id;
        apply_graph_delta(
            &mut graph,
            GraphDelta::ReplaySetShownResourceById {
                surface_id: c_id,
                resource_id: Some(concept),
            },
        );
        let delta = CapturedDelta::ReplaySetResourceEdgesByIds {
            from_resource_id: resource.to_string(),
            to_resource_id: concept.to_string(),
            edges: Vec::new(),
        };
        apply_graph_delta(&mut graph, delta.replay_delta().unwrap());
        assert!(
            graph
                .resource_edges()
                .all(|(from, to, _)| from.id() != resource || to.id() != concept)
        );
        assert_eq!(
            touched_surface_ids(&graph, &delta),
            surface_names(&graph, &[a, b, c])
        );
        let self_pair = CapturedDelta::ReplaySetResourceEdgesByIds {
            from_resource_id: resource.to_string(),
            to_resource_id: resource.to_string(),
            edges: Vec::new(),
        };
        assert_eq!(
            touched_surface_ids(&graph, &self_pair),
            surface_names(&graph, &[a, b])
        );
    }

    #[test]
    fn shown_resource_changes_keep_the_surface_even_after_detach_or_removal() {
        use mere::kernel::graph::apply::{GraphDelta, apply_graph_delta};
        let (mut graph, a, b) = resource_views();
        let resource = graph.shown_resource_id(a).unwrap();
        let a_id = graph.get_node(a).unwrap().id;
        let record = CapturedDelta::ReplaySetResourceRecordById {
            resource_id: resource.to_string(),
            record: None,
        };
        let detach = CapturedDelta::ReplaySetShownResourceById {
            surface_id: a_id.to_string(),
            resource_id: None,
        };
        apply_graph_delta(&mut graph, detach.replay_delta().unwrap());
        assert_eq!(
            touched_surface_ids(&graph, &record),
            surface_names(&graph, &[b]),
            "Resource records project through final state, not old mappings"
        );
        assert_eq!(touched_surface_ids(&graph, &detach), vec![a_id.to_string()]);
        apply_graph_delta(
            &mut graph,
            GraphDelta::ReplayRemoveNodeById { node_id: a_id },
        );
        let ids = touched_surface_ids(&graph, &detach);
        assert_eq!(ids, vec![a_id.to_string()]);
        assert_eq!(
            ancestry_scopes(&graph, &ids[0])[0].segments(),
            &[a_id.to_string()]
        );
    }

    #[test]
    fn resource_projection_preserves_journal_author_and_no_self_wake() {
        use mere::kernel::graph::Author;
        let (graph, a, b) = resource_views();
        let resource = graph.shown_resource_id(a).unwrap();
        let expected = surface_names(&graph, &[a, b]);
        let mut app = App::test_stub();
        let graph_id = app.graph_runtimes.active_graph();
        app.graph_runtimes.activate_or_insert(
            graph_id,
            Some(app.session_id),
            mere::canvas::Canvas::with_graph(graph),
        );
        app.bind_behavior_journal().unwrap();
        let cursor = journal_high_water(&app);
        let subject = Subject::new([0x42; 32]);
        let author = Author::script(subject.to_hex(), "resource-body").via("turnstone");
        {
            let mut journal = app.journal.lock().unwrap();
            journal
                .record_as(
                    app.behavior_binding.unwrap(),
                    author.clone(),
                    CapturedDelta::ReplaySetResourceRecordById {
                        resource_id: resource.to_string(),
                        record: None,
                    },
                )
                .unwrap();
        }
        let entries = entries_since(&app, cursor);
        assert_eq!(entries.len(), 1);
        let wake = servitor::Wake {
            subject,
            matched: vec![cursor + 1],
        };
        assert_eq!(context_for(&entries, &wake).woken_by[0].nodes, expected);
        for surface in &expected {
            let watch = servitor::Watch {
                subject,
                scope: ScopePath::parse(surface).unwrap(),
                self_author: subject.to_hex(),
                cursor,
            };
            assert!(!watch.matches(&entries[0].as_event()));
        }
        assert_eq!(entries[0].author, subject.to_hex());
        assert_eq!(
            app.journal.lock().unwrap().entries().last().unwrap().author,
            author
        );
    }

    #[test]
    fn resource_and_surface_containment_both_contribute_current_watch_ancestry() {
        use mere::kernel::graph::apply::{add_node, assert_relation};
        use mere::kernel::graph::{ContainmentSubKind, EdgeAssertion};
        let (mut graph, a, _) = resource_views();
        let parent = add_node(
            &mut graph,
            None,
            "https://containers.test/collection#one".into(),
            Default::default(),
        );
        let parent_alias = add_node(
            &mut graph,
            None,
            "https://containers.test/collection#two".into(),
            Default::default(),
        );
        let folder = add_node(
            &mut graph,
            None,
            "https://folders.test/surface".into(),
            Default::default(),
        );
        assert_relation(
            &mut graph,
            a,
            parent,
            EdgeAssertion::Containment {
                sub_kind: ContainmentSubKind::UrlPath,
            },
        )
        .unwrap();
        assert_relation(
            &mut graph,
            a,
            folder,
            EdgeAssertion::Containment {
                sub_kind: ContainmentSubKind::UserFolder,
            },
        )
        .unwrap();
        let leaf = graph.get_node(a).unwrap().id.to_string();
        let scopes = ancestry_scopes(&graph, &leaf);
        for key in [parent, parent_alias, folder] {
            let parent_id = graph.get_node(key).unwrap().id.to_string();
            assert!(
                scopes
                    .iter()
                    .any(|scope| scope.segments() == &[parent_id.clone(), leaf.clone()]),
                "both views of the resource container and the Surface folder are distinct regions"
            );
        }
    }
}
