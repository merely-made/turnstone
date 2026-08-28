# Page capture and provenance

**Status:** plan, 2026-08-28. Serves the capture half of E0 in the
[browser surfaces implementation plan](2026-08-25_browser_surface_implementation_plan.md),
which keeps E0's done-conditions. E0.2's first half, per-node page zoom, was
accepted 2026-08-27; this plan is its successor slice and closes E0 when it
lands.

## What a capture is

A capture freezes **what a person saw on a page**, as a durable artifact
attached to the node they saw it on. It is not a graph artifact:
`graphshell_client::frozen::FrozenScene` freezes a *disclosed graph projection*
(`src/frozen_projection_pane.rs`, `src/a11y.rs:134`) and keeps that meaning.
Two different things are being frozen and the names stay apart.

The deliverable is the **provenance envelope**, not the bytes. A PNG with no
record of which node, address, engine, viewport, scale, and moment produced it
is a loose file in a folder; the same bytes with that record are evidence. The
bytes are the envelope's payload.

## Decisions

These are open and are Mark's. Each names what the codebase already forces, so
the choice is between real options rather than tastes.

### D1. The capture contract is synchronous and mandatory; it cannot stay that way

`SurfaceProducer::capture_snapshot_png(&mut self) -> Result<Vec<u8>, SurfaceError>`
(genet `components/inker/src/surface_engine.rs:888`) is **required** — it has no
default body, so every producer must write it — and **synchronous**, so it must
return pixels before it returns at all.

No CEF-backed engine can honour that. Welding's capture is a two-step async
pair, `request_snapshot_png()` then `poll_snapshot_png()`
(`welding/src/surface.rs:775-790`), because it encodes and copies through
Chromium's DevTools screenshot path. Turnstone's tile therefore answers the
required method with `Unsupported` (`src/shell/weld.rs:500`) — a mandatory
method that the one live hosted engine cannot implement is the seam telling us
the shape is wrong.

Options:

- **A. Replace.** Retire the sync method; put a defaulted async request/result
  pair on both `SurfaceProducer` and `DocumentSession`.
- **B. Supplement.** Keep the sync method for engines that can answer it, add
  the async pair beside it. Two spellings of one capability, and the required
  method still forces every producer to write something.
- **C. Command and event.** A typed request returning a receipt identity, with
  the result arriving as an event — the shape find already uses
  (`DocumentFindChanged`), and the shape welding's PDF path already uses
  (`NavigationEvent::PdfPrintFinished`).

**Recommendation: A carried out as C** — one defaulted typed request, result as
an event carrying the receipt identity, sync method deleted rather than
deprecated (DOC_POLICY §3: no parallel obsolete system). It matches every engine
that exists, it matches the zoom slot pattern that just landed, and the custody
lane it feeds is already asynchronous (see D5).

### D2. Screenshot and print-PDF are different artifacts

Mark's correction, and welding states it in its own contract: `print_to_pdf`
"prints what the page looks like to a printer, not a screenshot — a page that
styles itself for `@media print` gets that" (`welding/src/surface.rs:748-752`).
Welding also keeps PDF export and the system print dialog deliberately separate,
because one produces a file and the other hands control to a platform dialog.

So: **distinct artifact kinds sharing one provenance envelope.** Proposed kinds
are page image (PNG, screen media, screen scale), page document (PDF, print
media), and page source (exact response bytes — see the view-source sidequest).
The decision Mark owns is whether the envelope is one record type with a kind
discriminant, or a kind-specific record per artifact. Recommendation: one
envelope with a kind field, because the provenance questions are identical
across kinds and the ledger should answer them uniformly.

### D3. Which page scale the envelope records

E0.2 made page scale a real per-node value with two halves: the **requested**
scale (persisted, `web.page_scale`) and the **applied** scale (engine-clamped,
transient, known only for engines that report it). An artifact shows the applied
one.

Recommendation: record the applied scale when the engine reports it, fall back
to the requested scale, and **mark which one it is**. A capture that records 5.0
when Livery clamped a request of 12.0 to 5.0 is correct; one that records 12.0
because that is what was asked is a false record.

### D4. Capture scope

Viewport-only or full-page. Chromium can capture beyond the viewport, but
welding does not expose that today, so full-page is engine work, not turnstone
work. Recommendation: **viewport in this slice**, with `capture_scope` in the
envelope from the start, so a later full-page capture is a new value rather than
a schema change.

### D5. Whether a capture writes a user-visible file

Downloads always do both: exact bytes into the session's Muniment blob store,
plus a convenience copy in the download directory (`src/download.rs:211-240`).
A capture is not a download — nobody asked for a file in Downloads — but a
capture nobody can find is not much use either.

Recommendation: **deposit always, copy only on explicit save.** The blob deposit
makes it durable and node-attached; the file copy becomes the Save action of P5.

### D6. Retained capture in this slice, or honestly unsupported

`DocumentSession` has **no** capture method at all today, and every retained
lane reports `page_capture` unsupported (genet
`components/genet-documents/src/engines.rs:43` and `:827`). Retained capture is
net-new the same way retained zoom was.

Livery can in principle be captured: it emits a paint list that netrender
already rasterizes to produce the frame on screen. The question is whether this
slice pays for a rasterize-to-PNG path or ships hosted capture first.

Recommendation: **contract in P1, implementation deferred to P4 and gated on
Mark's call.** Livery keeps its honest unsupported status until the path exists.
E0.2 proved a contract can land ahead of a lane implementing it.

## Phases

### P1. The capture contract

Put the chosen shape (D1) on both traits in inker: a defaulted typed request and
a result carrying the artifact bytes, kind, and the facts only the engine knows
(viewport, applied scale, scope). Retire the mandatory sync method.

