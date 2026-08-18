> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-18 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/graphshell_docs/implementation_strategy/viewer/2026-04-16_smolweb_compliance_and_middlenet_html_contract.md`
> - Source commit: `401e2fcc`
>
> None of it was ever built. It is filed here as research, not as a plan and
> not as a statement of where Turnstone is going. It speaks the donor
> vocabulary (Graphshell as the browser, Middlenet for the smolweb engine,
> Verse for the network layer) and its relative links point at donor docs that
> no longer exist locally. The current position on this ground is
> `2026-08-17_smolweb_browser_gap_analysis.md` in this folder. Read the text
> below for the ideas, not the direction.
>
> Siblings: `2026-04-16_smolnet_capability_model_and_scroll_alignment.md`,
> recovered into this folder earlier today, names this file in its Related list
> as the HTML/CSS support contract for the Middlenet HTML lane, so recovering
> it closes that reference. `2026-03-28_smolnet_follow_on_audit.md` is in this
> folder from the same pass as this file. The two Middlenet specs it leans on
> most, `2026-03-29_middlenet_engine_spec.md` and
> `2026-04-16_middlenet_lane_architecture_spec.md`, were recovered in the same
> pass to `mere/design_docs/mere_docs/research/`, since the engine architecture
> belongs to Mere rather than to the browser app.
>
> The smolweb.org subset and CSS grading rubric were never adopted as a
> validator-backed contract. Genet renders HTML, and nematic deliberately does
> not own an HTML reader-mode engine.

---

<!-- This Source Code Form is subject to the terms of the Mozilla Public
     License, v. 2.0. If a copy of the MPL was not distributed with this
     file, You can obtain one at https://mozilla.org/MPL/2.0/. -->

# Smolweb Compliance and the Middlenet HTML Contract

**Date**: 2026-04-16
**Status**: Proposed HTML-lane contract; Direct feed/article lane already landed
**Scope**: Adopt smolweb.org's HTML subset, CSS grading rubric, and `w`
subdomain convention as the authoritative content-support contract for
Middlenet's HTML lane. Define validator integration, rendering-guarantee
tiers, discovery signals, and consequences for Verse indexing.

**Related docs**:

- [`2026-04-16_middlenet_lane_architecture_spec.md`](2026-04-16_middlenet_lane_architecture_spec.md)
  — canonical lane split and selection contract
- [`2026-03-29_middlenet_engine_spec.md`](2026-03-29_middlenet_engine_spec.md)
  — Middlenet scope and tiered-content framing
- [`2026-03-30_protocol_modularity_and_host_capability_model.md`](2026-03-30_protocol_modularity_and_host_capability_model.md)
  — host-aware degradation and protocol packaging classes
- [`../implementation_strategy/viewer/universal_content_model_spec.md`](../implementation_strategy/viewer/universal_content_model_spec.md)
  — viewer routing and content selection policy
- [`../../verso_docs/research/2026-03-28_smolnet_follow_on_audit.md`](../../verso_docs/research/2026-03-28_smolnet_follow_on_audit.md)
  — smallnet protocol audit (smolnet, not smolweb; adjacent scope)
- [`../../verso_docs/research/2026-04-16_smolnet_capability_model_and_scroll_alignment.md`](../../verso_docs/research/2026-04-16_smolnet_capability_model_and_scroll_alignment.md)
  — broader smolnet capability-model note; documents where Scroll and other
  lightweight document protocols sit relative to the HTML lane
- [`../../verse_docs/technical_architecture/VERSE_AS_NETWORK.md`](../../verse_docs/technical_architecture/VERSE_AS_NETWORK.md)
  — Verse community/index context

**External references**:

- smolweb.org: https://smolweb.org/
- Guidelines: https://smolweb.org/guidelines.html
- HTML subset: https://smolweb.org/specs/index.html
- CSS grading: https://smolweb.org/css-grading.html
- Validator: https://smolweb.org/validator/
- Validator source: https://codeberg.org/smolweb/smolweb-validator
- Modern Font Stacks: https://modernfontstacks.com/
- XHTML Basic (ancestor): https://www.w3.org/TR/xhtml-basic/

---

## Implementation Note (2026-04-20)

The Direct Lane feed/article surface is now implemented in the repository via
the `middlenet-core` / `middlenet-adapters` / `middlenet-render` /
`middlenet-engine` split and the native `viewer:middlenet` integration.

That means this document should be read narrowly:

- it remains the contract for the future `middlenet-html` lane,
- it does not describe the already-landed feed/article Direct Lane,
- and it should not be used to justify forcing current Middlenet work back
  through HTML before the HTML lane itself exists.

---

## 1. Why Adopt Smolweb as the HTML Lane Contract

Middlenet's HTML lane has had an unbounded scope problem. "Static HTML" is
true but vague; "rich web content minus JavaScript" is also true but still
unbounded. Any attempt to define the lane by exclusion lets the definition
drift.

Smolweb supplies what was missing: a **published, RFC-2119-normative,
community-maintained, validatable subset** of HTML and CSS designed for
lightweight rendering on constrained devices. It is written for the same
reasons Middlenet exists — reduce bloat, respect the reader, work on modest
hardware, degrade gracefully. Its values align.

Graphshell therefore adopts smolweb as the **support contract** for the HTML
lane. "Graphshell renders smolweb-compliant content well" is a concrete,
defensible claim. Everything above smolweb's scope is best-effort or escalated
to the Servo lane. Everything below is a subset.

This is not a claim that all HTML Graphshell ever renders is smolweb. It is a
claim about **what the HTML lane is designed to serve**. Content outside
smolweb may still render — better than text, worse than Servo — but Graphshell
makes no guarantees outside that envelope.

---

## 2. Core Invariants

### 2.1 Smolweb is the HTML lane's target surface

The HTML lane MUST render smolweb-compliant content faithfully. Deviations
from smolweb compliance MAY degrade gracefully but MUST NOT produce silent
rendering errors.

### 2.2 Readability without CSS or JS is preserved

Smolweb requires that pages remain readable without stylesheets or scripts.
The HTML lane MUST preserve this property: a page that reads as plain
structured text without CSS in a compliant browser SHOULD also read as plain
structured text in Graphshell when CSS is disabled or absent.

### 2.3 CSS grade determines rendering guarantees

The lane's rendering commitments are tiered by smolweb CSS grade (see §4).
Grade A–C are fully supported; Grade D is supported with best-effort
performance; Grade E–F may be partially implemented or ignored.

### 2.4 Validation is a first-class content signal

Smolweb validation status MUST be surfaceable as node metadata, searchable in
Verse, and usable by ranking policy. A validated smolweb page is a
higher-confidence HTML-lane candidate than an unvalidated one.

### 2.5 The `w` subdomain convention is honored

Graphshell MUST recognize the `w.*` subdomain signal as a declaration of
smolweb compliance and MAY use it as a lane-routing hint.

---

## 3. HTML Subset Support

The HTML lane MUST support the smolweb subset as its rendering target. The
subset consists of:

### 3.1 Element groups (authoritative list: smolweb specs/index.html)

- **Structural**: `<html>`, `<head>`, `<title>`, `<body>`
- **Semantic**: `<header>`, `<footer>`, `<main>`, `<nav>`, `<section>`,
  `<article>`, `<aside>`, `<details>`, `<summary>`, `<figure>`,
  `<figcaption>`, `<data>`
- **Textual**: `<abbr>`, `<address>`, `<blockquote>`, `<br>`, `<cite>`,
  `<code>`, `<dfn>`, `<div>`, `<em>`, `<h1>`–`<h6>`, `<kbd>`, `<p>`,
  `<pre>`, `<q>`, `<samp>`, `<span>`, `<strong>`, `<time>`, `<var>`
- **Hypertextual**: `<a>`
- **Listing**: `<dl>`, `<dt>`, `<dd>`, `<ol>`, `<ul>`, `<li>`
- **Forms**: `<button>`, `<fieldset>`, `<form>`, `<input>`, `<label>`,
  `<legend>`, `<select>`, `<optgroup>`, `<option>`, `<textarea>`
- **Table**: `<caption>`, `<table>`, `<tbody>`, `<td>`, `<tfoot>`,
  `<th>`, `<thead>`, `<tr>`
- **Media**: `<audio>`, `<img>`, `<source>`, `<video>`
- **Presentation**: `<b>`, `<hr>`, `<i>`, `<small>`, `<sub>`, `<sup>`
- **Metainformation**: `<meta>`, `<link>`, `<base>`, `<noscript>`, `<script>`,
  `<style>`

### 3.2 Explicitly excluded

- `<object>`, `<param>` (banned by smolweb to prevent plugin/applet inclusion)
- Deprecated tags: `<acronym>`, `<big>`, `<tt>`
- External scripts from CDNs or third-party origins (smolweb forbids;
  Graphshell MUST NOT execute these)

### 3.3 Rendering requirements

- The HTML lane MUST render all supported elements with appropriate semantics.
- Semantic tags (`<header>`, `<main>`, `<article>`, etc.) MUST be usable for
  accessibility projection and for structure-aware UI (outline, navigation,
  reader mode).
- `<form>` support MAY be deferred but SHOULD be considered for parity with
  smolweb's self-description of its scope.
- `<script>` tags MAY be rendered as inert text or ignored in the HTML lane;
  script execution is out of scope for this lane (see §10).

---

## 4. CSS Grade → Rendering Guarantee Tiers

Graphshell MUST map smolweb's CSS grades to explicit rendering-support tiers
in the HTML lane. The grading lives at smolweb.org/css-grading.html; this
spec records how Graphshell honors it.

### 4.1 Supported tiers

| Smolweb grade | Graphshell support | Description |
|---|---|---|
| **A** | MUST render correctly | Box model, typography, basic layout, basic background, lists. CSS Mobile Profile 1.0 baseline. |
| **B** | MUST render correctly | `@media` queries, min/max dimensions, inline-block, relative positioning, visibility, direction. |
| **C** | SHOULD render correctly | Border-radius, opacity, box-sizing, text-overflow, CSS variables, overflow, aspect-ratio, gap, object-fit, modern polish. |
| **D** | SHOULD render with best-effort performance | Absolute/fixed positioning, flexbox, box-shadow, text-shadow, `@font-face`, table layout, logical properties. May skip subpixel optimizations. |
| **E** | MAY render partially | CSS Grid, `transform`, `transition`, `position: sticky`, `calc()`, `clip-path`, `contain`. Implementation at Graphshell's discretion; may degrade to simpler layout. |
| **F** | MAY ignore | Animations, filters, 3D transforms, container queries, experimental features. No guarantee of support. |

### 4.2 Fallback behavior

When the HTML lane encounters unsupported CSS:

- Unsupported properties MUST be ignored, not raise errors.
- Ignored properties MUST NOT cascade into broken layout; the page SHOULD
  still be navigable.
- The content MUST remain readable (invariant §2.2).
- The user SHOULD be able to see which grade the page required and which
  grade the current lane supports, for diagnostic purposes.

### 4.3 Design implication: Grade A+B as the real target

The common case for the HTML lane — blogs, articles, documentation, personal
sites, static generators, smallnet HTTP content — is served by Grades A and
B. C is polish. D is "modern but still lightweight." Grade E and above are
where Servo delegation becomes preferable.

The HTML lane's engineering budget SHOULD prioritize exceptional Grade A–C
fidelity over partial Grade E–F coverage. If Grade E content needs to render
well, the Servo lane is available; middlenet does not need to duplicate it.

---

## 5. The `w` Subdomain Convention

Smolweb proposes `w.example.com` as a signal of smolweb compliance,
analogous to `www`. Graphshell MUST honor this convention.

### 5.1 Lane routing

- A URL with a `w.*` subdomain SHOULD be preferentially routed to the HTML
  lane with smolweb assumptions.
- The routing is a hint, not a guarantee; Graphshell MUST still validate what
  it renders.
- Sites without a `w` subdomain MAY still be smolweb-compliant; the subdomain
  is a declaration, not the only evidence.

### 5.2 UI surface

- When a user visits a `w.*` URL, the UI SHOULD display the smolweb
  compliance claim as a status indicator.
- A mismatched claim (site declares `w.*` but fails validation) SHOULD be
  surfaced as a weak trust signal — not an error, but worth noting.

### 5.3 Verse integration

- Verse observation cards for `w.*` URLs SHOULD carry a smolweb-claim flag.
- Community ranking MAY use smolweb compliance as a positive signal for
  text-oriented communities that value lightweight content.

---

## 6. Validator Integration

Smolweb maintains a validator ([smolweb.org/validator/](https://smolweb.org/validator/),
source at [codeberg.org/smolweb/smolweb-validator](https://codeberg.org/smolweb/smolweb-validator)).
Graphshell SHOULD integrate validation as a first-class capability.

### 6.1 Recommended integration points

- **Per-page badge**: display a validation result indicator for the current
  HTML-lane page (valid, warnings, or failing).
- **Verse indexing**: carry validation result as metadata on ObservationCards
  for HTML content. Validated-smolweb is a durable content-quality signal.
- **Community ranking**: communities SHOULD be able to weight smolweb-valid
  content in their RankingWeights.
- **Author workflow**: if Graphshell ever supports authoring HTML (see
  related HTML-as-publication-format discussion in the engine spec §3.3),
  validation SHOULD be a first-class pre-publish check.

### 6.2 Implementation options

- **Bundle the validator**: adopt smolweb-validator (Rust? Go? TBD — verify
  language at implementation time) as a dependency or service.
- **Remote validation**: call smolweb.org/validator/ as a service when
  connectivity permits.
- **Local re-implementation**: write a small Rust validator against the
  published grammar for offline and constrained-envelope use.

The three options are not mutually exclusive. Local validation is preferable
for privacy (queries stay local) and latency; remote is a fallback.

### 6.3 Validation result model

A validation result SHOULD carry:

- Overall status (valid / warnings / failing)
- HTML subset deviations (list of out-of-subset elements or attributes)
- CSS grade profile (highest grade used, counts per grade)
- Script/external-resource findings
- `w` subdomain consistency check

This mirrors the RankingWeights axes in the lane architecture spec and lets
communities set policy (e.g., "HTML lane content must be Grade A–C, valid,
with no external scripts").

---

## 7. Consequences for Verse

Smolweb compliance is not merely a rendering concern; it is a signal that
travels through Verse's ObservationCard, SplitPackage, and CommunityManifest
layers.

### 7.1 ObservationCard additions

For HTML content, ObservationCards MAY include:

- `smolweb_claim: bool` — whether the source declared smolweb compliance
  (e.g., `w.*` subdomain, explicit meta)
- `smolweb_valid: Option<ValidationResult>` — local validation outcome at
  capture time
- `css_grade_profile: Option<GradeProfile>` — highest grade used, grade
  histogram

These are optional fields; non-HTML content carries `None`.

### 7.2 CommunityManifest additions

Communities MAY declare smolweb preferences:

- `smolweb_required: bool` — community only accepts smolweb-valid HTML
  contributions
- `max_css_grade: Option<Grade>` — community caps rendering complexity
- `validation_on_ingest: bool` — whether validator runs as part of
  contribution acceptance

This fits the existing ValidationPolicy primitive in the lane architecture
spec.

### 7.3 Ranking implications

A community weighted toward smolweb compliance produces search results where
lightweight, respectful, readable content ranks above bloated alternatives.
This is a meaningful editorial stance, not merely a technical preference.
Communities choose it; the protocol supports it.

---

## 8. Comms Lane: Smolweb and Feeds

Smolweb's guidelines explicitly recommend RSS feeds as the primary discovery
and follow mechanism: "Smolweb surfers like to have the possibility to follow
you without a specific social network." RSS/Atom is the smolweb social fabric.

Graphshell's Middlenet already covers feeds. Smolweb adoption reinforces this:

- Feed support is not an optional middlenet feature; it is the expected
  discovery mechanism for smolweb-authored content.
- The Feeds Direct Lane (gemtext-ish → SemanticDocument) serves smolweb feeds
  well without needing the HTML lane.
- `<link rel="alternate" type="application/rss+xml">` in smolweb page heads
  SHOULD be auto-detected and offered as a subscription surface.

The smolweb community's IRC channel (`#smolweb` on Libera.Chat) is an
organic comms lane, handled via Graphshell's existing IRC work. No new comms
protocol is required.

