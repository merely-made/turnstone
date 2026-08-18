> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-16 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/graphshell_docs/research/2026-04-09_smolweb_discovery_and_aggregation_signal_model.md`
> - Source commit: `24101567`
>
> None of it was ever built. It is filed here as research, not as a plan and
> not as a statement of where Turnstone is going. It speaks the donor
> vocabulary (Graphshell as the browser, Middlenet for the smolweb) and its
> relative links point at donor docs that no longer exist locally. The current
> position on this ground is `2026-08-17_smolweb_browser_gap_analysis.md` in
> this folder. Read the text below for the ideas, not the direction.

---

<!-- This Source Code Form is subject to the terms of the Mozilla Public
     License, v. 2.0. If a copy of the MPL was not distributed with this
     file, You can obtain one at https://mozilla.org/MPL/2.0/. -->

# Smolweb Discovery and Aggregation Signal Model

**Date**: 2026-04-09
**Status**: Research-to-architecture follow-on note
**Purpose**: Define the distinct signal lanes implied by CAPCOM, Antenna,
Cosmos, Spacewalk, GUS, and Wander so Graphshell can grow user-visible
Middlenet discovery without collapsing all external inputs into one vague
"aggregator" concept.

**Related docs**:

- [`2026-04-09_smolweb_graph_enrichment_and_accessibility_note.md`](2026-04-09_smolweb_graph_enrichment_and_accessibility_note.md)
- [`2026-04-09_smolweb_browser_capability_gaps.md`](2026-04-09_smolweb_browser_capability_gaps.md)
- [`../technical_architecture/2026-04-09_identity_convergence_and_person_node_model.md`](../technical_architecture/2026-04-09_identity_convergence_and_person_node_model.md)
- [`../technical_architecture/2026-04-09_graphshell_verse_uri_scheme.md`](../technical_architecture/2026-04-09_graphshell_verse_uri_scheme.md)
- [`../technical_architecture/unified_view_model.md`](../technical_architecture/unified_view_model.md)
- [`../technical_architecture/graphlet_model.md`](../technical_architecture/graphlet_model.md)

---

## 1. Why This Note Exists

Graphshell already knows about a useful set of external smolweb services and
practices:

- CAPCOM,
- Antenna,
- Cosmos,
- Spacewalk,
- GUS,
- Wander.

The gap is no longer awareness. The gap is product shape.

If Graphshell flattens all of these into one undifferentiated "aggregator"
surface, it loses the most useful thing they teach:

- discovery is not freshness,
- freshness is not clustering,
- clustering is not search,
- search is not neighborhood traversal.

This note defines those lanes explicitly.

---

## 2. Core Position

Graphshell should treat smolweb discovery as a **multi-signal graph enrichment
problem**, not as a single ranking feed.

The five core signal families are:

1. **Discovery**: what exists and might be worth following.
2. **Freshness**: what changed recently.
3. **Clustering**: what belongs together as one thread or constellation.
4. **Search**: what matches an explicit query.
5. **Neighborhood traversal**: what becomes visible by walking trusted or
   selected local recommendation paths.

Every imported result should retain signal provenance instead of being merged
into one opaque score.

---

## 3. Signal Families and Example Sources

### 3.1 Discovery

Primary question:

- what feeds, capsules, boards, or sources should the user know exist?

Representative sources:

- CAPCOM,
- Spacewalk,
- curated discovery packs,
- future community-maintained source lists.

Expected output shape:

- candidate subscriptions,
- source nodes,
- wayfinding surfaces,
- provenance on where the suggestion came from.

### 3.2 Freshness

Primary question:

- what changed recently enough that the user may care now?

Representative sources:

- Antenna,
- recency-aware feed polling,
- source-health and update pipelines.

Expected output shape:

- recent-update surfaces,
- source health views,
- stale/empty/broken signals distinct from discovery.

### 3.3 Clustering

Primary question:

- which items belong to one thread, issue space, or local world?

Representative sources:

- Cosmos,
- Bubble-like reply/reference networks,
- future cross-source citation/reply grouping.

Expected output shape:

- constellation projections,
- thread graphlets,
- anchor/frontier relationships,
- local-world navigation views.

### 3.4 Search

Primary question:

- what best matches this explicit query right now?

Representative sources:

- GUS-like search services,
- future local or hybrid search indexes.

Expected output shape:

- result sets with engine provenance,
- query context,
- index freshness/coverage information.

### 3.5 Neighborhood Traversal

Primary question:

- what becomes discoverable by walking recommendation neighborhoods rather than
  running a direct query?

Representative sources:

- Wander,
- future community neighborhood graphs,
- trusted-console walk paths.

Expected output shape:

- explainable walk trails,
- discovery provenance by path,
- bounded random-walk or neighborhood exploration surfaces.

---

## 4. Product Rule: Keep Signals Distinct

Graphshell should not flatten discovery inputs into one generic "smolweb feed"
unless the user explicitly asks for a blended view.

Separate by default:

- discovery candidates,
- subscriptions,
- recent updates,
- clustered threads/constellations,
- search results,
- neighborhood exploration.

Blended views are allowed later, but only if they remain explainable and retain
signal provenance.

---

## 5. Provenance Requirements

Every surfaced item should be able to answer at least:

- why am I seeing this?
- which signal family surfaced it?
- which source or engine contributed it?
- when was it observed or indexed?
- is this a candidate, a subscribed item, a clustered relation, or a search
  hit?

This is required both for user trust and for Graphshell's graph-native product
identity.

---

## 6. Graph Object Implications

This signal model implies at least four object classes around external sources:

1. **Source node**.
   A feed, capsule, board, engine, or neighborhood source.
2. **Imported content node**.
   A post, entry, page, issue item, or other discovered artifact.
3. **Signal/provenance record**.
   Why the item was surfaced, through which lane, and when.
4. **User subscription or follow state**.
   The user's explicit continuing relationship to the source.

Graphshell should resist collapsing all of these into one imported page node.

---

## 7. User-Visible Middlenet Growth Track

This note defines the first architectural slice for user-visible Middlenet
growth.

Recommended order:

1. discovery/freshness/clustering/search/neighborhood taxonomy,
2. source-node and subscription-state model,
3. discovery packs,
4. recent-update/source-health surfaces,
5. constellation projection prototype,
6. search provenance surfaces.

This sequence produces useful browsing growth before Graphshell commits to a
large unified ranking system.

---

## 8. Immediate Follow-On Deliverables

The next notes or implementation-facing follow-ons should be:

1. a source/subscription manager note,
2. a discovery-pack manifest note,
3. a constellation projection note,
4. provenance UI rules for surfaced discovery results,
5. retention rules for imported/discovered artifacts vs saved items.

The key architectural discipline is simple: **separate the signals, preserve
their provenance, and let Graphshell combine them deliberately rather than by
accident.**

---

## 9. Implementation Slices

### Slice A: Signal Taxonomy in Data Shape

- add canonical signal-family labels for discovery, freshness, clustering,
   search, and neighborhood traversal,
- ensure surfaced results retain lane provenance,
- prevent imported results from collapsing into a single generic aggregator
   shape.

### Slice B: Source and Subscription Split

- define source objects separately from user subscription state,
- preserve candidate-versus-subscribed semantics,
- ensure freshness and discovery lanes can both point at the same source
   without becoming the same relation.

### Slice C: Lane-Specific Surfaces

- expose discovery candidates, recent updates, search results, and clustered
   constellations as distinct surfaces,
- permit blended views only if lane provenance remains visible,
- ensure user questions differ cleanly by surface.

### Slice D: Provenance UI

- show why a result is visible,
- show which engine, walk path, or source surfaced it,
- show whether the item is a candidate, clustered artifact, search hit, or
   subscribed-source update.

---

## 10. Validation

### Manual

1. Verify a discovered source candidate is not presented as an active
    subscription by default.
2. Verify a recent-update surface explains freshness without pretending to be a
    discovery ranking surface.
3. Verify search results expose engine provenance and index context.
4. Verify neighborhood-walk results show an explainable path rather than a
    generic score.

### Contract

- every surfaced item can answer which signal family surfaced it,
- source nodes, imported artifacts, and provenance/signal records remain
   separable,
- blended views do not erase original lane identity.

---

## 11. Done Gate

This note is implemented at the architectural floor when:

- the five signal families exist as explicit product lanes,
- provenance is preserved per surfaced item,
- at least one user-visible surface exists for discovery, freshness, and one of
   clustering/search/neighborhood traversal,
- and Graphshell no longer relies on a vague single "aggregator" concept.