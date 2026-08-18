> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-18 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/verso_docs/research/2026-04-16_smolnet_capability_model_and_scroll_alignment.md`
> - Source commit: `c59149c9`
>
> None of it was ever built. It is filed here as research, not as a plan and
> not as a statement of where Turnstone is going. It speaks the donor
> vocabulary (Graphshell as the browser, Verso as the peer runtime, Middlenet
> for the smolweb, Verse for the network layer) and its relative links point at
> donor docs that no longer exist locally. The current position on this ground
> is `2026-08-17_smolweb_browser_gap_analysis.md` in this folder. Read the text
> below for the ideas, not the direction.
>
> Sibling: `2026-03-28_smolnet_dependency_health_audit.md`, recovered into this
> folder in the same pass, is the dependency half of the same question, and is
> what section 10 below means by "the dependency health rubric". The
> admission-bar audit both documents lean on
> (`2026-03-28_smolnet_follow_on_audit.md`) was not recovered and survives only
> in the archive's history.
>
> Worth knowing before reading the tiers: every protocol this note ranks Tier A
> or Tier B now has a shipping engine in genet's `nematic` component
> (`nematic.gemtext`, `.gopher`, `.finger`, `.feed`, `.markdown`, `.text`,
> `.spartan`, `.nex`, `.titan`, `.misfin`, `.scroll`, `.guppy`), so the tiering
> was overtaken by events. The capability framing it argues for, routing app
> flows through composed transport and format handlers rather than through
> named protocols, was not adopted.

---

<!-- This Source Code Form is subject to the terms of the Mozilla Public
     License, v. 2.0. If a copy of the MPL was not distributed with this
     file, You can obtain one at https://mozilla.org/MPL/2.0/. -->

# Smolnet Capability Model and Scroll Alignment

**Date**: 2026-04-16
**Status**: Research / Architecture note
**Purpose**: Reframe smolnet support around protocol capabilities rather than
named protocols, define a transport-plus-format architecture for Middlenet
adapters, tier follow-on protocol priorities, and highlight Scroll as a
strategic protocol because its UDC-centric worldview aligns unusually well with
Graphshell and Verse.

**Related**:

- [`2026-03-28_smolnet_follow_on_audit.md`](2026-03-28_smolnet_follow_on_audit.md)
  - admission-bar audit for Titan, Spartan, Misfin, Nex, and Guppy
- [`../../graphshell_docs/technical_architecture/2026-03-29_middlenet_engine_spec.md`](../../graphshell_docs/technical_architecture/2026-03-29_middlenet_engine_spec.md)
  - Middlenet scope, portable engine framing, and protocol/content-space model
- [`../../graphshell_docs/technical_architecture/2026-04-16_middlenet_lane_architecture_spec.md`](../../graphshell_docs/technical_architecture/2026-04-16_middlenet_lane_architecture_spec.md)
  - lane split, async rendering lifecycle, and HTML-lane boundaries
- [`../../graphshell_docs/implementation_strategy/viewer/2026-04-16_smolweb_compliance_and_middlenet_html_contract.md`](../../graphshell_docs/implementation_strategy/viewer/2026-04-16_smolweb_compliance_and_middlenet_html_contract.md)
  - HTML/CSS support contract for Middlenet's HTML lane
- [`../technical_architecture/VERSO_AS_PEER.md`](../technical_architecture/VERSO_AS_PEER.md)
  - current Gemini/Gopher/Finger baseline and Verso placement
- [`../../verse_docs/technical_architecture/VERSE_AS_NETWORK.md`](../../verse_docs/technical_architecture/VERSE_AS_NETWORK.md)
  - Verse community/index posture and UDC-facing network context

**External references**:

- Scroll spec portal: https://portal.mozz.us/gemini/scrollprotocol.us.to/spec.scroll
- Scroll specification text provided in-session on 2026-04-16

---

## 1. Why This Note Exists

The earlier smolnet follow-on audit was intentionally conservative. It asked
which post-Gemini/Gopher/Finger protocols deserved native attention first and
why. That was the right question for an initial admission bar.

What is now needed is a broader architectural lens:

- the smolnet world is larger than Gemini/Gopher/Finger plus a few nearby
  follow-ons
- many named protocols collapse into a small number of capability and format
  families
- Middlenet should scale by composing transport and format handlers rather than
  by adding bespoke protocol stacks one by one
- Scroll deserves special treatment because it is not just another retrieval
  protocol; it natively carries UDC semantics that align with Graphshell's and
  Verse's own classification ambitions

This note therefore complements the earlier audit rather than replacing it.

---

## 2. The Smolnet Landscape by Capability

Viewed by protocol name, the ecosystem looks sprawling. Viewed by capability,
it is much more compact.

### 2.1 Document-retrieval protocols

These protocols primarily answer "request an address, receive a document or
directory-like response":

- Gemini
- Gopher
- Gopher+
- Finger
- Spartan
- Nex
- Scroll
- Mercury
- Scorpion
- Text
- Guppy
- Molerat
- Terse
- FSP
- SuperText

### 2.2 Document-upload or mutation protocols

These protocols extend the "retrieve a document" model with body-bearing
requests or upload semantics:

- Titan
- Spartan

### 2.3 Messaging or mailbox protocols

These protocols are primarily about sending messages rather than retrieving a
document:

- Misfin
- NNCP is adjacent here conceptually, though it is better treated as later
  infrastructure for intermittent or offline-first messaging than as a core
  smolnet document lane

### 2.4 Search and discovery services

These are usually services hosted on existing protocols rather than distinct
wire protocols:

- GUS
- CAPCOM
- Antenna
- Cosmos
- Spacewalk
- Wander

The important architectural consequence is that Graphshell should not ask
"which named protocols do we support?" first. It should ask which capability
families the product needs and then map protocols onto them.

---

## 3. Capability Grid and Protocol Axes

Most smolnet document protocols vary along a handful of axes:

| Axis | Common values |
| --- | --- |
| Transport | Plain TCP / TLS / Noise / UDP with acknowledgement |
| Status model | None / single digit / two digit / small enumerated family |
| Content declaration | Item-type implied / MIME-like header / implicit |
| Content format | Gemtext-like / Gopher menu / plain text / richer gemtext variant / raw |
| Identity model | None / client certificate / protocol-specific identity binding |

This reframing matters because it collapses many "new protocol" requests into a
much smaller implementation surface:

- Gemini and Spartan are near-neighbors once transport and body-bearing request
  differences are isolated
- Scroll is best understood as a Gemini-shaped protocol with additional
  semantic and formatting structure
- Mercury, Text, and Nex simplify parts of the same general retrieval shape
- Guppy changes transport behavior more than it changes document semantics

The capability model Graphshell should expose is therefore something like:

- retrieve document
- publish or upload
- send message
- discover identity or endpoint
- query with user input
- navigate an index or menu

App-level flows should target capabilities, not protocols. "Publish this" or
"query this source" should route through the protocol registry to a capability
provider, rather than baking Titan, Spartan, Gemini, or Gopher-specific logic
into the product surface.

---

## 4. Recommended Middlenet Adapter Architecture

The current lane split in Middlenet is still correct: adapters produce content
and renderers realize it. What needs tightening is the adapter architecture so
it scales across smolnet protocols without becoming a protocol zoo.

### 4.1 `middlenet-transport`

A transport-focused crate SHOULD own wire-level mechanics:

- connection type: TCP, TLS, Noise, UDP-with-ack
- request shape: URL only, URL plus selector, URL plus input, URL plus body
- response framing: no header, single-digit status, two-digit status,
  item-type-prefixed menus, and similar families

In that model, many protocols become configuration plus a small amount of
edge-case handling rather than wholly separate implementations.

### 4.2 `middlenet-formats`

A format-focused crate SHOULD own content decoding into `SemanticDocument`
fragments or deltas:

- gemtext core
- gemtext extensions
- gopher menu
- plain text with links
- feed formats
- Markdown
- raw faithful-source rendering when needed

Gemtext-like variants should share one parser core plus extension hooks rather
than forking early.

### 4.3 `middlenet-adapters`

The adapter crate SHOULD be glue:

- choose transport configuration by scheme or endpoint capability
- choose format parser by content declaration and protocol rules
- produce streamed `DocumentDelta` output into the Middlenet lane system

This keeps protocol comprehensiveness mostly an adapter-level cost instead of a
whole-engine redesign each time.

---

## 5. Coverage Priorities

Not every protocol deserves equal effort. A tiered posture keeps Graphshell
honest and maintainable.

### 5.1 Tier A: worth full support

- Gemini plus Titan
- Gopher
- RSS, Atom, and JSON Feed
- Finger
- Markdown and plain text
- Spartan
- Nex

These either have clear deployment reality, very low incremental cost, or both.

### 5.2 Tier B: strategically interesting once the capability model lands

- Misfin
- Scroll
- Text

These are worth support once the adapter architecture is capability-driven.
Misfin matters because it gives smolnet a real messaging lane. Scroll matters
because of semantic alignment with Verse and Graphshell. Text matters because
it expands the transport and trust surface in a way that is still conceptually
compact.

### 5.3 Tier C: real but lower-priority long tail

- Mercury
- Scorpion
- Guppy
- Gopher+
- SuperText

These are best approached only after the capability model proves itself on the
more important families above.

### 5.4 Tier D: defer until someone has a concrete need

- Molerat
- Terse
- FSP

The product should not overfit the long tail before the core families are
stable.

---

## 6. Why Scroll Is Special

Scroll is not just another protocol on the list.

Its most interesting property for Graphshell is that it treats Universal
Decimal Classification as a first-class content-shaping concern. That is
architecturally unusual in the smolnet world and unusually aligned with Verse,
which already treats UDC as a semantic typing and relevance signal for
community search and indexing.

That makes Scroll interesting in at least three ways:

- **semantic ingestion**: Graphshell can ingest Scroll documents with more
  native structure than generic gemtext because the source is already trying to
  say what kind of thing it is
- **Verse alignment**: UDC-bearing documents map more naturally into Verse
  ranking, filtering, and subject-scoped community indices
- **authoring fit**: Scroll feels closer to Graphshell's worldview than a pure
  "small document over the wire" protocol because it bakes classification into
  the publication surface

For that reason, Scroll SHOULD be treated as a strategic Tier B target even if
its deployment remains smaller than Gemini's or Gopher's.

This does not mean Middlenet should special-case Scroll everywhere. It means
the capability and format architecture should leave room for Scroll's
additional semantics to survive ingestion instead of being flattened away.

### 6.1 `scrolltext` is not just "gemtext plus extras"

The provided specification makes clear that Scroll's document format is a
distinct descriptive markup language, not just an ad hoc Gemini variant.

Important properties for Middlenet:

- MIME type is `text/scroll`
- headings run from `#` through `#####`, with level-5 headings explicitly
  treated as textual titles rather than outline-defining section boundaries
