# Page capture and provenance

**Status:** in progress, 2026-08-30. D1-D6 were ruled before P1 began. P1's
owner contracts and exact source pins are landed; its clean-source Turnstone
compile gate remains open on the resolver receipt recorded below, so P2 has not
begun. Serves the capture half of E0 in the
[browser surfaces implementation plan](2026-08-25_browser_surface_implementation_plan.md),
which keeps E0's done-conditions. E0.2's first half, per-node page zoom, was
accepted 2026-08-27; this plan is its successor slice and closes E0's capture
work when it lands. E0 itself also retains the independent CEF authentication
acceptance tail.

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

Mark ruled these on 2026-08-30. Each names what the codebase already forces, so
the decision is an implementation boundary rather than a taste.

### D1. The pre-P1 capture contract was synchronous and mandatory

`SurfaceProducer::capture_snapshot_png(&mut self) -> Result<Vec<u8>, SurfaceError>`
(pre-P1 genet `components/inker/src/surface_engine.rs`) was **required**: it had
no default body, so every producer had to write it. It was also **synchronous**,
so it had to return pixels before returning at all.

No CEF-backed engine could honour that. Welding's capture is a two-step async
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

**Ruling: replace, with transport-native completion.** Inker owns one request id
and common request/result vocabulary. Hosted pages start capture through
`WebSurface` and complete through its ordered event stream. Retained
`DocumentSession` implementations return the same typed result directly until a
real retained event stream exists; symmetry alone does not justify inventing
one. The old synchronous `SurfaceProducer::capture_snapshot_png` is deleted
rather than deprecated (DOC_POLICY §3: no parallel obsolete system).

The host mints a **capture request id**, not a receipt id: a failed request has
no durable receipt. The first hosted slice permits one outstanding capture per
surface and returns a typed busy result for a second. At decision time Welding's
helper dropped its internal request identity before polling; `844f949a9f`
subsequently made that identity durable through completion. The Turnstone
adapter still enforces single-flight before it can claim hosted capture.

### D2. Screenshot and print-PDF are different artifacts

Mark's correction, and welding states it in its own contract: `print_to_pdf`
"prints what the page looks like to a printer, not a screenshot — a page that
styles itself for `@media print` gets that" (`welding/src/surface.rs:748-752`).
Welding also keeps PDF export and the system print dialog deliberately separate,
because one produces a file and the other hands control to a platform dialog.

So: **distinct artifact kinds sharing one provenance envelope.** Proposed kinds
are page image (PNG, screen media, screen scale), page document (PDF, print
media), and page source (exact response bytes — see the view-source sidequest).
**Ruling: one outer envelope with typed kind-specific details.** Shared identity,
source, engine, time and content-hash facts stay uniform; image geometry, PDF
print facts and source-response facts do not. P1 admits only the page-image
variant. PDF export adds its own variant when implemented. Exact response bytes
remain the separate view-source sidequest rather than being called a rendered
page capture.

### D3. Which page scale the envelope records

E0.2 made page scale a real per-node value with two halves: the **requested**
scale (persisted, `web.page_scale`) and the **applied** scale (engine-clamped,
transient, known only for engines that report it). An artifact shows the applied
one.

**Ruling: retain both facts.** Record the requested scale unconditionally and
the engine-reported applied scale separately as optional. Never relabel the
requested value as applied when readback is unavailable. A Livery capture can
therefore prove a request of 12.0 was applied as 5.0, while Weld truthfully
records the request and an unknown applied value.

### D4. Capture scope

Viewport-only or full-page. Chromium can capture beyond the viewport, but
welding does not expose that today, so full-page is engine work, not turnstone
work. **Ruling: viewport in this slice**, with `capture_scope` in the envelope
from the start. The viewport record includes CSS scroll origin, CSS extent,
output pixel dimensions and named coordinate spaces. An engine may report an
unknown origin or extent, but that remains explicit and keeps its capability
Partial rather than fabricating geometry. A later full-page capture becomes a
new scope value rather than a schema change.

### D5. Whether a capture writes a user-visible file

Downloads always do both: exact bytes into the session's Muniment blob store,
plus a convenience copy in the download directory (`src/download.rs:211-240`).
A capture is not a download — nobody asked for a file in Downloads — but a
capture nobody can find is not much use either.

