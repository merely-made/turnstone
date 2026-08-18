> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-16 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/graphshell_docs/research/2026-04-09_smolweb_browser_capability_gaps.md`
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

# Smolweb Browser Capability Gaps

**Date**: 2026-04-09  
**Status**: Research note / backlog-shaping input  
**Purpose**: Record the browser-capability gaps that still separate
Graphshell's promising smolweb/middlenet foundation from a genuinely mature
day-to-day smolweb browser.

**Related docs**:

- [`2026-04-09_smolweb_graph_enrichment_and_accessibility_note.md`](2026-04-09_smolweb_graph_enrichment_and_accessibility_note.md)
- [`../technical_architecture/2026-03-29_middlenet_engine_spec.md`](../technical_architecture/2026-03-29_middlenet_engine_spec.md)
- [`../technical_architecture/2026-03-30_protocol_modularity_and_host_capability_model.md`](../technical_architecture/2026-03-30_protocol_modularity_and_host_capability_model.md)
- [`../implementation_strategy/subsystem_accessibility/SUBSYSTEM_ACCESSIBILITY.md`](../implementation_strategy/subsystem_accessibility/SUBSYSTEM_ACCESSIBILITY.md)
- [`../implementation_strategy/subsystem_history/SUBSYSTEM_HISTORY.md`](../implementation_strategy/subsystem_history/SUBSYSTEM_HISTORY.md)
- [`../implementation_strategy/social/comms/COMMS_AS_APPLETS.md`](../implementation_strategy/social/comms/COMMS_AS_APPLETS.md)
- [`../implementation_strategy/social/comms/2026-04-09_irc_public_comms_lane_positioning.md`](../implementation_strategy/social/comms/2026-04-09_irc_public_comms_lane_positioning.md)
- [`../../verso_docs/research/2026-03-28_smolnet_follow_on_audit.md`](../../verso_docs/research/2026-03-28_smolnet_follow_on_audit.md)

**External references**:

- [Lagrange main page](https://gmi.skyjake.fi/lagrange/)
- [Lagrange v1.19: Site Structure, Vertical Tabs, and Subscription Management](https://gmi.skyjake.fi/gemlog/2025-09_lagrange-1.19.gmi)
- [Lagrange v1.13: Curses TUI, Spartan, Emoji](https://gmi.skyjake.fi/gemlog/2022-05_lagrange-1.13.gmi)
- [Amfora README](https://github.com/makew0rld/amfora)

---

## 1. Executive Summary

Graphshell is no longer missing only protocol ideas. It is increasingly missing
the browser-capability layer that makes a smolweb browser feel complete,
trustworthy, and pleasant to live in every day.

The key conclusion from looking at current Graphshell research alongside mature
smolweb clients is:

1. The next bottleneck is browser maturity more than protocol breadth.
2. Protocol additions should be scoped against capability gaps, not collected
   for their own sake.
3. The strongest next work is trust UX, subscription/source health, source and
   page tools, discovery/wayfinding structure, retention boundaries, and
   carefully bounded communication/mutation lanes.

In other words: Graphshell already has a believable smolweb engine direction,
but it still needs the layer of operational, navigational, and accessibility
affordances that make that direction cohere as a browser.

---

## 2. What Current Clients Suggest

The most useful lesson from existing smolweb browsers is not "support exactly
the same protocols they do." It is that they spend real design energy on the
boring capabilities that keep simple protocols usable.

The current public Lagrange materials are especially instructive:

- v1.19 adds a structure sidebar, vertical tabs, and a dedicated subscription
  manager with visibility into recent updates, feed entry counts, latest post
  dates, and empty or broken subscriptions.
- v1.13 added an explicit source-vs-styled toggle for Gemtext pages, which is
  a strong precedent for preserving faithful source while still allowing a more
  processed reading mode.

Amfora's README points in a similar direction. Its feature set emphasizes:

- TOFU and error handling,
- bookmarks and subscriptions,
- page download,
- client certificate support,
- search in page,
- opening non-text content in another application,
- and command-level actions on the current URL.

These are not peripheral luxuries. They are the browser-operations layer that
stops smolweb browsing from feeling brittle.

---

## 3. Current Graphshell Baseline

Graphshell already has several strong ingredients:

- a graph-first model,
- Navigator as a bounded local-world projection surface,
- protocol-faithful middlenet adapters for Gemini/gemtext, Gopher, Finger,
  RSS, Atom, JSON Feed, Markdown, and plain text,
- identity convergence work around WebFinger, NIP-05, Matrix, and
  ActivityPub,
- explicit accessibility posture,
- explicit social/comms host-surface positioning.

That means this note is not arguing that Graphshell lacks vision. It is
arguing that the following browser capabilities still need to be made explicit
enough to guide implementation.

---

## 4. Capability Gaps

### 4.1 Trust, Identity, and Certificate UX

This is one of the biggest gaps.

Graphshell already has identity convergence research, but a mature smolweb
browser also needs routine trust ergonomics:

- TOFU state visibility,
- certificate failure and rotation handling,
- client certificate management,
- clear distinction between plaintext protocol, secure protocol, and broken
  trust state,
- good answers to "why did this stop working?"

Graphshell implication:

- trust state must become a legible browser surface, not just an internal
  protocol detail.

### 4.2 Subscription and Source-Health Operations

Supporting feeds is not the same thing as operating a feed browser well.

What still needs clearer definition:

- active subscription inventory,
- stale/empty/broken/redirected feed visibility,
- recent-update highlighting,
- source health and refresh behavior,
- explicit distinction between discovery candidate and actual subscription,
- better recency handling than naive polling alone

Graphshell implication:

- subscriptions should become a first-class operational surface with health,
  provenance, and actionability.

### 4.3 Page, Source, and Local Page Tools

Smolweb browsing depends heavily on small tools that help a user orient
themselves inside a page or capsule.

Important missing capabilities:

- source view vs styled view,
- page outline and heading inventory,
- site or capsule structure view,
- search in page,
- save/download/open externally,
- alternate-open routing for the current resource

Graphshell implication:

- the shell needs more page-local tools, not just better rendering.

### 4.4 Discovery Signal Separation

Current research already identified useful external discovery sources:

- CAPCOM, Antenna, and Spacewalk for discovery/update aggregation,
- Cosmos for clustering,
- GUS for search,
- Wander for neighborhood traversal.

The gap is not awareness of these services. The gap is product separation.

Graphshell still needs to keep distinct:

- discovery,
- freshness,
- clustering,
- search,
- random-walk or neighborhood traversal

Graphshell implication:

- do not flatten every discovery input into one opaque ranking model.

### 4.5 Public Wayfinding Surfaces

Smolweb browsing is not just about documents. It is also about orientation
infrastructure:

- support spaces,
- bulletin boards,
- aggregators,
- search engines,
- issue spaces,
- channel directories,
- community hubs such as tildeverse infrastructure

Graphshell implication:

- these should be modeled as wayfinding surfaces and graph inputs, not only as
  pages someone happens to open manually.

### 4.6 Retention, Offline, and Consent-Bound Archive Model

Graphshell's archive instincts are strong, but the artifact model is still not
fully explicit.

Important distinctions still need to harden:

- transient cache vs saved item,
- saved item vs explicit offline-reading surface,
- fetched source artifact vs "what I actually saw" capture,
- graph citation/clipping vs transcript/log retention,
- private local retention vs exportable dataset artifact,
- retention by user action vs silent background capture

Graphshell implication:

- the browser needs a clear retention and saved-reading model before broader
  archive features or dataset workflows expand.

### 4.7 Hosted Communication Lanes

IRC was the right expansion because it reflects real smolweb social practice.
Misfin remains relevant as a lightweight messaging/contact lane. Bubble and
similar Gemini spaces blur the line between document browsing and community
participation.

The gap is not merely protocol support. It is hosted communication ergonomics:

- channel-as-surface behavior,
- public vs bilateral vs room-like communication boundaries,
- link/excerpt capture into the graph,
- explicit transcript controls,
- communication discovery without letting Comms become a second giant product

Graphshell implication:

- keep communication lanes hosted, scoped, and graph-aware.

### 4.8 Mutation and Publication Loops

Graphshell's current smolweb direction is still more mature on reading than on
writing.

Potential next needs:

- Titan upload and submission flows,
- Bubble posting or issue participation,
- cleaner publication/export bridges for gemtext, Markdown, HTML, and feeds,
- explicit "publish to this lane" actions instead of hidden protocol sidecars

Graphshell implication:

- if Graphshell wants to become a serious smolweb tool, mutation and
  publication need to be treated as first-class workflows, not leftovers.

### 4.9 Surface Routing, Provenance, and Explainability

Graphshell's architecture already supports multiple surfaces, but the user
experience around routing still needs more explicit product shape.

Users should be able to understand:

- why a resource opened in this viewer,
- what other surface modes are available,
- whether a view is faithful, assistive, summarized, or transformed,
- why a given item appeared in a feed, graphlet, or discovery list

And users should be able to switch those surfaces easily when it matters:

- faithful source,
- reader-oriented or assistive view,
- feed-oriented view,
- side-by-side or split-surface inspection,
- open externally,
- graph capture,
- transcript on/off for communication surfaces.

Graphshell implication:

- routing should become explainable, inspectable, and easy to override, not
  merely correct.

### 4.10 Host-Degradation and Capability Signaling

The protocol modularity docs already define host-aware degradation as an
architectural rule. The remaining gap is user-facing and planning-facing
clarity.

For each host envelope, Graphshell still needs to make more obvious:

- which smolweb capabilities are portable,
- which require native power,
- which degrade to read-only or proxied forms,
- which should simply not appear on that host

Graphshell implication:

- host capability limits should be surfaced deliberately rather than discovered
  by failure.

---

## 5. What This Means For Protocol Scope

This note does not argue against more protocols. It argues for a better
admission question.

The next useful question is not:

- "what other protocol can Graphshell add?"

The next useful question is:

- "what browser capability does this protocol addition improve, unlock, or
  pressure-test?"

Examples:

- IRC is valuable because it sharpens the hosted-comms lane.
- Misfin is valuable because it sharpens the Gemini-adjacent contact and
  lightweight hosted-messaging lane.
- Titan is valuable because it sharpens mutation/publication.
- WebFinger is valuable because it sharpens discovery and identity convergence.
- Spacewalk is valuable because it sharpens discovery/freshness.
- Another obscure readable protocol is less valuable if trust, subscriptions,
  source tools, and retention still feel unfinished.

This is the core admission rule:

- browser maturity should improve before protocol breadth expands aggressively,
- additional tiny protocols should justify themselves through a distinctly
  user-felt capability, not through novelty alone.

---

## 6. Recommended Priority Order

### 6.1 First priority: browser maturity floor

Most urgent:

1. trust and certificate UX
2. subscription/source-health operations
3. page/source/site tools
4. retention and artifact-boundary model

These are the features that make existing protocol support feel robust.

### 6.2 Second priority: discovery and orientation

Next:

1. discovery/freshness/clustering/search separation
2. wayfinding surfaces and discovery packs
3. provenance and explainability for routing and surfaced results

These are the features that make Graphshell's graph-native advantages legible.

### 6.3 Third priority: participation lanes

Then:

1. IRC as hosted public comms
2. Misfin/contact follow-on evaluation
3. Titan and other mutation/publication workflows
4. Bubble-like posting/issue participation if and when it becomes worth doing

These are the features that turn Graphshell from a reader into a participant.

### 6.4 Fourth priority: additional protocol breadth

Only after the above is clearer should Graphshell aggressively broaden protocol
surface area beyond the strongest current candidates.

---

## 7. Executable Slices

### 7.1 Slice A: trust and source-health baseline

Deliverables:

- trust-state UX note,
- subscription manager/source-health note,
- certificate failure and rotation handling rules,
- client certificate management expectations

### 7.2 Slice B: source and page tools

Deliverables:

- faithful-source vs styled-view rules,
- page outline and capsule structure tools,
- search-in-page and alternate-open actions,
- save/download/open-externally behavior,
- explicit surface-switching ergonomics for faithful, assistive, feed, split,
  and external views

### 7.3 Slice C: discovery and wayfinding

Deliverables:

- separate ingest models for discovery, freshness, clustering, and search,
- wayfinding surface classification,
- provenance display rules for why a result is present

### 7.4 Slice D: retention and archive model

Deliverables:

- cache/saved/offline-reading/captured/transcript/dataset artifact taxonomy,
- explicit consent and served-to-user boundaries,
- offline and saved-reading model

### 7.5 Slice E: comms and mutation follow-on

Deliverables:

- hosted-comms behavior matrix,
- IRC lane follow-on details,
- mutation/publication action model for Titan and related lanes

### 7.6 Slice F: host-capability closure

Deliverables:

- host-by-host support matrix for smolweb capabilities,
- degradation visibility rules,
- routing and capability-signaling guidance

---

## 8. Final Recommendation

Graphshell should treat "smolweb browser maturity" as a real product goal of
its own.

The project is already good at spotting compelling protocols, services, and
adjacent ideas. The next step is to harden the capabilities that make those
ideas durable:

- trust,
- subscriptions,
- source tools,
- discovery separation,
- wayfinding,
- retention boundaries,
- scoped communication,
- mutation and publication,
- explainable routing,
- host-aware degradation.

That is how Graphshell avoids becoming either:

- a protocol collector, or
- a beautiful renderer with too little browser reality around it.

It is also how the project earns the right to keep expanding the smolweb
surface with confidence.