- documents are section-oriented: headings begin sections and imply nested
  outline structure rather than merely decorating nearby paragraphs
- thematic breaks use `---`
- code blocks use triple backticks and optional format tags
- nested quotes and nested lists are first-class
- links use `=>`, while input links use `=:`
- inline emphasis, strong, and code toggles exist but are line-bounded
- line-level escaping is explicit and small enough to parse in a stream
- link relations such as `[Citation]`, `[Cross-reference]`, and polarity
  markers like `[+]` or `[-Citation]` are part of the document model, even if
  clients present them optionally

This is exactly the kind of source format Middlenet should preserve as its own
semantic lane instead of flattening into generic HTML.

### 6.2 Scroll's wire protocol adds meaning beyond Gemini

Scroll also matters at the transport-and-response layer.

The spec provided in-session defines:

- a request line of `<URI> <LanguageList><CRLF>`
- BCP47 language negotiation as a first-class request concern
- Gemini-style status handling for non-success cases
- success responses that include MIME type plus author, publish date, and
  modification date before the body
- metadata requests that return an abstract in scrolltext rather than the full
  resource body
- success codes in the `20`-`29` range whose second digit maps onto top-level
  UDC classes

That last point is unusually important. Scroll does not merely carry UDC as an
optional document annotation; it reflects UDC at the protocol response level.
That makes Scroll unusually compatible with Verse's own subject-typing and
community-indexing ambitions.