**Ruling: deposit always after an explicit capture is admitted; copy only on
explicit save.** The blob deposit makes it durable and node-attached; the file
copy becomes the Save action of P5. The existing download helper always performs
both operations, so P2 extracts a general representation-deposit command rather
than routing capture through `download::store`. Session, node and document
generation are captured when the request starts so a switch or navigation
cannot redirect a later completion.

### D6. Retained capture in this slice, or honestly unsupported

Before P1, `DocumentSession` had **no** capture method. P1 now gives it a
defaulted immediate method using the shared result vocabulary, while every
retained lane still reports `page_capture` unsupported (genet
`components/genet-documents/src/engines.rs`). Retained implementation remains
net-new the same way retained zoom was.

Livery can in principle be captured: it emits a paint list that netrender
already rasterizes to produce the frame on screen. The question is whether this
slice pays for a rasterize-to-PNG path or ships hosted capture first.

**Ruling: contract in P1, retained implementation deferred beyond the hosted
landing.** Livery keeps its honest unsupported status until a rasterize-to-PNG
path exists. P4 remains an explicit later lane and does not block the hosted
capture receipt; E0's capability rule already permits a typed unavailable
result. E0.2 proved a contract can land ahead of a lane implementing it.

## Phases

### P1. The capture contract (source landed; consumer gate open)

Put the shared typed request/result vocabulary in Inker. `WebSurface` gets a
defaulted request command and a correlated completion event;
`DocumentSession` gets a defaulted immediate capture method using the same
types. The result carries the page-image bytes plus the facts only the engine
knows (applied scale, viewport geometry, scope and output dimensions). Retire
the mandatory sync `SurfaceProducer` method.

Done-conditions:

- Every registered engine reports a typed supported, partial, or unsupported
  page-capture status, and no engine is forced to implement a method it cannot
  honour.
- The hosted completion identifies which request it answers on both success and
  failure. Lower transports preserve their own admitted request identities and
  do not evict completions before the adapter polls them.
- Turnstone compiles against the new contract with capture still unimplemented,
  proving the contract change is separable from the product work.

### P2. Hosted capture through Weld, into custody

Drive `request_snapshot_png` / `poll_snapshot_png` behind the P1 contract, and
deposit the result through the existing custody lane rather than a second store.

Done-conditions:

- A capture on a live Weld surface produces bytes deposited in the session's
  representation store, keyed by content hash.
- The first Turnstone adapter is single-flight and returns typed busy for a
  second request while one is outstanding, so host and Welding identities
  cannot be paired with the wrong provenance record.
- The deposit happens off the event-loop thread and answers with one app-owned
  update, matching the download custody actor.
- A capture requested on a node whose surface dies before the result arrives
  fails typed, leaves no partial artifact, and says so in observation.
- Navigation, node closure and session switch either retain the exact captured
  target generation or reject the stale completion; none can attach it to the
  newly current page.
- Two captures of the same unchanged page deposit one blob — content addressing
  proving itself.

### P3. The provenance record

The envelope, attached to the node: node identity, source address, engine,
viewport rectangle and output pixels, requested page scale, optional
engine-reported applied scale, request/completion observation interval, capture
scope, typed artifact details, and content hash.

Done-conditions:

- Every field is populated from a real source at capture time; none is inferred
  later or defaulted silently.
- The record survives restart and names its artifact by content hash, which
  still resolves in the representation store.
- An engram export redacts or carries the record deliberately, decided the way
  the `web.*` runtime facets were (`pandect::graph_engram`), not by omission.
- Observation and the Inspector can show a node's captures without opening the
  bytes.

### P4. Retained capture (deferred by D6)

Rasterize a Livery session's paint list to the same artifact kinds.

Done-conditions:

- Livery reports page-capture supported only once a real capture succeeds; the
  status flips with the implementation, never ahead of it.
- A retained capture at a non-default page scale records the applied scale and
  looks like what was on screen — the zoom receipts' positive control applies
  here too.
- Retained and hosted captures produce the same envelope shape, distinguishable
  only by the engine field.

### P5. Save and PDF export

There is no page save or print action today: `Action::SaveSession`
(`src/action.rs:231`) is session persistence, not page saving. So E0's
"save/print consumes the captured representation rather than a graph scene" is
not a rewire. This slice introduces Save over P2's artifact and a distinct PDF
export artifact. Actual printing remains deferred until Turnstone owns a
platform path that prints a deposited artifact; Welding's `print_to_pdf`
produces an artifact and does not consume one.

