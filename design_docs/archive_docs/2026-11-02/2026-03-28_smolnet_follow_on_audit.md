> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-18 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/verso_docs/research/2026-03-28_smolnet_follow_on_audit.md`
> - Source commit: `401e2fcc`
>
> None of it was ever built. It is filed here as research, not as a plan and
> not as a statement of where Turnstone is going. It speaks the donor
> vocabulary (Graphshell as the browser, Verso as the peer runtime holding the
> small-protocol lane, MiddleNet as the engine serving it) and its relative
> links point at donor docs that no longer exist locally. The current position
> on this ground is `2026-08-17_smolweb_browser_gap_analysis.md` in this
> folder. Read the text below for the ideas, not the direction.
>
> This is the admission-bar audit that the two smolnet documents recovered into
> this folder earlier today both lean on and both recorded as still orphaned.
> `2026-03-28_smolnet_dependency_health_audit.md` is the dependency half, and
> is the separate external-criteria pass that section 9 below asks for.
> `2026-04-16_smolnet_capability_model_and_scroll_alignment.md` is the
> architecture half, and defers its specific protocol decisions back to this
> rubric. Recovering it closes both references.
> `2026-03-28_rss_feed_graph_model.md`, recovered beside it in the same pass,
> opens by measuring a feed format against the admission bar defined here and
> concluding that the bar does not apply to a content format served over HTTP.
>
> Its own open verdicts were overtaken by events. Every candidate audited below
> (Titan, Spartan, Misfin, Nex, Guppy) now has a shipping engine in genet's
> `nematic` component, each answered the way section 4.5's default bias
> pointed, with an owned implementation rather than an adopted third-party
> crate.

---

<!-- This Source Code Form is subject to the terms of the Mozilla Public
     License, v. 2.0. If a copy of the MPL was not distributed with this
     file, You can obtain one at https://mozilla.org/MPL/2.0/. -->

# Smolnet Follow-On Audit

**Date**: 2026-03-28
**Status**: Research / Design Exploration
**Purpose**: Audit follow-on smallnet protocol candidates for native Verso support after the current Gemini/Gopher/Finger baseline, and define the admission bar for future protocol work.

**Related**:

- [`../technical_architecture/VERSO_AS_PEER.md`](../technical_architecture/VERSO_AS_PEER.md) — Verso runtime boundary and current Gemini/Gopher/Finger posture
- [`../implementation_strategy/2026-03-28_gemini_capsule_server_plan.md`](../implementation_strategy/2026-03-28_gemini_capsule_server_plan.md) — current small-protocol implementation baseline
- [`2026-04-16_smolnet_capability_model_and_scroll_alignment.md`](2026-04-16_smolnet_capability_model_and_scroll_alignment.md) — broader capability-model follow-on: transport-plus-format framing, Tier A/B/C/D protocol priorities, and Scroll's UDC alignment with Verse
- [`../../graphshell_docs/implementation_strategy/viewer/2026-03-08_simple_document_engine_target_spec.md`](../../graphshell_docs/implementation_strategy/viewer/2026-03-08_simple_document_engine_target_spec.md) — `SimpleDocument` boundary and block model
- [`../../graphshell_docs/implementation_strategy/system/register/protocol_registry_spec.md`](../../graphshell_docs/implementation_strategy/system/register/protocol_registry_spec.md) — protocol registry opt-in posture
- [`2026-03-28_permacomputing_alignment.md`](2026-03-28_permacomputing_alignment.md) — current Verso research framing for small protocols as living capability lanes
- [`../../verse_docs/research/2026-02-22_aspirational_protocols_and_tools.md`](../../verse_docs/research/2026-02-22_aspirational_protocols_and_tools.md) — earlier alternative-web ecosystem survey

---

## 1. Purpose

Verso already has a real small-protocol lane: Gemini, Gopher, and Finger servers exist, and their publication path is wired into the runtime.

What is still missing is an explicit rule for why those protocols were admitted and what should happen next.

This document fills that gap by:

- defining the admission bar for native small-protocol support in Verso
- distinguishing readable/publishable document lanes from discovery or messaging lanes
- auditing Titan, Spartan, Misfin, Nex, and Guppy for architectural suitability
- recording where the current repo context is sufficient for a suitability decision and where external ecosystem validation is still required

This is an architecture-fit audit, not a final dependency-selection document.

---

## 2. Current Baseline

The current Verso posture is already clear in code and docs:

- **Servo** is the browser engine and general fallback renderer for HTTP/HTTPS and rich web content.
- **Gemini** is the primary modern smallnet document and publication lane.
- **Gopher** is the minimal plaintext document lane.
- **Finger** remains a legacy compatibility lane rather than a preferred modern publication target.
- **WebFinger** is the preferred modern discovery replacement for public identity/contact discovery.

Current reality from the landed slice:

- Gemini/Gopher/Finger all fit a **text-first document protocol** shape.
- Gemini/Gopher/Finger all benefit from the same runtime patterns: route registration, privacy gating, protocol-aware publishing, and optional serialization through `SimpleDocument`.
- WebFinger already belongs in the social/discovery story, but not as a native viewer/document-rendering protocol.

This means Verso does not need a fresh philosophy pass. It needs a sharper selection rubric for follow-ons.

---

## 3. Core Position

Verso should use:

- **Servo** when the browser engine is actually the right tool: HTTP/HTTPS, rich web apps, and general-purpose fallback rendering.
- **Native protocol-shaped runtime support** only when a protocol gives the user something meaningful that Servo fallback does not honestly preserve.

That user-felt value usually means one or more of the following:

- protocol-native trust display
- native menu/document semantics
- direct graph mapping from selectors or links
- protocol-native publishing and round-tripping without HTML flattening
- a simpler and more honest authoring or serving surface

If those benefits cannot be named concretely, Servo fallback is probably enough.

---

## 4. Admission Bar

A protocol should receive native Verso support only if it clears all of the following:

### 4.1 User-felt benefit

The user can perceive a concrete advantage over Servo fallback.

Examples:

- Gemini certificate and route semantics matter to the experience.
- Gopher selector/menu structure should remain visible as such, not as synthetic HTML.
- A publish action should target the protocol's own idiom rather than an HTML bridge.

### 4.2 Stable capability-family fit

The protocol fits a reusable runtime family rather than demanding a one-off architecture.

The important family question is not "is this protocol small?" but "what kind of thing is it?"

### 4.3 Clear trust model

The protocol's security posture and identity assumptions must be intelligible enough to surface in diagnostics and UI.

If Graphshell cannot explain whether a lane is plaintext, certificate-bearing, identity-bound, or best-effort compatibility, the lane is not ready.

### 4.4 Small enough maintenance surface

The protocol must be small enough to implement or integrate without dragging in a second sprawling platform stack that undermines Verso's modularity.

### 4.5 Honest ecosystem posture

There must be either:

- a credible maintained Rust implementation worth adopting, or
- a protocol simple enough that a local implementation is plausibly lower-risk than depending on an unstable crate.

This document does **not** claim that crate-quality validation has been completed for the follow-ons below.

---

## 5. Capability Families

The next important step is to group protocols by runtime shape rather than by cultural adjacency.

### 5.1 Readable document protocols

Primary job: retrieve and display lightweight content.

Current members:

- Gemini
- Gopher
- Finger

Shared traits:

- fetch or accept text-first content
- expose links, selectors, or named lookup semantics
- support native rendering without depending on full browser semantics
- can often project to or from `SimpleDocument`

### 5.2 Publishable document protocols

Primary job: serve Graphshell content outward in a lightweight protocol-native form.

Current members:

- Gemini
- Gopher
- Finger

Shared traits:

- route registration
- privacy/access gating
- protocol-specific serializer
- status/receipt reporting

### 5.3 Discovery protocols

Primary job: describe identity and endpoint discovery, not browsing.

Current member:

- WebFinger

Shared traits:

- identity subject mapping
- structured discovery documents
- no requirement for a native document viewer

### 5.4 Messaging and mutation protocols

Primary job: transmit messages, uploads, or protocol-native state changes rather than simply serve documents.

Candidate members:

- Titan
- Spartan, if used for submit/update semantics
- Misfin

Shared traits:

- explicit send, upload, or mutation flows
- stronger action semantics than passive serving
- often poor fit for `SimpleDocument` as a primary model

---

## 6. `SimpleDocument` Boundary

`SimpleDocument` is already a productive bridge for the current lane, but it should not silently become a universal forcing function.

Good uses of `SimpleDocument`:

- fallback rendering for text-first document protocols
- export/import bridge between lightweight protocols and Graphshell-native surfaces
- simple authoring substrate for small document publication

Bad uses of `SimpleDocument`:

- forcing discovery protocols into fake document shapes
- treating messaging or submit protocols as if they were just pages with another transport
- using it to flatten protocol semantics that the user should still be able to see

Rule:

- `SimpleDocument` is a bridge and export substrate where it fits.
- `SimpleDocument` is not the mandatory center of every future smallnet protocol family.

---

## 7. Protocol Audits

## 7.1 Titan

**Primary family**: Messaging/mutation, with strong adjacency to the Gemini document lane.

**Suitability**: Strong candidate.

Why it fits:

- Titan extends the already-admitted Gemini ecosystem rather than inventing a wholly separate lane.
- It is a plausible next step if Graphshell wants native smallnet upload or submit semantics instead of read-only capsule serving.
- The user-felt benefit is concrete: protocol-native publish or update actions rather than forcing everything through HTML or a side-channel upload tool.

Risks:

- It pushes Verso beyond passive serving into explicit write/mutation semantics.
- It likely needs progress, failure, and permission UX beyond the current Gemini/Gopher/Finger server shape.

Recommendation:

- **Admit as the best next research-and-implementation candidate** once the current Gemini/Gopher/Finger/WebFinger shape is stabilized.

## 7.2 Spartan

**Primary family**: Messaging/mutation or lightweight document transfer, depending on how the project chooses to use it.

**Suitability**: Plausible, but lower priority.

Why it might fit:

- It appears to remain in the same broad smallnet culture and could support very small request/response semantics.

Why it is weaker than Titan:

- Graphshell already has Gemini as the primary modern lane.
- Graphshell already has Gopher as the minimal plaintext lane.
- Without a specific user-facing workflow, Spartan risks being "another tiny protocol" with no clear experiential win.

Recommendation:

- **Keep as optional follow-on research**, not as an immediate implementation priority.
- Require a documented user-felt benefit before admission.

## 7.3 Misfin

**Primary family**: Messaging/contact.

**Suitability**: Plausible, but not as a document lane.

Why it might fit:

- The current social/profile work already distinguishes discovery, profile publication, and protocol-native identity surfaces.
- A lightweight contact/message protocol could fit that broader smallnet identity story better than it fits the current `SimpleDocument`-centric document lane.

Why it should be treated carefully:

- It does not naturally read as "just another capsule document protocol."
- If adopted, it should probably plug into social/profile and contact semantics rather than be forced into the same architecture as Gemini/Gopher/Finger.

Recommendation:

- **Admit as a candidate for a separate lightweight contact/message lane**, not as a direct continuation of the current document-serving lane.

## 7.4 Nex

**Primary family**: Readable directory/document protocol.

**Suitability**: Plausible, but lower priority than Titan and still weaker than Gemini as a primary lane.

What current evidence now supports:

- Nex is a very small selector/path-over-TCP protocol with no response headers and no TLS.
- The request model is Gopher-like: the client sends the requested path and receives file contents directly.
- Current protocol descriptions are more specific than the earlier draft implied: if the requested path is empty or ends with a slash, Nex serves a directory listing; those listings use Gemini-style `=>` links rather than Gopher menu records.

Why it might fit:

- It belongs to the same broad text-first browsing family as Gemini/Gopher rather than to discovery or messaging.
- It preserves a user-visible hierarchy and directory/document distinction that is closer to Gopher than to generic plaintext fetch.
- It looks small enough that local implementation may be more realistic than adopting a large external stack, assuming product value exists.

Why it remains lower priority:

- The evidence currently points to Nex being a very small directory/document lane, not a lane with a clearly differentiated trust, identity, or publishing model that Graphshell users would strongly feel.
- Graphshell already has Gopher for selector-oriented plaintext browsing and Gemini for the preferred modern lane. Nex now looks less like a missing category and more like a nearby variant that must justify itself with a concrete workflow.
- Available evidence is stronger on simple serving and browsing semantics than on Graphshell-specific authoring value or a distinct publication story.

Recommendation:

- **Reclassify from research-only to optional readable directory/document candidate**.
- Do not prioritize implementation ahead of Titan or Misfin.
- Admit only if a specific user-visible benefit is documented, such as materially simpler local serving, Gopher-adjacent directory browsing that Graphshell wants to preserve natively, or compatibility with a target community Graphshell explicitly wants to interoperate with.

## 7.5 Guppy

**Primary family**: Readable document protocol with short-input semantics.

**Suitability**: Real but niche candidate; architecturally understandable, product-wise weak.

What current evidence now supports:

- Guppy has a public specification and reference implementations, not just scattered mentions.
- The protocol spec is explicit that Guppy is an unencrypted UDP client/server protocol for downloading text and text-based interfaces that may require short user input.
- Guppy uses URL requests, MIME-typed responses, redirects, error packets, and an input-prompt packet; it is a document retrieval protocol with lightweight interaction, not a chat or real-time streaming protocol.
- The response model is packet-chunked and acknowledgement-driven, so its main novelty is transport and session shape rather than a messaging or social model.

Why it might fit:

- It is clearly a lightweight text-document protocol rather than discovery, chat, or messaging.
- Its gemtext/plaintext response model and short-input affordance are close enough to the current smallnet viewer lane that native handling is conceptually understandable.

Why it is a weak candidate:

- The lack of encryption cuts against the current Graphshell preference for secure publish lanes and makes Guppy harder to justify as anything beyond optional compatibility.
- Because Graphshell already has Gopher for minimal plaintext compatibility and Gemini for the preferred modern secure lane, Guppy currently looks more like an interesting transport experiment than a necessary product surface.
- UDP chunking, acknowledgements, and session management also mean the implementation complexity is not as trivially small as the word "simple" might suggest.
- The user-felt benefit over the existing Gemini/Gopher split is still thin: faster small-document transfer or lightweight input handling is not enough unless it changes the experience in a way Graphshell intends to expose.

Recommendation:

- **Reclassify from research-only to low-priority readable-document candidate**.
- Keep it below Nex in priority because its plaintext UDP posture is harder to align with the current secure-by-default publication story.
- Consider it only as an explicit compatibility or experimentation lane for small text documents plus short input, not as a primary publish target.

---

## 8. Recommended Priority Order

Recommended order for actual follow-on work:

1. finish and stabilize the current Gemini/Gopher/Finger/WebFinger posture
2. evaluate Titan as the strongest next candidate if native mutation/upload semantics matter
3. evaluate Misfin if lightweight contact or messaging matters to the social/profile surface
4. treat Spartan as optional and justify it with a specific user workflow before admission
5. treat Nex as an optional low-priority readable directory/document lane after a concrete user workflow is identified
6. treat Guppy as a niche compatibility or experimentation lane for small text documents plus short input only, and only after an explicit plaintext/UDP justification is written down

This is intentionally conservative. The point of the current lane is to stay small, legible, and maintainable.

---

## 9. External Validation Gap

This audit is intentionally limited to architectural suitability using current repo context.

It does **not** claim that the Rust ecosystem has already been validated for:

- Titan
- Spartan
- Misfin
- Nex
- Guppy

That should be a separate research slice with external dependency-health criteria such as:

- maintenance cadence
- documentation quality
- protocol completeness
- platform support
- integration complexity
- security posture

Until that pass exists, this document should be read as a suitability audit, not a dependency recommendation.

---

## 10. Lagrange Precedent

Lagrange ([git.skyjake.fi/gemini/lagrange](https://git.skyjake.fi/gemini/lagrange)) is the most complete
smallnet browser reference implementation and directly informs the MiddleNet
engine's protocol strategy.

### 10.1 Protocol support matrix

Lagrange supports all protocols under consideration:

| Protocol | Lagrange support | Security | Notes |
|---|---|---|---|
| Gemini | Full, primary | TLS (TOFU) | |
| Gopher | Full | Plaintext TCP | Converts to intermediate doc model; faithful-source override |
| Finger | Full | Plaintext TCP | Rendered as plain text |
| Spartan | Full (v1.13) | Plaintext TCP | Shared upload dialog with Titan |
| Titan | Full (v1.6) | TLS (Gemini-adjacent) | Upload dialog; Gemtext composer; auto-save |
| Nex | Full | Plaintext TCP | Gemini-style `=>` links in directory listings |
| Misfin | Full (v1.18) | TLS | `text/gemini` + 3 new line types; messaging |
| Guppy | Mentioned | UDP (no TLS) | Low priority; transport experiment |

### 10.2 Single intermediate document model

Lagrange's key architectural decision: **all protocols parse to a common
intermediate document model (`GmDocument`)**, after which the renderer is
format-agnostic. Their own documentation states:

> "After this point the content is source format agnostic. If we had, say, a
> Markdown document, it could be laid out similarly and the page renderer
> should be able to handle it just fine."

The pipeline is:

```
Protocol-specific fetch + parse → GmDocument (layout, text runs) → GPU renderer
```

For the MiddleNet engine, the equivalent is:

```
Protocol-specific fetch + parse → DOM tree + CSS rules → Taffy/Stylo/WebRender
```

Each protocol maps its native semantics to the common document model:
- Gemini gemtext → heading, paragraph, link, preformat blocks
- Gopher menu → link list + preformat blocks (with optional heuristic conversion)
- Finger → plain text body
- Misfin → `text/gemini` + messaging line types (new block types in the document model)

### 10.3 Gopher rendering: conversion with faithful fallback

Lagrange converts Gopher menus to Gemtext-equivalent structure using heuristic
autodetection. Gemtext markers found in raw Gopher data are escaped to prevent
bleed-through. A user preference disables autodetection for faithful source
display.

**The MiddleNet engine should follow this exactly:**
- Default: convert Gopher menu to the common document model (links, preformat,
  paragraphs)
- Faithful-source mode: render raw Gopher as monospace preformatted text
- Escape any characters that would be misinterpreted by the document model

### 10.4 Security posture: warn on failure, inform on absence

Lagrange warns on encryption *failures* (untrusted certificate banners) rather
than on encryption *absence*. Gopher, Finger, Spartan, and Nex appear without
warning banners — but the protocol and its security properties are visible.

**The MiddleNet engine security hierarchy:**

Prefer the secure protocol where there is meaningful overlap:
- Discovery: WebFinger (HTTPS) > Finger (plaintext)
- Modern document: Gemini (TLS) > Spartan (no TLS) for equivalent content
- Upload: Titan (TLS, Gemini-adjacent) > Spartan (no TLS) for equivalent actions
- Messaging: Misfin (TLS) > legacy alternatives

For protocols with no secure analogue (Gopher, Nex, Guppy): render faithfully,
show the protocol name and its plaintext nature as a neutral informational
indicator, not an alarm. The user who navigates to `gopher://` knows what they
are doing.