### 6.3 Scroll aligns with Graphshell's existing architectural preferences

The provided spec reinforces several Graphshell preferences already present in
the docs:

- **streamability**: scrolltext is explicitly designed to be streamed and
  rendered progressively
- **metadata-first access**: abstract/metadata requests line up well with
  previews, hover cards, and low-cost fetch policies
- **language-aware retrieval**: BCP47 request language lists fit Graphshell's
  desire to respect reader preference rather than relying on site-side state
- **small-protocol honesty**: Scroll keeps the protocol simple while still
  allowing richer semantics than Gemini
- **Titan adjacency**: the spec explicitly recommends Titan support, which
  means Scroll sits comfortably beside the existing Gemini/Titan publication
  worldview rather than replacing it
- **certificate continuity**: TOFU, SNI, and optional client certs all fit the
  trust UX Graphshell is already building around Gemini-shaped lanes

### 6.4 What Middlenet should preserve if Scroll support lands

If Graphshell adds Scroll support, the adapter and document model SHOULD
preserve at least the following instead of collapsing them into plain text:

- heading-derived section hierarchy, including the special "title-like but not
  outline-defining" role of level-5 headings
- section hashes and heading-number-derived local navigation targets
- author, publish date, and modification date metadata lines
- BCP47 language negotiation intent and the language attached to returned
  resources