Done-conditions:

- Every registered engine reports a typed supported, partial, or unsupported
  page-capture status, and no engine is forced to implement a method it cannot
  honour.
- The result identifies which request it answers, so a second capture cannot be
  mistaken for the first.
- Turnstone compiles against the new contract with capture still unimplemented,
  proving the contract change is separable from the product work.

### P2. Hosted capture through Weld, into custody

Drive `request_snapshot_png` / `poll_snapshot_png` behind the P1 contract, and
deposit the result through the existing custody lane rather than a second store.

Done-conditions:

- A capture on a live Weld surface produces bytes deposited in the session's
  representation store, keyed by content hash.
- The deposit happens off the event-loop thread and answers with one app-owned
  update, matching the download custody actor.
- A capture requested on a node whose surface dies before the result arrives
  fails typed, leaves no partial artifact, and says so in observation.
- Two captures of the same unchanged page deposit one blob — content addressing
  proving itself.

### P3. The provenance record

The envelope, attached to the node: node identity, source address, engine,
viewport, page scale with its requested/applied marker, observed time, capture
scope, artifact kind, and content hash.

Done-conditions:

- Every field is populated from a real source at capture time; none is inferred
  later or defaulted silently.
- The record survives restart and names its artifact by content hash, which
  still resolves in the representation store.
- An engram export redacts or carries the record deliberately, decided the way
  the `web.*` runtime facets were (`pandect::graph_engram`), not by omission.
- Observation and the Inspector can show a node's captures without opening the
  bytes.

### P4. Retained capture (gated on D6)

Rasterize a Livery session's paint list to the same artifact kinds.

Done-conditions:

- Livery reports page-capture supported only once a real capture succeeds; the
  status flips with the implementation, never ahead of it.
- A retained capture at a non-default page scale records the applied scale and
  looks like what was on screen — the zoom receipts' positive control applies
  here too.
- Retained and hosted captures produce the same envelope shape, distinguishable
  only by the engine field.

### P5. Save and print over the artifact

There is no page save or print action today: `Action::SaveSession`
(`src/action.rs:231`) is session persistence, not page saving. So E0's
"save/print consumes the captured representation rather than a graph scene" is
not a rewire — it is introducing Save and Print on top of P2's artifact.

Done-conditions:

- Save writes the user-visible copy from the deposited artifact, never by
  re-capturing or re-fetching.
- Print uses the print-media path (`print_to_pdf`) and is offered separately
  from image capture, so neither pretends to be the other.
- Both are capability-gated per engine and simply absent where unsupported,
  matching how the zoom actions behave.
- A save of a capture taken at 125% produces the artifact that was captured, not
  a fresh one at the current scale.

### P6. Receipts

Headed acceptance in the scenario harness, following the `browser_zoom` pair.

Done-conditions:

- A capture receipt proves the full path on a real engine: request, deposit,
  envelope, restart, and resolution of the artifact by content hash afterwards.
- Each proof carries a positive control — an assertion that would fail if the
  capture were stale, empty, or from the wrong node.
- Unsupported engines prove absence: the actions are not offered, and the
  capability states the reason.

## Findings

- **2026-08-28, live tree:** `SurfaceProducer::capture_snapshot_png` is required
  and synchronous (inker `surface_engine.rs:888`), while welding's capture is
  async (`welding/src/surface.rs:775-790`). Turnstone answers it `Unsupported`
  (`src/shell/weld.rs:500`). The contract, not the tile, is the thing that is
  wrong.
- **2026-08-28, live tree:** `DocumentSession` has no capture method at all;
  retained capture is net-new, exactly as retained zoom was before E0.2.
- **2026-08-28, live tree:** the custody lane already exists and already does
  what capture needs — a serialized off-thread writer depositing into
  `representations.redb` through `muniment::BlobStore`, returning a
  `mere::kernel::graph::ContentHash` (`src/download.rs:211-240`,
  `src/action.rs:1004-1008`). Capture reuses it; it does not get a second store.
  The content-hash field of the envelope is therefore already provided.
- **2026-08-28, live tree:** welding separates `print_to_pdf` (print media,
  file-producing, callback-backed) from `print` (platform dialog) on purpose
  (`welding/src/surface.rs:744-772`), which is the same distinction this plan
  draws between artifact kinds one level up.
- **2026-08-28, live tree:** no page save or print action exists;
  `Action::SaveSession` is session persistence. P5 introduces rather than
  rewires.
- **2026-08-28, live tree:** `FetchedPage` retains exact response bytes and the
  decoded body (`src/action.rs:978-987`), so view source over the fetch lane is
  cheap. Hosted engines fetch internally and turnstone never holds those bytes,
  so hosted view source needs an engine seam or an honest unsupported status —
  the same split page capture itself has.

## Sidequests

Recorded here so they are not lost, and because each is independently closable:

- **View source**, riding along with P2/P3: retained lanes can serve it from
  held bytes; hosted lanes cannot without a seam (see Findings).
- **CEF authentication probe**: one probe of whether `GetAuthCredentials` fires
  for a proxy challenge. If it does not, keep the partial status and stop
  digging.
- **Zoom parity tail**: extend the typed page-zoom command to scripted, reader,
  and smolweb lanes, and settle the `page-zoom-applied` observation nuance — the
  field currently means "this node has been zoomed this session", not "this
  engine reads back".
- **F0 acceptance debt**: headed multi-match and zero-match find receipts, plus
  AccessKit Previous/Next/Close controls.

## Progress

- **2026-08-28:** plan written. Contract, artifact-kind, envelope, scope,
  custody, and retained-lane questions raised as decisions D1–D6 ahead of any
  implementation; no code written.
