> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-18 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/verso_docs/research/2026-03-28_smolnet_dependency_health_audit.md`
> - Source commit: `b86a3906`
>
> None of it was ever built. It is filed here as research, not as a plan and
> not as a statement of where Turnstone is going. It speaks the donor
> vocabulary (Graphshell as the browser, Verso as the peer runtime holding the
> small-protocol lane) and its relative links point at donor docs that no
> longer exist locally. The current position on this ground is
> `2026-08-17_smolweb_browser_gap_analysis.md` in this folder. Read the text
> below for the ideas, not the direction.
>
> Sibling: `2026-04-16_smolnet_capability_model_and_scroll_alignment.md`,
> recovered into this folder in the same pass, is the architecture half, and
> its caveats defer specific protocol decisions back to this rubric. The
> architecture-fit audit named at the top of this file
> (`2026-03-28_smolnet_follow_on_audit.md`) was not recovered and survives only
> in the archive's history.
>
> The rubric outlived its own open verdicts. Every candidate it declines to
> rule on (Titan, Spartan, Misfin, Nex, Guppy) was later answered the way its
> default bias pointed, with an owned engine in genet's `nematic` component
> rather than an adopted third-party crate.

---

<!-- This Source Code Form is subject to the terms of the Mozilla Public
     License, v. 2.0. If a copy of the MPL was not distributed with this
     file, You can obtain one at https://mozilla.org/MPL/2.0/. -->

# Smolnet Dependency Health Audit

**Date**: 2026-03-28
**Status**: Research / Dependency Triage
**Purpose**: Define the dependency-health criteria for follow-on smallnet protocol work in Verso, record the current repo-local posture, and make explicit which protocol candidates still require external ecosystem validation before dependency adoption.

**Related**:

- [`2026-03-28_smolnet_follow_on_audit.md`](2026-03-28_smolnet_follow_on_audit.md) — architecture-fit audit for Titan, Spartan, Misfin, Nex, and Guppy
- [`../technical_architecture/VERSO_AS_PEER.md`](../technical_architecture/VERSO_AS_PEER.md) — Verso runtime and current small-protocol boundary
- [`../implementation_strategy/2026-03-28_gemini_capsule_server_plan.md`](../implementation_strategy/2026-03-28_gemini_capsule_server_plan.md) — current implementation baseline for Gemini/Gopher/Finger
- [`../../graphshell_docs/implementation_strategy/viewer/2026-03-08_simple_document_engine_target_spec.md`](../../graphshell_docs/implementation_strategy/viewer/2026-03-08_simple_document_engine_target_spec.md) — `SimpleDocument` bridge boundary

---

## 1. Purpose

The follow-on audit answers whether a protocol is a good architectural fit.

This document answers a different question:

- if Graphshell decides a protocol is worth supporting, should Verso adopt an external Rust implementation, or is a local implementation the healthier choice?

This distinction matters because small protocols often look simple enough to implement locally, but their surrounding ecosystem can still impose hidden maintenance costs.

---

## 2. Current Constraint

This session does **not** include authoritative external ecosystem validation for:

- Titan
- Spartan
- Misfin
- Nex
- Guppy

So this document does **not** make final crate recommendations for those protocols.

Instead, it defines the dependency-health rubric and records the current default posture:

- prefer existing landed dependencies when they already solve the problem cleanly
- prefer local implementations for very small, legible protocol surfaces when doing so keeps Verso simpler
- adopt a third-party crate only when it clearly lowers maintenance risk rather than hiding it

---

## 3. Baseline Dependency Posture

The current small-protocol lane already demonstrates the repo's practical stance.

Current landed or planned dependencies in the Gemini/Gopher/Finger slice include:

- `tokio-rustls`
- `rcgen`

The rest of the runtime surface is intentionally local to Verso:

- protocol-specific servers under `mods/native/verso/*`
- protocol-specific serializers on top of `SimpleDocument`
- GraphIntent wiring and registry integration inside the repo

That is a useful baseline rule:

- small, well-bounded protocol behavior is a reasonable candidate for local implementation
- TLS, certificate, and other lower-level transport machinery should still prefer established libraries rather than bespoke cryptography or transport code

---

## 4. Dependency-Health Rubric

Any external crate considered for a follow-on smallnet protocol should be evaluated against all of the following.

### 4.1 Maintenance cadence

- Is the crate actively maintained?
- Are releases recent enough to trust it in a live integration path?
- Is there evidence that bugs and protocol changes are handled in a reasonable timeframe?

### 4.2 Protocol completeness

- Does the crate implement the subset Graphshell actually needs?
- Does it cover both client and server surfaces where relevant?
- Does it hide important protocol details that Graphshell needs to surface to the user?

### 4.3 Dependency surface size

- Does the crate drag in a large or fragile dependency tree?
- Does it introduce native libraries or build complexity that work against Verso's small, inspectable protocol lane?

### 4.4 Runtime fit

- Does the crate fit the repo's async/runtime model?
- Can it be integrated cleanly with existing registry, intent, diagnostics, and lifecycle patterns?
- Does it cooperate with the current TLS/network stack instead of forcing a parallel architecture?

### 4.5 Security posture

- Does the crate handle trust, TLS, identity, and parser boundaries responsibly?
- Would adopting it reduce risk, or merely outsource unexamined risk?

### 4.6 Legibility and overrideability

- Can Graphshell still surface protocol-native trust and routing semantics clearly?
- Can the crate be wrapped without losing the details the UI and diagnostics should expose?

### 4.7 Local-implementation alternative

- Is the protocol so small that a local implementation would be simpler, easier to audit, and more maintainable?
- If yes, an external crate should face a higher bar before adoption.

---

## 5. Default Bias By Protocol Family

### 5.1 Readable and publishable document protocols

Default bias:

- prefer local implementations when the protocol is genuinely small and the protocol-native semantics need to stay visible
- continue relying on established transport/security crates underneath where needed

This matches the current Gemini/Gopher/Finger shape.

### 5.2 Discovery protocols

Default bias:

- prefer standard data-shape and HTTP/TLS libraries over monolithic protocol-specific stacks

WebFinger is the best current example: Graphshell needs structured discovery publication more than it needs a large dedicated runtime stack.

### 5.3 Messaging and mutation protocols

Default bias:

- be more open to external crates if the protocol semantics are subtle enough that local implementation risk outweighs the cost of adoption

This is where Titan, Spartan, and Misfin need careful external validation rather than assumptions.

---

## 6. Candidate Protocol Dependency Posture

## 6.1 Titan

**Architecture fit**: Strong candidate in the follow-on audit.

**Dependency posture now**: No external crate recommendation yet.

What must be validated externally:

- whether a maintained Rust implementation exists
- whether it supports the exact request/upload semantics Graphshell would need
- whether it exposes protocol details clearly enough for publish UX, diagnostics, and permission gating

Current stance:

- Titan should not be blocked on a crate search before the product decision is made
- but no dependency should be adopted until the external audit confirms maintenance quality and integration shape

## 6.2 Spartan

**Architecture fit**: Optional, lower-priority candidate.

**Dependency posture now**: No external crate recommendation yet.

What must be validated externally:

- whether there is a credible Rust implementation at all
- whether adopting it would create a distinct user-facing benefit rather than adding maintenance for a redundant lane

Current stance:

- because Spartan is already lower priority architecturally, dependency adoption should be even more conservative

## 6.3 Misfin

**Architecture fit**: Plausible as a contact/message lane rather than a document lane.

**Dependency posture now**: No external crate recommendation yet.

What must be validated externally:

- whether the Rust ecosystem has a maintained implementation
- whether the crate fits identity/contact workflows without smuggling in a larger app model Graphshell does not want

Current stance:

- evaluate only if the product commits to a lightweight contact/message surface in Verso or adjacent social tooling

## 6.4 Nex

**Architecture fit**: Optional readable directory/document candidate.

**Dependency posture now**: No external crate recommendation yet.

Current stance:

- the architecture-fit audit now places Nex in the lightweight readable directory/document family
- dependency work should still be deferred until there is clearer evidence for either:
- a maintained Rust implementation worth adopting, or
- a product decision that justifies a small local implementation instead
- because Nex appears to be a very small selector/path protocol with directory-listing behavior, the default bias should lean toward local implementation unless an external crate clearly lowers maintenance risk

## 6.5 Guppy

**Architecture fit**: Low-priority readable-document candidate with short-input semantics.

**Dependency posture now**: No external crate recommendation yet.

Current stance:

- the architecture-fit audit now places Guppy in the lightweight document-and-short-input family, but only as a niche candidate
- because Guppy is explicitly plaintext, UDP-based, and acknowledgement/session-driven, dependency selection should be even more conservative than for Nex
- if Graphshell ever supports Guppy, a small local implementation may still be preferable, but only if it can honestly handle retransmission, chunking, and denial-of-service guardrails without obscuring protocol semantics

---

## 7. Recommended External Audit Questions

When a real ecosystem pass is run, each candidate protocol should be checked with the same short questionnaire:

1. Which Rust crates implement this protocol today?
2. Which of those appear maintained and documented?
3. What feature subset do they actually implement?
4. What is the transitive dependency cost?
5. Does the crate integrate with Tokio and the current Verso runtime model?
6. Does it preserve protocol-native trust and routing details well enough for diagnostics/UI?
7. Would a local implementation be simpler and easier to audit?

The goal is not to maximize reuse. The goal is to minimize long-term maintenance surprise.

---

## 8. Summary

The right current posture is:

- keep using established transport/security libraries where they reduce real risk
- keep protocol-shaped behavior local when the protocol is genuinely small and the semantics matter to the user
- avoid adopting follow-on protocol crates until there is an explicit external dependency-health pass

That keeps Verso's smallnet lane small in the good sense: understandable, protocol-honest, and maintainable.
