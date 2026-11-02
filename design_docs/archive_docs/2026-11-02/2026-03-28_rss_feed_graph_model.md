> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-18 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/verso_docs/research/2026-03-28_rss_feed_graph_model.md`
> - Source commit: `401e2fcc`
>
> None of it was ever built as written. It is filed here as research, not as a
> plan and not as a statement of where Turnstone is going. It speaks the donor
> vocabulary (Graphshell as the browser, Verso as the peer runtime, Verse for
> the network layer) and its graphlet, lens and workbench links point at donor
> docs that no longer exist locally. Read the text below for the ideas, not the
> direction.
>
> Sibling: `2026-03-28_smolnet_follow_on_audit.md`, recovered into this folder
> in the same pass, is the admission bar that section 1 below measures RSS
> against before setting it aside as a content format rather than a transport.
>
> Turnstone did land graph-native feed subscription, so the consumption half of
> this note has a real successor in the shipping tree rather than in a doc: the
> `feed` module and the `nematic.feed` engine in genet cover fetching and
> parsing, and feeds enter the graph as nodes. The chain topology, capacity
> eviction, ghost nodes and Atom publication described below were not built.

---

<!-- This Source Code Form is subject to the terms of the Mozilla Public
     License, v. 2.0. If a copy of the MPL was not distributed with this
     file, You can obtain one at https://mozilla.org/MPL/2.0/. -->

# RSS/Atom Feed Graph Model

**Date**: 2026-03-28
**Status**: Research / Design Exploration
**Purpose**: Analyze RSS/Atom as a Graphshell capability — both consumption (feed subscription with a live graph structure) and publication (serve graph content as Atom feeds). Define the feed graphlet interaction model, including the chain topology, capacity eviction, post harvesting, ghost nodes, and workbench opening semantics.

**Related**:

- [`../../graphshell_docs/technical_architecture/graphlet_model.md`](../../graphshell_docs/technical_architecture/graphlet_model.md) — Graphlet kinds, ownership model, boundary rules
- [`../../graphshell_docs/implementation_strategy/workbench/graphlet_projection_binding_spec.md`](../../graphshell_docs/implementation_strategy/workbench/graphlet_projection_binding_spec.md) — Graphlet ↔ workbench binding, reconciliation
- [`../../graphshell_docs/implementation_strategy/graph/2026-03-14_graph_relation_families.md`](../../graphshell_docs/implementation_strategy/graph/2026-03-14_graph_relation_families.md) — Edge families; `rss-membership` as Imported sub-kind
- [`../../graphshell_docs/implementation_strategy/graph/2026-03-21_edge_family_and_provenance_expansion_plan.md`](../../graphshell_docs/implementation_strategy/graph/2026-03-21_edge_family_and_provenance_expansion_plan.md) — `rss-membership` provenance marker
- [`../../graphshell_docs/implementation_strategy/graph/2026-03-27_lens_decomposition_and_view_policy_plan.md`](../../graphshell_docs/implementation_strategy/graph/2026-03-27_lens_decomposition_and_view_policy_plan.md) — Lens policy surfaces and per-view overrides
- [`../technical_architecture/VERSO_AS_PEER.md`](../technical_architecture/VERSO_AS_PEER.md) — Capsule server pattern (Gemini/Gopher/Finger); structural analog for Atom feed server
- [`2026-03-28_smolnet_follow_on_audit.md`](2026-03-28_smolnet_follow_on_audit.md) — Smolnet admission bar (RSS is not a transport protocol and does not apply here)
- [`../../verse_docs/research/2026-02-22_aspirational_protocols_and_tools.md`](../../verse_docs/research/2026-02-22_aspirational_protocols_and_tools.md) — §7.1 "Feed Nodes" concept, `feed-rs` crate

---

## 1. Why RSS/Atom Is Different from the Smolnet Protocols

RSS/Atom is not a transport protocol. It does not define a TCP-level connection, handshake, or session model. It is an **XML content format served over HTTP/HTTPS** — a transport Servo already handles.

The smolnet follow-on audit's admission bar (user-felt benefit over Servo fallback, stable capability-family fit, clear trust model, small maintenance surface) does not apply in the same way. RSS is not competing with Servo for rendering — it is a structured content format that Servo renders as raw XML, losing all feed semantics in the process.

The question is not "should Verso add another protocol handler" but "should Graphshell treat feed content as a first-class graph structure with native subscription, projection, and publication semantics."

The answer is yes, for two reasons:

1. **Consumption**: RSS/Atom feeds are the largest corpus of structured, update-bearing content on the web. Every blog, podcast, newspaper, GitHub repo, subreddit, and YouTube channel publishes one. Native feed subscription gives Graphshell a live content pipeline from the existing web — far more content than Gemini and Gopher combined.

2. **Publication**: An Atom feed is universally consumable. Any feed reader, any platform, any aggregator can subscribe. Serving graph content as Atom makes Graphshell a publisher to the entire existing RSS ecosystem — billions of clients, not dozens.

---

## 2. Two Capabilities

### 2.1 Feed Consumption (Subscription + Graph Projection)

A feed URL becomes a **feed node** — a graphlet anchor that owns a live chain of **post nodes** representing feed entries.

This is not a viewer. It is a graph-level subscription that creates and manages nodes over time. The viewer renders the individual post nodes (via `SimpleDocument`, Servo webview for the linked article, or a native feed-entry viewer).

### 2.2 Feed Publication (Atom Capsule Server)

Serve a graph collection, tag, or workspace as an Atom feed over HTTP — the same capsule server pattern as Gemini/Gopher/Finger, but over HTTP (RFC 4287 Atom Syndication Format).

Each served node becomes an `<entry>` with title, updated timestamp, content summary, and link back to the node's canonical address (Gemini capsule URL, HTTP URL, or `graphshell://node/{uuid}`).