### 10.5 Revised protocol priority order

In light of Lagrange's full support matrix and the MiddleNet engine context:

1. **Gemini** — primary modern secure document lane (already implemented)
2. **Gopher** — minimal plaintext document lane (already implemented); engine
   converts to document model with faithful-source override
3. **Titan** — best next candidate; Gemini-adjacent TLS upload semantics;
   shared upload dialog with Spartan
4. **Misfin** — TLS messaging; uses `text/gemini`; plugs into social/contact
   story not the document-serving lane
5. **Spartan** — plaintext alternative to Gemini/Titan; value is simplicity;
   shares Titan upload dialog; admit after Titan
6. **Nex** — plaintext directory/document lane; Gemini-style links in listings;
   Gopher-adjacent; low priority
7. **Guppy** — UDP, no TLS; low priority; treat as compatibility/experiment
8. **Finger** — already implemented; WebFinger preferred for identity discovery;
   keep as faithful plaintext compatibility lane

---

## 11. Design Posture Summary

The right smallnet posture for Verso is now fairly clear:

- keep Servo as the general browser engine and fallback renderer
- keep native protocol-shaped support where protocol truth matters to the user
- keep Gemini as the primary modern document lane
- keep Gopher as the minimal plaintext document lane
- keep Finger as a compatibility lane only
- keep WebFinger as the preferred modern discovery lane
- admit new protocols only when they clear a strict bar of user-felt value, stable family fit, and maintainable runtime shape

This prevents the smallnet lane from turning into either:

- aesthetic protocol collecting, or
- a second hidden browser stack inside Verso.
