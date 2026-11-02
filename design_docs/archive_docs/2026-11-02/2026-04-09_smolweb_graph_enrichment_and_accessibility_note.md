> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-16 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/graphshell_docs/research/2026-04-09_smolweb_graph_enrichment_and_accessibility_note.md`
> - Source commit: `24101567`
>
> None of it was ever built. It is filed here as research, not as a plan and
> not as a statement of where Turnstone is going. It speaks the donor
> vocabulary (Graphshell as the browser, Middlenet for the smolweb) and its
> relative links point at donor docs that no longer exist locally. The current
> position on this ground is `2026-08-17_smolweb_browser_gap_analysis.md` in
> this folder. Read the text below for the ideas, not the direction.

---

# Smolweb Graph Enrichment and Accessibility Note

**Date**: 2026-04-09  
**Status**: Research note / backlog-shaping input with executable slices  
**Purpose**: Evaluate a set of smolweb-adjacent projects and protocol practices as concrete opportunities for Graphshell's graph, Navigator, history, and accessibility model.

**Related docs**:

- [`../technical_architecture/unified_view_model.md`](../technical_architecture/unified_view_model.md)
- [`../technical_architecture/graphlet_model.md`](../technical_architecture/graphlet_model.md)
- [`../technical_architecture/2026-03-29_middlenet_engine_spec.md`](../technical_architecture/2026-03-29_middlenet_engine_spec.md)
- [`../technical_architecture/2026-04-09_identity_convergence_and_person_node_model.md`](../technical_architecture/2026-04-09_identity_convergence_and_person_node_model.md)
- [`../technical_architecture/2026-04-09_graphshell_verse_uri_scheme.md`](../technical_architecture/2026-04-09_graphshell_verse_uri_scheme.md)
- [`../technical_architecture/2026-02-18_universal_node_content_model.md`](../technical_architecture/2026-02-18_universal_node_content_model.md)
- [`../implementation_strategy/navigator/NAVIGATOR.md`](../implementation_strategy/navigator/NAVIGATOR.md)
- [`../implementation_strategy/subsystem_accessibility/SUBSYSTEM_ACCESSIBILITY.md`](../implementation_strategy/subsystem_accessibility/SUBSYSTEM_ACCESSIBILITY.md)
- [`../implementation_strategy/subsystem_history/SUBSYSTEM_HISTORY.md`](../implementation_strategy/subsystem_history/SUBSYSTEM_HISTORY.md)
- [`2026-03-30_middlenet_vision_synthesis.md`](2026-03-30_middlenet_vision_synthesis.md)

**External references**:

- [Bubble: Bulletin Boards for Gemini](https://gmi.skyjake.fi/bubble/)
- [CAPCOM Geminispace aggregator](https://portal.mozz.us/gemini/gemini.circumlunar.space/capcom/?reader=1)
- [Announcing Antenna](https://warmedal.se/~bjorn/posts/announcing-antenna.html)
- [Announcing "Cosmos"](https://gmi.skyjake.fi/gemlog/2022-01_cosmos.gmi)
- [Joining Gemini Space and Reading Over Encrypted Connection Without Unnecessary Clutter](https://techrights.org/o/2021/04/28/joining-gemini-space/)
- [makew0rld/amfora v1.9.0 release notes](https://newreleases.io/project/github/makew0rld/amfora/release/v1.9.0)
- [Gemini Application Developer Guide v1.0.1, April 8th 2026](https://git.skyjake.fi/gemini/app-guide.git/)
- [Project Gemini FAQ](https://geminiprotocol.net/docs/faq.gmi)
- [atlas-engineer/history-tree](https://github.com/atlas-engineer/history-tree)
- [susam/wander](https://codeberg.org/susam/wander)
- [irchiver](https://irchiver.com/)

---

## 1. Executive Summary

These smolweb projects are relevant to Graphshell not because they are merely
interesting protocols, but because they expose already-shaped graph material:

- Bubble provides posts, replies, subspaces, and issue-tracker-like relations.
- CAPCOM provides broad feed aggregation and rotating discovery across active
  Gemini feeds.
- Antenna provides freshness-oriented feed aggregation driven by publisher
  submission rather than periodic polling.
- Cosmos provides thread grouping across multiple aggregators into explicit
  "constellations".
- Spacewalk provides another aggregator/discovery stream centered on following
  updates to pages it tracks.
- GUS provides search/index infrastructure for Gemini rather than feed-style
  aggregation.
- IRC provides a long-lived communication protocol with strong implications for
  live discussion, logging, and consent-bound archival.
- Wander provides a decentralized discovery graph made of neighborhood links and
  random-walk traversal.
- `history-tree` provides a useful temporal model for owner-specific forward
  paths and spawned browsing contexts.
- irchiver provides a strong precedent for local-first archive capture and
  searchable memory.

The main product conclusion is:

1. Graphshell should treat the smolweb as a graph enrichment opportunity, not
   only as a rendering problem.
2. Graphshell should preserve protocol faithfulness while adding optional
   assistive enrichment.
3. Markdown should remain the current inward-facing authored-content default,
   while HTML remains the richer long-term outward/publication and accessibility
   substrate.

### 1.1 Current Implementation Baseline (2026-04-09)

Some of the document/protocol foundation needed for this note is already live:

- Graphshell already has protocol-faithful Middlenet adapters for
  Gemini/gemtext, Gopher, Finger, RSS, Atom, JSON Feed, Markdown, and plain
  text.
- `viewer:middlenet` routing already exists on native desktop.
- Person-node convergence already covers WebFinger, NIP-05, Matrix, and
  ActivityPub actors, with provenance, freshness, and refresh UI.

The enrichment surfaces discussed here are mostly **not** implemented yet:

- no Bubble/Cosmos/CAPCOM/Antenna/Spacewalk/GUS integrations,
- no discovery packs,
- no constellation projection,
- no protocol-specific assistive lenses,
- no IRC/archive lane,
- no owner-scoped history branching derived from `history-tree`.

---

## 2. Why These Sources Matter

Graphshell already defines itself as a graph-first browser where:

- graph truth is primary,
- Navigator derives bounded local worlds and graphlets,
- history is temporal truth rather than a linear tab list,
- smallnet and middlenet content should be rendered faithfully,
- accessibility is a cross-cutting system requirement rather than a garnish.

This means the best smolweb opportunities are the ones that strengthen:

- graph ingestion,
- Navigator projections,
- temporal memory,
- discovery,
- accessibility,
- publication and archival loops.

These projects all contribute at least one of those.

---

## 3. Project-by-Project Relevance

### 3.1 Bubble

Bubble is especially relevant because it spans several content grammars that
Graphshell already cares about:

- personal publication feeds,
- shared forum posting,
- subspace/topic grouping,
- issue-tracker-like workflows,
- cross-references to commits and issues.

For Graphshell, Bubble is not just "support one Gemini app". It is a compact
example of what a graph-aware small-protocol social/document substrate can look
like.

Useful Graphshell implications:

- Treat Bubble posts, reply chains, subspaces, tags, and issue references as
  graph-bearing content rather than a flat page stream.
- Add Bubble-compatible starter bookmarks and discovery packs as optional seed
  content, not hardcoded defaults.
- Treat Bubble issue spaces as a bridge between Gemini-native discussion and
  broader workbench/project workflows.

### 3.2 Cosmos

Cosmos is arguably the cleanest immediate fit for Navigator and graphlets.

Its core move is to collect posts from multiple aggregators and group linked
discussion into "constellations". In Graphshell terms, that is already close to
an explicit derived graphlet:

- bounded thread world,
- anchor post,
- relation edges through reply/reference links,
- frontier expansion through related posts and newer thread members.

Useful Graphshell implications:

- A Cosmos import or adapter could feed Navigator "constellation" projections
  with relatively little additional conceptual wiring.
- Constellation layouts could become a first-class specialty navigation mode.
- Cosmos-style thread grouping should inform Graphshell's own cross-source
  threading model for RSS, Atom, gemlogs, Bubble posts, and later social lanes.

### 3.3 CAPCOM and Antenna

CAPCOM and Antenna are worth considering together because they cover adjacent
but distinct aggregation roles.

CAPCOM is a public aggregator of subscribable Gemini pages and Gemini Atom
feeds. Its public description emphasizes:

- a broad database of active feeds,
- rotating monthly selection for serendipitous discovery,
- feed submission and active-feed listing,
- compatibility with self-hosting the software.

Antenna takes a different approach. Its announcement describes it as a
freshness-oriented feed aggregator that does not continuously poll a remembered
set of feeds. Instead, publishers explicitly submit a feed URL when they update
their feed, and the service ingests that queue on a short interval. The same
announcement notes that Antenna was initially seeded from feeds known to
CAPCOM.

That difference is useful for Graphshell.

CAPCOM suggests:

- a broad feed-discovery input,
- rotating or sampled exposure for serendipity,
- a good source for "what exists?" and "what should I subscribe to?" style
  Navigator views.

Antenna suggests:

- a recency- and freshness-sensitive input,
- feed-update event semantics,
- a stronger model for "what changed recently?" than naive periodic polling.

Together with Cosmos, they imply a useful three-way split:

- CAPCOM: broad feed discovery,
- Antenna: timely recency aggregation,
- Cosmos: cross-source thread clustering.

Useful Graphshell implications:

- Treat feed discovery, feed freshness, and thread grouping as separate but
  composable graph-enrichment lanes.
- Use CAPCOM-like sources to populate a discovery graph and candidate
  subscriptions.
- Use Antenna-like sources to drive recent-activity and feed-frontier
  projections.
- Use Cosmos-like sources to cluster related posts into thread/constellation
  graphlets.

### 3.4 Spacewalk and GUS

Spacewalk and GUS are both worth considering, but they should be classified
carefully.

They are not protocols in the same sense as Gemini or IRC. They are services
within the Gemini ecosystem:

- Spacewalk is an aggregator/discovery surface.
- GUS ("Gemini Universal Search") is a search/index surface.

Public writeups describing Gemini hubs and aggregators place Spacewalk alongside
CAPCOM as a Geminispace aggregator, and describe it as following updates to the
pages it tracks. Later client release notes around Gemini search infrastructure
also note that `gus.guru` had been replaced in practice by `geminispace.info`
running the same codebase, which usefully underscores that GUS is a search
service layer rather than a protocol commitment.

This gives Graphshell another helpful split:

- CAPCOM / Antenna / Spacewalk: discovery and update aggregation lanes,
- Cosmos: thread/constellation clustering lane,
- GUS-style services: search and indexing lane.

Useful Graphshell implications:

- Treat search as a graph-enrichment input that differs from feed discovery.
- Preserve provenance on search hits: which engine, when indexed, and why a
  result was surfaced.
- Allow discovery, recency, clustering, and search to remain separate signals
  that can be composed in Navigator rather than flattened into one ranking
  model.

### 3.5 IRC

IRC is relevant, but unlike Spacewalk and GUS it is a protocol-level concern.

For Graphshell, IRC looks less like a document-rendering lane and more like a
communication, presence, and archive-adjacent lane:

- live channels,
- direct conversation surfaces,
- nick/channel/server relationships,
- links, references, and quoted excerpts that can become graph material,
- log artifacts when the user chooses to retain them.

Its strongest fit is therefore not "make IRC look like Gemini". Its strongest
fit is:

- live conversation surface,
- graphable references and excerpts,
- optional consent-bound archival and dataset workflows,
- interoperability with discovery and workbench research flows.

IRC also pairs naturally with the earlier irchiver idea. Together they suggest a
split between:

- live communication truth,
- locally retained excerpts or logs,
- derived searchable/archive artifacts.

### 3.6 `history-tree`

`history-tree` matters because it solves a closely related but not identical
problem to Graphshell's current history model.

Its strongest ideas are:

- never forgetting forward branches,
- modeling multiple owners over overlapping history,
- owner-specific default forward children,
- explicit spawned-owner relationships when opening from one browsing context
  into another.

Graphshell already rejects flat linear history. However, `history-tree`
suggests a useful refinement:

- panes, frames, or other workbench-owned browsing contexts can share temporal
  ancestry while keeping context-local forward semantics.

Useful Graphshell implications:

- Keep the current history subsystem authority, but add a research follow-on for
  owner-scoped forward-path semantics.
- Treat "open in new pane/frame/workbench context" as a temporal branching event
  with explicit parentage.
- Distinguish global temporal truth from owner-local continuation preference.

### 3.7 Wander

Wander looks simple at first glance, but it provides a valuable discovery
pattern:

- local recommendation lists,
- explicit neighborhood links to other consoles,
- random-walk browsing through a decentralized recommendation network,
- no central ranking authority.

That makes it relevant to Graphshell's discovery and graph-enrichment goals.

Useful Graphshell implications:

- Ingest Wander neighborhoods as a discoverability graph.
- Expose a "wander" mode in Navigator or Shell that performs bounded,
  explainable random walks through trusted or user-selected neighborhoods.
- Preserve provenance so the user can see not just what was found, but through
  which console or neighborhood path it arrived.

### 3.8 irchiver

irchiver is not a small-protocol project, but it is highly relevant as an
archive-memory precedent.

The strongest ideas are:

- local-first capture,
- preserving what the user actually saw,
- storing durable, simple output formats,
- making later retrieval and search a first-class outcome.

Useful Graphshell implications:

- Archival capture should be user-consented, legible, and local-first.
- Graphshell should only archive material that was actually served to the user
  and/or material whose participants have explicitly consented to saving.
- "What I saw" may be a distinct artifact class from fetched source content.
- Dataset-building features should keep privacy and explicit consent as hard
  boundaries, not afterthoughts.

---

## 4. HTML vs Markdown

Both formats matter. They do different jobs.

### 4.1 Current Graphshell position

Current docs already establish a clear policy:

- browsed content is rendered as itself,
- the shared middlenet document model is internal,
- Graphshell-authored content currently defaults to Markdown,
- HTML is part of the broader middlenet and full publication/rendering world.

That split is still sound as of 2026-04-09 and should not be changed
accidentally.

### 4.2 Comparison

| Concern | Markdown | HTML |
|---|---|---|
| Authoring friction | Very low | Higher |
| Readability in source form | Excellent | Moderate |
| Diffability and lightweight storage | Excellent | Good |
| Rich semantic structure | Limited | Strong |
| Accessibility surface area | Good when constrained and disciplined | Strongest mature option |
| Styling/layout control | Minimal by design | Rich |
| Embedding, media, and interaction hooks | Limited | Strong |
| Public interchange target | Good for documents and notes | Better for broad publication and compatibility |
| Fit for Graphshell notes/annotations/co-op docs today | Strong | Weaker as a default |
| Fit for long-term outward publication and assistive rendering | Partial | Strong |

### 4.3 Recommendation

Markdown should remain Graphshell's default inward-facing authored format for:

- notes,
- graph annotations,
- lightweight shared documents,
- versioned local content,
- low-friction composition.

HTML should be treated as the wiser long-term richer surface for:

- outward/public publication,
- accessibility-sensitive richer documents,
- layout-sensitive exports,
- assistive render targets,
- protocol-bridging publication where richer semantics are worth carrying.

Important boundary:

- This does **not** mean Graphshell should silently replace Markdown with HTML
  as the default authored format now.
- It **does** mean Graphshell should avoid boxing itself into a future where
  Markdown is the only serious authored/publication lane.
- If Graphshell ever promotes HTML into a first-class authored surface, that
  should be a conscious architecture change with explicit rationale, not an
  accidental side effect of accessibility work or protocol enrichment.

The healthiest direction is:

- Markdown as the default authored floor,
- HTML as a first-class richer target when the product needs it,
- faithful source rendering for browsed protocols,
- shared internal adaptation where useful, but without flattening protocol
  identity.

---

## 5. Faithful Render Plus Optional Assistive Enrichment

This is the key policy recommendation from this note.

Simple protocols are not inherently less accessible than HTML, but they do put
more responsibility onto the client. The Gemini Application Developer Guide
updated on 2026-04-08 explicitly frames Gemini apps as needing to work across
graphical, text-based, and even audio-only clients. That is a very
Graphshell-compatible stance and points toward **better client adaptation**,
not silent protocol replacement.

Graphshell should preserve the content grammar of the source protocol:

- Gemini should render as Gemini.
- Gopher should render as Gopher, with faithful-source options where needed.
- RSS and Atom should remain feed-shaped content.
- Static HTML should remain HTML-shaped content unless explicitly routed through
  reader-mode or another declared transform.

However, Graphshell should also offer optional assistive enrichment layers.

Useful assistive enrichments include:

- document outline and heading summary,
- section and action inventory,
- thread summary for forum-like or feed-like content,
- link-role labeling and clearer action language,
- alt-text and preformatted-block summaries where available,
- text-to-speech-oriented views,
- ASCII/icon fallback modes where decorative Unicode or emoji would reduce
  clarity,
- graph-aware "why is this here?" provenance summaries,
- readability transforms that are explicit modes rather than silent rewrites.

This aligns with the current accessibility subsystem and with the Gemini app
guide's repeated warning that Gemini applications must remain usable across
graphical, text-based, and non-visual/audio-only clients.

In short:

- source-faithful by default,
- assistive when requested or beneficial,
- never silently protocol-erasing.

---

## 6. Concrete Graphshell Opportunities

### 6.1 Discovery Packs

Add optional small-web starter packs instead of fixed defaults:

- Bubble instances,
- Cosmos,
- Wander consoles,
- curated RSS/Atom sources,
- trusted Gemini app examples.

The user should be able to opt into one or more packs, inspect provenance, and
remove them cleanly.

### 6.2 Constellation Projection

Add a Navigator projection type inspired by Cosmos:

- anchor post or anchor document,
- reply/reference edges,
- frontier ranking,
- thread cluster layout,
- recent-activity and unread-like cues.

This projection should work for more than Cosmos:

- Bubble,
- gemlogs,
- RSS/Atom citation chains,
- future Nostr/Matrix bridges.

### 6.3 Small-Protocol Social Threading

Treat Bubble and related Gemini social surfaces as graph inputs:

- posts become nodes,
- replies/references become typed edges,
- subspaces become bounded topical regions or graphlets,
- issue trackers become thread + state projections.

### 6.4 Owner-Scoped Temporal Branching

Use `history-tree` as inspiration for a history follow-on:

- explicit branching when opening into a new pane/frame/context,
- owner-local forward preference,
- durable shared ancestry,
- replayable temporal relationships without collapsing to a single flat stack.

### 6.5 Archive and Dataset Lane

Treat irchiver-like archive behavior as a future optional lane:

- local-first,
- explicit consent,
- what-was-seen artifacts distinct from fetched-source artifacts,
- search and dataset export as declared outcomes.

### 6.6 Accessibility Lenses for Simple Protocols

Build protocol-specific assistive lenses without mutating source truth:

- Gemini app outline lens,
- feed summary lens,
- thread-summary lens,
- low-distraction reading lens,
- screen-reader-friendly navigation aids,
- explicit transport/trust labeling that stays neutral for plaintext protocols.

---

## 7. Executable Slices

This note should now be read as a set of concrete slices rather than as a loose
backlog of adjacent ideas.

### 7.1 Slice A: discovery and aggregation signal model

Goal:

- define separate ingest/provenance lanes for discovery, recency, clustering,
  and search rather than flattening them into one ranking model.

Deliverables:

- a follow-on note covering CAPCOM, Antenna, Cosmos, Spacewalk, and GUS as
  distinct signals,
- graph object/provenance rules for imported feed items and discovery sources,
- hooks for Navigator and ranking without committing to one global scorer.

### 7.2 Slice B: constellation projection prototype

Goal:

- turn thread-like structures into a bounded local-world projection for
  Navigator and graphlets.

Deliverables:

- typed reply/reference edge rules,
- an anchor/frontier model,
- one projection path that works first for Bubble/Cosmos-like imports and later
  for gemlogs, feeds, and social bridges.

### 7.3 Slice C: discovery packs

Goal:

- give users opt-in seed content for smolweb exploration without hardcoding a
  canonical default universe.

Deliverables:

- pack manifest format,
- provenance display for why a source is present,
- install/remove flow for Bubble, Cosmos, Wander, and curated feed examples.

### 7.4 Slice D: assistive lenses for simple protocols

Goal:

- add accessibility and orientation benefits without erasing protocol truth.

Deliverables:

- outline/heading lens,
- feed-summary and thread-summary lenses,
- speech-friendly and low-distraction modes,
- explicit trust/transport labeling rules that remain neutral for plaintext
  protocols.

### 7.5 Slice E: IRC and archive lane positioning

Goal:

- model IRC as a communication/archive-adjacent lane rather than forcing it
  into the document renderer.

Deliverables:

- communication truth vs retained excerpt/archive artifact split,
- consent rules for logging and dataset export,
- integration boundaries with discovery and workbench research flows.

### 7.6 Slice F: owner-scoped temporal branching

Goal:

- borrow the strongest `history-tree` ideas without discarding Graphshell's
  existing temporal-history authority.

Deliverables:

- explicit branch events for opening into new panes/frames/contexts,
- owner-local forward preference semantics,
- durable shared ancestry rules.

### 7.7 Slice G: archive and dataset lane

Goal:

- add a local-first, consent-bound archive capability for "what was seen"
  rather than only fetched source artifacts.

Deliverables:

- artifact distinction between fetched source and observed/retained capture,
- search/export goals,
- privacy and consent guardrails.

## 8. Program Tracks

The next planning pass should run on two explicit tracks.

### 8.1 Track A: user-visible Middlenet growth

Recommended order:

1. Slice A: discovery and aggregation signal model
2. Slice C: discovery packs
3. Slice B: constellation projection prototype
4. Slice D: assistive lenses for simple protocols

Why this order:

- it produces visible new Middlenet affordances quickly,
- it separates discovery/recency/clustering/search before UI solidifies,
- it lets Graphshell ship breadth without giving up protocol faithfulness.

### 8.2 Track B: architectural closure before wider expansion

Recommended order:

1. identity convergence note and URI scheme note,
2. browser-envelope co-op and degradation policy,
3. graph-object classification model,
4. history branching follow-on,
5. IRC/archive positioning.

Why this order:

- it closes the architectural seams most likely to drift while the product is
  still mostly native desktop,
- it keeps future discovery and cross-protocol work anchored to coherent
  address, identity, and host-policy rules,
- it prevents smolweb feature growth from outrunning the system model.

---

## 9. Final Recommendation

Graphshell should lean harder into the smolweb, but in a way that strengthens
its architecture instead of bloating it.

The right move is not:

- "support every niche protocol feature directly in the core UI", or
- "upgrade simple protocols by silently turning them into richer web pages".

The right move is:

- ingest their structure into the graph,
- give Navigator strong projections for discovery and constellation-like local
  worlds,
- keep history temporal and branching,
- preserve protocol faithfulness,
- add optional assistive enrichment where it increases legibility,
  accessibility, or orientation.

This lets Graphshell "use every part of the buffalo" without disrespecting the
design intent of the protocols it wants to champion.