---

## 3. The Feed Graphlet

### 3.1 Structure

A subscribed feed is a **feed graphlet**: a bounded, ordered subgraph anchored by the feed node.

```
[Feed Node] ──rss-membership──▶ [Post 1 (newest)]
                                      │
                               rss-chain-order
                                      │
                                      ▼
                                [Post 2]
                                      │
                               rss-chain-order
                                      │
                                      ▼
                                [Post 3]
                                      │
                               rss-chain-order
                                      │
                                      ▼
                                 ... up to N
```

- **Feed node**: the anchor. Represents the feed URL itself. Visually distinct from post nodes (different color, heavier border, feed icon badge). Carries feed-level metadata: title, description, site URL, last-fetch timestamp, poll interval, capacity N.
- **Post nodes**: members. Each represents one `<entry>` / `<item>`. Connected to the feed node by `rss-membership` edges (Imported family). Connected to each other by `rss-chain-order` edges that encode publication order (newest → oldest).
- **Capacity N**: the maximum number of post nodes the feed graphlet holds. User-configurable per feed (default: 20–50).

### 3.2 Edge Taxonomy

Two new edge sub-kinds, both in the **Imported** family:

| Sub-kind | Semantics | Persistence | Layout influence |
|----------|-----------|-------------|-----------------|
| `rss-membership` | Post belongs to this feed | Derived-readonly (recomputed from feed state) | Radial from feed node (when active) |
| `rss-chain-order` | Temporal ordering between posts | Derived-readonly | Sequential chain force (when active) |

`rss-membership` already exists in the edge family taxonomy. `rss-chain-order` is new — it encodes the publication sequence so the chain shape is a graph-backed property, not a renderer heuristic.

### 3.3 Visual Treatment

The feed graphlet should be visually legible as a distinct structure on the canvas:

- **Feed node**: larger, distinct color (e.g. warm amber), heavier stroke, feed icon badge. The anchor is unmistakable.
- **Post nodes**: standard size, tinted subtly (e.g. lighter amber) to signal membership. The tint fades with age down the chain.
- **Chain edges** (`rss-chain-order`): lighter weight than semantic edges, possibly dashed or dotted. Distinct enough to read as "sequence" rather than "user-authored relationship."
- **Membership edges** (`rss-membership`): hidden by default (like other Imported edges). Shown on hover/selection of the feed node or via lens activation.

The chain reads as a visual appendage of the feed node — a tail of posts trailing behind it in publication order.

---

## 4. Lifecycle: Emission, Eviction, Harvest

### 4.1 Emission

The feed node **slowly emits** post nodes. On subscription or after each poll, new entries don't all appear at once — they materialize one at a time at a configurable rate ("time to N").

```
poll returns 5 new entries
  → post 1 appears immediately (newest)
  → post 2 appears after delay
  → post 3 appears after delay
  → ...up to N total
```

The emission rate is user-configurable per feed:

```rust
pub struct FeedConfig {
    /// Maximum post nodes in the chain at any time.
    capacity: u32,                  // default: 30
    /// How often to poll the feed URL.
    poll_interval: Duration,        // default: 30 minutes
    /// Delay between emitting successive post nodes after a poll.
    emission_interval: Duration,    // default: 5 seconds
}
```

This gives the feed a living, breathing quality — posts emerge from the feed node over time rather than appearing as a static dump. The user sees the feed "working."

### 4.2 Eviction ("Eating")

When a new post would push the chain over capacity N, the feed node **eats the oldest post node** to make room. The oldest node at the tail of the chain is archived: its `rss-membership` and `rss-chain-order` edges are removed, the node itself is either deleted or demoted to a dormant state (retaining its metadata in the graph store but invisible on the canvas).

```
capacity = 5, chain has [1, 2, 3, 4, 5]
new post arrives → feed eats post 5
chain becomes [new, 1, 2, 3, 4]
```

The eviction is visual: the oldest post node shrinks/fades and is absorbed back into the feed node. The feed node is the lifecycle authority — it creates and reclaims its children.

### 4.3 Harvest (User Rescue)

A user can **harvest** a post node to save it permanently in their graph, removing it from the feed's lifecycle control.

**Mechanism**: drag the post node away from the chain. When the drag distance exceeds a threshold (or drag force exceeds a spring constant), the post node **pops off** the chain:

1. The `rss-chain-order` edges on either side of the harvested node are **rewired**: the predecessor connects directly to the successor, closing the gap in the chain.
2. The `rss-membership` edge is **toggled off** (not deleted — the provenance record is preserved, but the edge no longer participates in the graphlet's active membership set).
3. The post node becomes a **free graph node** — fully user-owned, no longer subject to the feed's eviction lifecycle. It persists indefinitely like any other node.
4. A **ghost node** appears in the chain at the position the harvested node occupied (see §5).

The harvested node retains its content, metadata, and the dormant `rss-membership` provenance edge. It is now a first-class citizen in the user's semantic graph. The user can tag it, link it to other nodes, annotate it — it's theirs.

**Threshold tuning**: The pop-off threshold should feel like plucking a berry from a vine — a deliberate tug, not an accidental drag. A spring constant on the chain edges provides the resistance. The threshold could be:
- Distance-based: pop off when dragged > X pixels from the chain axis
- Force-based: pop off when the drag force exceeds the `rss-chain-order` edge's spring constant
- Combined: distance for the trigger, spring constant for the visual stretch animation

---

## 5. Ghost Nodes

When a post node is harvested from the chain, a **ghost node** takes its place to maintain the chain's topology and provide a shortcut back to the harvested content.

### 5.1 What a Ghost Node Is

A ghost node is a **lightweight proxy** in the chain — visually smaller, translucent, displaying a thumbnail or abbreviated title of the harvested post. It occupies the topological position the harvested node held, keeping the chain unbroken.

```
Before harvest:  [Feed] → [A] → [B] → [C] → [D]
Harvest C:       [Feed] → [A] → [B] → [Ghost:C] → [D]
                                            ↓ (shortcut)
                                           [C] (free, user-owned)
```

### 5.2 Ghost Node Properties

```rust
pub struct GhostNode {
    /// The chain position this ghost occupies.
    chain_position: u32,
    /// The harvested node this ghost is a proxy for.
    target_node_id: NodeId,
    /// Display: thumbnail/title from the target node, rendered translucent.
    display_hint: GhostDisplayHint,
}

pub enum GhostDisplayHint {
    /// Show a miniature version of the target node's content.
    Thumbnail,
    /// Show the target node's title only.
    TitleOnly,
}
```

### 5.3 Ghost Node Behavior

- **Click/activate**: navigates to the actual harvested node (focuses it on the canvas or opens it in the workbench). The ghost is a shortcut, not content.
- **Chain participation**: the ghost participates in `rss-chain-order` edges like a normal chain member. The chain shape is preserved.
- **Eviction**: when the ghost reaches the tail of the chain and would be evicted, it simply disappears. The harvested node it pointed to is unaffected (it's already user-owned).
- **Visual**: translucent, reduced size, possibly with a small link icon indicating it's a proxy. Clearly distinct from live post nodes.

### 5.4 Ghost Nodes as a General Primitive

Ghost nodes are introduced here for the feed chain, but the concept is general: a lightweight proxy that maintains topological position while the real node lives elsewhere in the graph. Future uses could include:

- A node that has been moved to a different frame but still "belongs" in a graphlet's topology
- A node that represents a collapsed cluster (expand to see the full subgraph)
- A pinned reference to a node in another user's graph (co-op context)

This research doc does not propose ghost nodes as a general graph primitive — that decision belongs in the graphlet model or graph architecture docs. But the feed chain demonstrates the pattern clearly, and a general primitive would be the natural follow-on.

---

## 6. Workbench Opening Semantics

The feed graphlet is technically a graphlet, but the default interaction is to open **one post at a time** — not the entire feed.

### 6.1 Default: Single Post

Clicking a post node in the chain opens that single post in the workbench (or focuses the existing tile if already open). This is the common case — you're browsing a feed, you see an interesting title, you open it.

The post node opens using the appropriate viewer:
- If the post has a `<link>` to a full article: open in Servo webview
- If the post has inline `<content>`: render as `SimpleDocument` or in a native feed-entry viewer
- If the post is a podcast enclosure: open in AudioViewer

### 6.2 Graphlet Open: Entire Feed

The user can open the entire feed graphlet in the workbench as a tile group. This creates a linked graphlet binding (per `graphlet_projection_binding_spec.md`) — the workbench tile group tracks the feed's membership and updates as new posts are emitted and old ones evicted.

This is useful for "feed reading mode" — scanning through all current posts in a structured layout.

### 6.3 Multi-Selection: Arbitrary Posts

The user can multi-select arbitrary post nodes from the chain (Ctrl+click, lasso, etc.) and open that selection as a workbench group. This creates an `UnlinkedSessionGroup` — not tied to the feed's lifecycle, just a snapshot of the selected posts.

### 6.4 Summary

| Action | Result | Binding |
|--------|--------|---------|
| Click single post | Open that post in workbench | None (single node) |
| Open feed graphlet | Open all current posts as tile group | `Linked` to feed graphlet |
| Multi-select posts | Open selected posts as tile group | `UnlinkedSessionGroup` |
| Click ghost node | Navigate to harvested post | None (shortcut) |

---

## 7. Feed Publication (Atom Server)

### 7.1 Capsule Server Pattern

Serve a graph selection as an Atom feed over HTTP, following the same pattern as the Gemini/Gopher/Finger capsule servers:

| Intent | Effect |
|--------|--------|
| `StartAtomFeedServer { port }` | Start the HTTP server (default: 8080 or configurable) |
| `StopAtomFeedServer` | Stop the server |
| `ServeCollectionAsAtomFeed { collection_id, title, feed_path }` | Register a node collection/tag/workspace as a feed |
| `UnserveAtomFeed { feed_path }` | Remove a feed from the server |

### 7.2 Feed Construction

A served Atom feed maps graph nodes to Atom entries:

```xml
<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>My Research Graph</title>
  <link href="gemini://localhost:1965/" rel="alternate"/>
  <updated>2026-03-28T12:00:00Z</updated>
  <id>urn:uuid:{collection_id}</id>

  <entry>
    <title>{node.title}</title>
    <link href="{node.url}" rel="alternate"/>
    <id>urn:uuid:{node.id}</id>
    <updated>{node.last_modified}</updated>
    <summary>{node.content_preview}</summary>
    <author><name>{user.display_name}</name></author>
  </entry>
  <!-- ...more entries, ordered by last_modified desc -->
</feed>
```

### 7.3 What Gets Published

Only nodes in the registered collection/tag/workspace with `ArchivePrivacyClass::PublicPortable` are included. The same access control model as the Gemini capsule server applies.

### 7.4 Cross-Protocol Synergy

The Atom feed server and the Gemini capsule server complement each other:

- **Atom feed**: discovery channel. Feed readers poll it, see new entries.
- **Gemini links**: each Atom `<entry>` can link to `gemini://host:1965/node/{uuid}` for the full lightweight document.
- **HTTP link**: or to the node's original HTTP URL if applicable.

A user publishes their graph to both Gemini (full content) and Atom (update notifications). Subscribers discover via Atom, read via Gemini. The entire small-web publishing stack from a single Graphshell instance.

---

## 8. Implementation Notes

### 8.1 Feed Parsing

`feed-rs` (crates.io) handles RSS 0.9/1.0/2.0, Atom 1.0, and JSON Feed. Well-maintained, pure Rust, no C dependencies. This is the right crate.

### 8.2 Feed Polling Worker

A `FeedPollWorker` — a tokio task supervised by ControlPanel, same pattern as `SyncWorker` and the capsule server workers:

```rust
pub struct FeedPollWorker {
    subscriptions: Vec<FeedSubscription>,
    http_client: reqwest::Client,
    command_rx: mpsc::Receiver<FeedCommand>,
    output_tx: mpsc::Sender<FeedOutput>,
    diagnostics: DiagnosticsWriteHandle,
}

pub struct FeedSubscription {
    feed_url: Url,
    feed_node_id: NodeId,
    config: FeedConfig,
    last_poll: Option<SystemTime>,
    known_entry_ids: HashSet<String>,  // deduplication via <id>/<guid>
}

pub enum FeedCommand {
    Subscribe { feed_url: Url, config: FeedConfig },
    Unsubscribe { feed_node_id: NodeId },
    UpdateConfig { feed_node_id: NodeId, config: FeedConfig },
    PollNow { feed_node_id: NodeId },
}

pub enum FeedOutput {
    NewEntries { feed_node_id: NodeId, entries: Vec<FeedEntry> },
    PollFailed { feed_node_id: NodeId, error: String },
    FeedGone { feed_node_id: NodeId },  // HTTP 410 or repeated failures
}
```

`FeedOutput::NewEntries` is received by the reducer, which emits `GraphIntent`s to create post nodes, `rss-membership` edges, and `rss-chain-order` edges, and to evict the oldest node if over capacity.

### 8.3 Atom Feed Server

`axum` or `warp` for the HTTP server. Atom XML construction is simple enough to template directly (no heavy XML crate needed), or use `atom_syndication` (crates.io) for correct RFC 4287 serialization.

### 8.4 GraphIntent Surface

```rust
// Feed subscription lifecycle
SubscribeToFeed { feed_url: Url, config: FeedConfig }
UnsubscribeFromFeed { feed_node_id: NodeId }
UpdateFeedConfig { feed_node_id: NodeId, config: FeedConfig }
PollFeedNow { feed_node_id: NodeId }

// Post lifecycle (emitted by reducer in response to FeedOutput)
EmitFeedPost { feed_node_id: NodeId, entry: FeedEntry }
EvictOldestFeedPost { feed_node_id: NodeId }
HarvestFeedPost { post_node_id: NodeId }  // user-initiated

// Atom publication
StartAtomFeedServer { port: Option<u16> }
StopAtomFeedServer
ServeCollectionAsAtomFeed { collection_id: CollectionId, title: String, feed_path: String }
UnserveAtomFeed { feed_path: String }
```

### 8.5 Diagnostics

```
feed:poll:success        — Info  — feed polled, N new entries
feed:poll:failed         — Warn  — poll failed (HTTP error, parse error, timeout)
feed:poll:gone           — Warn  — feed appears permanently gone (410 or repeated 404)
feed:post:emitted        — Info  — post node created
feed:post:evicted        — Info  — oldest post evicted from chain
feed:post:harvested      — Info  — post harvested by user
feed:atom:started        — Info  — Atom feed server listening
feed:atom:stopped        — Info  — Atom feed server stopped
```

---

## 9. Ownership Boundary

| Concern | Owner | Notes |
|---------|-------|-------|
| Feed parsing | Verso (or core, if feeds are useful without web) | `feed-rs` crate, HTTP fetch |
| Feed polling | ControlPanel-supervised worker | Same pattern as SyncWorker |
| Graph node/edge creation | Reducer (via `GraphIntent`) | Feed worker emits intents, never mutates directly |
| Chain topology | Graph domain | `rss-chain-order` edges are graph truth |
| Ghost nodes | Graph domain | Ghost is a node variant, not a renderer trick |
| Visual treatment | Canvas style policy | Feed-specific `CanvasStylePolicy` entries |
| Emission timing | Feed worker | Configurable delay per subscription |
| Eviction | Reducer | Intent-driven; feed worker signals, reducer acts |
| Harvest | User action → reducer | Drag threshold handled by canvas input → intent |
| Atom publication | Verso capsule server pattern | HTTP server, same lifecycle as Gemini |

---

## 10. Open Questions

1. **Feed node without Verso**: Should feed subscription require Verso (network access), or should it work with any HTTP-capable path? If a future lean mode has no Servo but has `reqwest`, feed polling could still work. The graph structure and ghost node model are entirely shell-side — only the HTTP fetch needs networking.

2. **Ghost node as general primitive**: This doc introduces ghost nodes for the feed chain. Should the graphlet model adopt ghost nodes as a general concept (proxy for a node that lives elsewhere in the graph)? The feed chain is a strong motivating case, but the general primitive belongs in `graphlet_model.md` if adopted.

3. **Emission animation**: The "slow emission" effect (posts materializing one at a time) is a canvas animation concern. Should the emission interval be a graph-level config (stored on the feed node) or a presentation-level config (lens/style policy)? Recommendation: graph-level, since it affects when nodes actually exist in the graph, not just when they're rendered.

4. **Feed discovery**: Should Graphshell auto-detect RSS/Atom feeds when browsing (via `<link rel="alternate" type="application/atom+xml">` in HTML)? This is a natural Servo integration point — detect the feed link, show a "Subscribe" action in the chrome.

5. **Atom feed server port sharing**: The Atom feed server serves over HTTP. The embedded Nostr relay also uses HTTP (for NIP-11). Should they share a port with path-based routing, or use separate ports? Path-based (`/atom/...` for feeds, `/` for NIP-11) is cleaner but requires a shared HTTP router.

6. **Chain direction on canvas**: Should the chain grow left-to-right (newest on left, reading order) or radially outward from the feed node? This is a layout/physics concern — the `rss-chain-order` edge family could have a directional layout hint.
