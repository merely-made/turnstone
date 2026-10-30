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

/// How deep a containment walk may go before it stops looking.
///
/// Containment is asserted per relation and nothing forbids a cycle, so the
/// walk carries a visited set; this bound is the second belt, against a chain
/// so long that building its scope costs more than the match is worth.
const MAX_ANCESTRY_DEPTH: usize = 32;

/// The scope prefix every app-tier watch sits under.
///
/// `app/<event-name>`, so `app` alone covers every app event and a name covers
/// one. It cannot collide with a graph scope: those are UUID segments, and no
/// UUID is the literal string `app`.
pub const APP_SCOPE_ROOT: &str = "app";

#[cfg(test)]
#[path = "behaviors/tests.rs"]
mod tests;

#[path = "behaviors/touched_ids.rs"]
mod touched_ids;
#[path = "behaviors/trigger_context_is_empty.rs"]
mod trigger_context_is_empty;
pub use touched_ids::touched_ids;
#[path = "behaviors/touched_surface_ids.rs"]
mod touched_surface_ids;
use touched_surface_ids::touched_surface_ids;
#[path = "behaviors/ancestry_scopes.rs"]
mod ancestry_scopes;
pub use ancestry_scopes::ancestry_scopes;
#[path = "behaviors/app_event_scope.rs"]
mod app_event_scope;
pub use app_event_scope::app_event_scope;
#[path = "behaviors/app_entries.rs"]
mod app_entries;
use app_entries::app_entries;
#[path = "behaviors/projected_tail.rs"]
mod projected_tail;
use projected_tail::projected_tail;
#[path = "behaviors/entries_since.rs"]
mod entries_since;
pub use entries_since::entries_since;
#[path = "behaviors/binding_matches.rs"]
mod binding_matches;
use binding_matches::binding_matches;
#[path = "behaviors/routing_refused.rs"]
mod routing_refused;
use routing_refused::routing_refused;
#[path = "behaviors/drain.rs"]
mod drain;
pub use drain::drain;
#[path = "behaviors/drain_time_tier.rs"]
mod drain_time_tier;
use drain_time_tier::drain_time_tier;
#[path = "behaviors/drain_app_tier.rs"]
mod drain_app_tier;
use drain_app_tier::drain_app_tier;
#[path = "behaviors/drain_graph_tier.rs"]
mod drain_graph_tier;
use drain_graph_tier::drain_graph_tier;
#[path = "behaviors/context_for.rs"]
mod context_for;
pub use context_for::context_for;
#[path = "behaviors/member_of.rs"]
mod member_of;
use member_of::member_of;
#[path = "behaviors/scoped_reads.rs"]
mod scoped_reads;
use scoped_reads::scoped_reads;
#[path = "behaviors/journal_high_water.rs"]
mod journal_high_water;
use journal_high_water::journal_high_water;
#[path = "behaviors/report.rs"]
mod report;
use report::report;