Done-conditions:

- Save writes the user-visible copy from the deposited artifact, never by
  re-capturing or re-fetching.
- PDF export uses the print-media path (`print_to_pdf`) and is offered separately
  from image capture, so neither pretends to be the other.
- Save and PDF export are capability-gated per engine and simply absent where
  unsupported, matching how the zoom actions behave.
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

- **2026-08-28, pre-P1 tree:** `SurfaceProducer::capture_snapshot_png` was
  required and synchronous, while Welding capture was asynchronous. Turnstone
  answered the mandatory method `Unsupported`. P1 retired that seam in Genet
  `da8762fd910`; hosted capture now uses a request plus correlated completion,
  and retained sessions use a defaulted immediate method.
- **2026-08-28, pre-P1 tree:** `DocumentSession` had no capture method at all.
  Genet `da8762fd910` adds the defaulted contract without claiming a retained
  implementation; retained capture remains net-new.
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
- **2026-08-30, pre-P1 Welding tree:** `SnapshotChannel` retained internal CDP
  ids while requests waited, but its public poll result dropped the id and its
  bounded result queue evicted the oldest completion at sixteen. Welding
  `844f949a9f` now returns a typed request id with every success or error, counts
  pending plus unpolled results against one sixteen-slot admission bound, and
  rejects overflow without evicting an admitted completion. This is
  source/compile evidence; a new live cross-platform screenshot receipt remains
  later work.
- **2026-08-30, live tree:** `download::store` always deposits and creates a
  user-visible file (`src/download.rs:211-239`). Capture reuses the Muniment
  store and serialized actor pattern, not that combined operation.
- **2026-08-30, consumer resolution:** Turnstone's ignored local `Cargo.lock`
  was produced under `.cargo/config.toml` sibling-path redirects, so it cannot
  attest the exact published Genet, Mere and Welding identities. Clean-source
  resolution must run from outside the checkout with `--manifest-path` before a
  locked compile can count as the P1 consumer receipt.

## Sidequests

Recorded here so they are not lost, and because each is independently closable:

- **View source**, after P3 rather than inside P1: retained lanes can serve it
  from held bytes; hosted lanes cannot without a seam (see Findings).
- **CEF authentication acceptance tail**: one probe of whether
  `GetAuthCredentials` fires for a proxy challenge. If it does not, keep the
  partial status and stop digging. This remains required before E0 as a whole is
  marked landed, although it is independent of capture implementation.
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
- **2026-08-30:** D1-D6 ruled. P1 began with transport-native hosted versus
  retained completion, request rather than receipt identity, dual scale facts,
  exact viewport geometry, deposit-only custody, and retained capture deferred.
  Save and PDF export are separated from actual printing; the CEF auth probe is
  retained as an E0 acceptance tail.
- **2026-08-30:** P1 source implementation landed. Genet `da8762fd910` owns the
  shared viewport request/result vocabulary, correlated hosted completion,
  defaulted retained method and typed busy error; its Inker tests pass 106/106,
  Graft/Scrying/Weld tests pass 21/21, Pelt routing passes 3/3, and the Pelt
  desktop library checks. Welding `844f949a9f` retains request identity through
  completion and rejects a seventeenth admitted capture without eviction; its
  CEF-featured Welding tests pass 52/52 plus two compile doctests, and the
  Windows demo checks. Mere `9667541261` aligns all 35 Genet pins; root,
  Distillery and Graphshell-web `--no-deps` metadata parse offline, while Knot
  desktop's workspace-membership error remains outside this slice.
- **2026-08-30:** Turnstone commits `4a6ee8d`, `5b80580` and `6b1870c` retire
  its obsolete sync stub and pin Genet `da8762fd910`, Welding `844f949a9f` and
  Mere `9667541261` exactly. The clean-source compile receipt is still open:
  bounded Cargo 1.96 and 1.97 resolver runs each crossed roughly 418 CPU seconds
  and 1.5 GB without writing a refreshed ignored lock, so they were stopped.
  No Turnstone compile is claimed, and P2 waits on this final P1 done-condition.
- **2026-08-30:** Turnstone's workspace toolchain now pins Rust 1.97.1, matching
  Mere and Genet; the prior workspace-version mismatch is retired. This does not
  change the clean-source resolver/compile boundary above: no Turnstone compile
  is claimed, and P2 remains blocked on that P1 done-condition.