---

## 9. Lane Selection With Smolweb Signals

The lane architecture spec §6 defines lane selection. Smolweb adds signals:

### 9.1 Updated routing

| Signal | Lane preference |
|---|---|
| `w.*` subdomain | HTML lane (smolweb assumptions) |
| smolweb-valid at capture | HTML lane, confident |
| HTML with Grade A–C only | HTML lane, confident |
| HTML with Grade D | HTML lane, best-effort |
| HTML with Grade E+ or fullnet JS | Servo lane |
| Broken/unparseable | Raw Source lane |

### 9.2 Override semantics

A user MAY override lane selection for smolweb-claiming content (e.g., to
force the Servo lane for a `w.*` site that happens to require richer
rendering than the HTML lane supports). Graphshell MUST explain the
consequence (lost smolweb guarantees, full browser overhead).

---

## 10. Scripts, External Resources, and Security

Smolweb's Content Security Policy stance is strict:

- JavaScript is allowed only from the same host
- External scripts from other origins or CDNs are forbidden
- Sites MUST remain usable without JavaScript
- `<noscript>` fallbacks are recommended

Graphshell's HTML lane inherits this stance:

- **The HTML lane MUST NOT execute JavaScript from any origin.**
  If a page requires JS to function, it escalates to the Servo lane.