- UDC class information from success response codes
- link relations and relation polarity where present
- input-link semantics for query-style prompts
- metadata-request abstracts as first-class preview material

These are not just rendering details. They are navigation, indexing, preview,
and trust signals.

---

## 7. Messaging and Data Transfer

### 7.1 Messaging posture

Smolnet's native messaging coverage is thin.

Misfin is the clearest purpose-built lane here and is worth keeping on the
roadmap because it is close enough to Gemini-shaped transport and content
handling to be tractable once the capability model exists.

NNCP is conceptually interesting for Graphshell's long-horizon
offline-capable/permacomputing posture, but it belongs in a later
infrastructure phase rather than the core Middlenet rollout.

### 7.2 Data-transfer posture

The smolnet ecosystem does not provide a compelling large-artifact transfer
story. Titan handles uploads in a Gemini-adjacent way; document protocols
handle document retrieval; none of that substitutes for a proper artifact layer.

Graphshell's answer here remains Verse plus `iroh-blobs`.

The capability model should therefore route by address and intent:

- small document retrieval goes to smolnet or Middlenet protocol handlers
- artifact retrieval goes to Verse-addressed or `iroh`-addressed flows
- the user should not have to care which substrate satisfies the request

This is an architectural complement, not a conflict.

---

## 8. Relationship to the HTML Lane

The smolnet capability model and the smolweb HTML contract are adjacent, not
identical.

- smolnet is about protocol and transport families for lightweight networked
  content
- smolweb is about the constrained HTML/CSS contract for Middlenet's HTML lane

That distinction matters because Graphshell should not silently convert every
small protocol into generic HTML and call the problem solved. Protocol-faithful
documents remain first-class. The HTML lane exists for actual HTML content and
HTML-shaped archives or assistive views, not as a way to erase source protocol
semantics.

The product posture should therefore be:

- faithful source semantics first
- optional assistive enrichment second
- HTML lane when the source is actually HTML or when an archive/export surface
  explicitly calls for it

Scroll is a particularly strong example of this rule. A Scroll document should
normally enter Middlenet as Scroll, not as "HTML-shaped content with a nicer
theme." Export to HTML may still be useful for publication or archive views,
but Graphshell should preserve Scroll-native semantics at the canonical
document-model layer.

---

## 9. What the Docs and Implementation Plans Should Say

This note implies a few documentation and architecture updates:

1. Replace long protocol lists with capability tables where possible.
2. Describe protocol support as the composition of transport and format
   handlers.
3. Keep Tier A/B/C/D priority language explicit rather than implying equal
   priority for every named protocol.
4. Call out Scroll explicitly as a UDC-aligned protocol worth strategic
   attention.
5. Treat Misfin as the main native messaging lane if smolnet messaging grows.
6. Keep Verse and `iroh-blobs` as the artifact-transfer answer rather than
   searching for a smolnet-native large-file story that does not really exist.

---

## 10. Caveats

This note is intentionally architectural rather than ecosystem-statistical.

- protocol-health and deployment-size claims here are directional, not a
  rigorous census
- specific protocol support decisions should still pass through the dependency
  health rubric and the earlier admission-bar audit
- Scroll is highlighted here because of architectural fit, not because of a
  claim that it is widely deployed

That is enough for design direction even where exact community-size figures are
uncertain.