- **The HTML lane MUST NOT load external resources from origins other than
  the page's host, except for explicitly user-approved exceptions.**
  This matches smolweb's CSP defaults.
- `<script>` tags in HTML-lane content MAY be rendered as inert (displayed
  as code, ignored for execution) or stripped entirely.
- `<noscript>` content MUST be rendered in the HTML lane (since no script
  is running).

This is stricter than a general HTML renderer but aligns with smolweb and
with Graphshell's trust posture.

---

## 11. Non-Goals

- Graphshell does not require every HTML URL to be smolweb-compliant.
  Non-compliant content still opens — in the HTML lane with degraded
  guarantees, or in the Servo lane.
- Graphshell does not advertise itself as a smolweb browser. It is a
  graph-first multi-lane browser where the HTML lane happens to be
  smolweb-shaped.
- Graphshell does not enforce smolweb on authors. Markdown remains the
  inward-facing authored format; if HTML authoring is added later, smolweb
  compliance is recommended but not mandatory (see engine spec §3.3).
- Graphshell does not gate search results on smolweb compliance globally.
  Communities may; the protocol does not.

---

## 12. Open Questions

- **Validator dependency**: which validator implementation to adopt or
  bundle. Check smolweb-validator's language, license, and WASM-viability.
- **CSS grade computation at runtime**: Graphshell needs to know what grade
  it's rendering. Is this computed from the stylesheet up front (static
  analysis) or observed during rendering (instrumentation)?
- **Interaction with Blitz-internals plan**: if the HTML lane uses blitz-dom
  + Stylo + a custom WebRender backend, does Stylo naturally support grade
  computation, or is a pre-pass required?
- **Partial support signaling**: when the HTML lane encounters Grade E
  content and can't render it well, how is that surfaced? A "rendered
  simplified" indicator? An offer to escalate to Servo?
- **Form handling**: smolweb includes forms. The HTML lane may need to
  decide between rendering forms as visible-but-inert, supporting GET
  submissions, or escalating to Servo for any form interaction.

---

## 13. Execution Slices

### Slice A: Subset recognition

- ingest smolweb HTML subset as the authoritative element/attribute list
- teach middlenet-adapters to flag out-of-subset constructs without crashing
- surface subset deviations in diagnostics

### Slice B: CSS grade mapping

- import smolweb CSS grade table
- classify stylesheet rules by grade during parse
- expose grade profile as document metadata

### Slice C: `w` subdomain signal

- detect `w.*` subdomain at URL resolution
- route with smolweb assumptions when present
- display compliance claim in UI

### Slice D: Validator integration

- decide between bundled, remote, or local-reimplementation validator
- wire validation result into ObservationCard and UI
- expose in Verse search result cards

### Slice E: Community policy

- add `smolweb_required` and `max_css_grade` to CommunityManifest
- wire into validation policy and ranking weights

### Slice F: Comms/feed linkage

- auto-detect `<link rel="alternate" type="application/rss+xml">` in HTML-lane
  content
- offer feed subscription as a first-class action next to the page
- ensure smolweb sites' recommended RSS-based following just works

---

## 14. Summary

Smolweb solves the scope problem of Graphshell's HTML lane. It converts
"how much HTML do we render?" from an open-ended engineering question into a
**published, validatable, community-backed subset** with a **graded CSS
rubric**. By adopting smolweb as the HTML lane's support contract, Graphshell
gets:

- a concrete definition of "middlenet-appropriate HTML",
- a rubric for CSS complexity budgeting (Grade A–C as the real target),
- a validator as an ecosystem tool rather than one Graphshell has to invent,
- a content-quality signal that flows through Verse,
- a discovery signal (`w.*`) for preferential routing,
- alignment with an existing community whose values match Graphshell's.

Smolweb does not replace the lane architecture. It populates the HTML lane's
requirements with real content.
