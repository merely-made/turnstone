# Browser surfaces implementation plan

**Status, 2026-08-30:** in progress. T0 contributed document surfaces, K0
one-gesture Keep, F0 retained find, the D0 host decision surface, E0.1 engine
capability disclosure, and E0.2 per-node page zoom are landed. Captured-page
provenance P1's owner contracts and exact source pins are landed under the
ruled [page capture and provenance plan](2026-08-28_page_capture_plan.md); its
clean-source Turnstone compile receipt remains open. Live CEF authentication
callback delivery remains an independent E0 acceptance tail. This plan
succeeds the remaining-work portion of the historical
[browser gap analysis](2026-08-17_smolweb_browser_gap_analysis.md); the analysis
remains useful as research and acceptance evidence.

## Authority and order

Turnstone owns user-agent actions, durable graph truth, shell policy, and the
composition of surfaces. Genet engines own document behavior. Mere owns shared
graph and projection primitives. A browser feature crosses those boundaries
only through an existing port or a contract forced by a second consumer.

The dependency order is:

1. T0 contributed surfaces, closed.
2. K0 Keep, because later private-space promotion and curation depend on it.
3. F0 retained find and D0 decision UI. They may proceed independently.
4. E0 engine parity and capture, after the surface and decision contracts are
   explicit.
5. A0 arrivals and network inspection, consuming the same event and custody
   models.
6. S0 shallows, after Keep and persistence admission are real.
7. X0 extension completion, after grant decisions and contributed surfaces
   have proved their host boundaries.

## Phases

### T0. Contributed document surfaces

Landed in `1e9dde1` and `a37e6a1`. Turnstone now registers data-only surface
providers, admits provider-owned sessions behind a typed unavailable surface,
retains session identity by `PaneId`, routes input without provider access to
the app, and opens Knot documents through the shared pane path.

Done-conditions:

- Provider lookup and source-schema admission are explicit and unambiguous.
- Repinning evicts the old retained session.
- Focus, pointer, wheel, and key input reach the retained provider session.
- Knot is a real external consumer rather than a Turnstone-specific document
  model.

### K0. One-gesture Keep and the first regard state

Landed `Action::KeepNode { member }` as a captured, target-bound promotion. The
existing `keep` graph tag remains truth. `feed` stays a separate tag so keeping
a node does not subscribe it. Project the state through observation, the shared
action catalog, and the browser strip. The action is one-way and idempotent;
release and purge policy are not a hidden toggle.

Done-conditions:

- A focus change after intent creation cannot redirect Keep.
- A successful Keep writes graph truth, emits `node-kept`, requests a session
  save, and redraws.
- The strip offers **Keep**, then retains disabled **Kept** state.
- The contextual palette and automation catalog offer the same target-bound
  action and remove it after completion.
- A focused unit receipt and a headed scenario assert the observable state.

### F0. Retained find

Landed as a document-session capability returning an authoritative count,
current match, and, when the engine can disclose them, retained match records
with structural label or role and reveal target. The omnibar and Ctrl+F summon
one retained chrome field rather than a second command catalog. Hosted engines
and Livery documents implement the same host-facing capability without
Turnstone peeking into either DOM.

Done-conditions:

- Query changes update a retained match model rather than a one-shot probe hit.
- Next and previous wrap predictably and reveal the selected match.
- The match count and current structural label are observable and accessible.
- At least one Livery document and one hosted engine pass the same consumer
  tests.

Accepted 2026-08-26. Focused model and adapter tests pass, and both
`scenarios/browser_find.scn` and
`scenarios/browser_find_weld_windows.scn` finish `RESULT ok` with visible
selection plus the same search-input and status accessibility projection.

Nonblocking F0 acceptance debt stays explicit rather than reopening the lane:

- add headed multi-match wrap and zero-match cases; current headed receipts use
  one match, while lower-level Livery and Weld tests cover the richer states;
- check the headed captures into a durable receipt directory;
- project Previous, Next, and Close into the AccessKit find subtree; keyboard
  activation and the retained visible buttons already work;
- extend the host-edited find field with clipboard paste, Delete, and caret
  movement when text-editing parity becomes the active slice.

### D0. Permission and authentication decisions

Turn the existing request events into visible, origin-scoped decisions. Web
policy and participant-grant authority remain distinct even if one Cambium
component renders both.

Done-conditions:

- Pending requests survive redraw and cannot attach to a later navigation.
- Allow once, standing allow, and deny lower to the exact request identity.
- Sensitive credential material never enters observation, logs, or graph
  truth.
- Restart tests prove which decisions persist and which deliberately do not.

Accepted 2026-08-26 at the host boundary. Retained permission and
authentication requests, exact `{ node, request }` lowering, navigation and
surface withdrawal, keyboard/pointer interaction, accessible controls,
credential redaction, standing permission persistence, and process-only
  credential memory pass focused tests. The real Windows Weld permission
scenario finishes `RESULT ok`, visibly presents the origin-scoped card, answers
the exact callback, and observes the page continue. CEF 151 still does not emit
`GetAuthCredentials` for the fixture's top-level Basic challenge, so this plan
does not claim a live authentication receipt; that engine-delivery gap moves to
E0. The broader library run passes 371 tests and ignores 5; its sole failure is
the pre-existing
`place::lanes::tests::a_partition_heals_and_both_sides_converge` delivery-after-heal
gate, outside D0.

### E0. Engine parity, page zoom, and capture

Complete the user-agent contract across registered engines. Page zoom is a
document scale and remains separate from UI zoom and canvas camera zoom. Page
capture freezes what the person saw; `FrozenScene` continues to mean a graph
projection and is not renamed into a page snapshot.

Done-conditions:

- Each registered engine reports support or a typed unavailable result for
  find, page zoom, capture, and navigation controls.
- Each claimed permission or authentication callback passes a live consumer
  receipt; partial and unavailable statuses name the missing backend seam.
- The requested page-zoom scale always persists per node; engine policy
  determines only quantization, bounds, and the effective applied value.
  Chrome scale never changes.
- A captured page representation retains source identity and observed-state
  provenance.
- Save/print consumes the captured representation rather than a graph scene.

E0.1 accepted 2026-08-26. Retained `DocumentSession` and hosted `WebSurface`
implementations report the same typed find, page-zoom, page-capture, and
navigation statuses. Turnstone mirrors the exact unsupported reasons and
partial-support details into the Apparatus, Inspector, observation snapshot,
and probe fields. Find is offered only when the owning engine reports supported
or partial service. This disclosure does not pretend that a method-shaped seam
is a working page control.

E0.2 begins with per-node page zoom. Capture follows once the hosted async
request/result path and retained observed-state provenance are explicit. The
CEF authentication callback remains an independent engine-delivery receipt.

E0.2 zoom accepted 2026-08-27. The `browser_zoom` scenario pair (plus the
restart pair and `fixtures/browser_zoom_server.ps1`) proves the
100→125→100 ladder, media-query reflow, transformed hit testing, find
reveal, restart replay into a freshly spawned engine, and unchanged
chrome/canvas scale on both retained Livery and hosted Weld. Livery reports
the applied value through the typed `set_page_zoom`; Weld stays Partial
because Windows CEF cannot read the effective level back. Capture and the CEF
authentication receipt remain open. Capture is in progress under the
[page capture and provenance plan](2026-08-28_page_capture_plan.md); its D1-D6
decisions were ruled 2026-08-30 before P1 began. P1 source is landed but awaits
its clean-source Turnstone compile receipt before P2 starts. Capture closes
E0's capture work, while the independent authentication probe remains before E0
as a whole is marked landed.

### A0. Arrivals, custody, and network inspection

Compose downloads, feeds, shared-place arrivals, and request diagnostics as
views over existing records. Do not create another history database. Network
inspection reads redacted request events and custody records rather than actor
internals.

Done-conditions:

- An Arrivals projection can distinguish unread feed entries, completed
  downloads, and shared-place additions by provenance.
- Opening an arrival selects or opens its durable graph target.
- Network rows preserve request identity, phase, timing, and public routing
  metadata while redacting secrets and sensitive Gemini queries.
- The pane stays correct across session switches and restart.

### S0. The shallows

Implement the shallows as a persistence-admission boundary. Appearance and
space placement may reveal that boundary but do not define it. Nodes and
representations remain ephemeral until the exact member is promoted through
Keep with a provenance decision.

Done-conditions:

- Restart proves unkept shallows content never entered durable session stores.
- Keep atomically promotes graph truth and the representations selected by
  policy, or leaves both ephemeral on failure.
- Any lens can project the same shallows state without turning it into a
  window-private mode.
- Private-state indicators are accessible and do not rely on color alone.

### X0. Extension completion

Finish the three trust lanes already named by the product: host command packs,
portable `DocumentScript`, and registered loaders/providers. Reuse D0 for
grant decisions and T0 for contributed UI. Do not grant a document surface an
app, renderer, device, or filesystem handle by convenience.

Done-conditions:

- Install, review, revoke, and restart receipts exist for each supported lane.
- `DocumentScript` has a real Turnstone consumer with bounded inputs and
  outputs.
- Provider registration failures are typed and visible.
- Revocation removes future authority without invalidating retained evidence.

## Findings

- **2026-08-25, live tree:** `App::available_actions` is already the common
  catalog for the omnibar, observation, automation, and contextual panes
  (`src/app/palette.rs`, `src/shell/drive.rs`). New commands must join there.
- **2026-08-25, live tree:** `keep` already means durable node curation and is
  also applied to feed sources; subscription is independently represented by
  `feed` (`src/feed.rs`, `src/app/feed_arms.rs`). Ordinary Keep must not imply a
  subscription.
- **2026-08-25, live tree:** UI zoom is live in the retained chrome appearance,
  while canvas zoom is a camera gesture. Neither is page zoom
  (`src/chrome_view.rs`, `src/main.rs`).
- **2026-08-25, live tree:** the permission and authentication contract carries
  exact identities in `AppEvent`; D0 needed retained decision state and exact
  lowering rather than another request vocabulary (`src/observe.rs`). Backend
  callback delivery remains a per-engine capability.
- **2026-08-25, live tree:** the probe path supplies a point text target, not a
  retained, navigable match set (`src/shell/drive.rs`). The gap analysis's
  reference to the Inspector as an existing find index overstates the seam.
- **2026-08-25, live tree:** `FrozenScene` freezes a disclosed graph scene
  (`src/frozen_projection_pane.rs`, `src/a11y.rs`). It cannot serve as page
  capture without collapsing two different artifacts.
- **2026-08-25, live tree:** contributed-surface admission and Knot consumption
  landed in `1e9dde1` and `a37e6a1`. That seam is available to D0, A0, and X0.
- **2026-08-25, consumer build:** advancing the stale Mere and Genet git
  packages exposed an uninherited Parley patch before Turnstone compiled.
  Genet now exposes its fork as a workspace package in `94a4dc4155b`, and
  Turnstone restates that root patch just as it already does for Taffy.
- **2026-08-25, clean consumer:** Genet intentionally carries a 0.0.1
  `layout-dom-api` name-claim package beside the live 0.1.0 package. A Git patch
  without a version was ambiguous outside the path-overridden checkout;
  Turnstone now selects exactly 0.1.0 in both the direct dependency and patch.
- **2026-08-25, clean consumer:** the first focused Keep receipt passed with
  pushed Mere `11d996a6`, Genet `94a4dc4155b`, and Netrender `6f1a4fe70`. It
  exposed parallel registry and Git islands for
  `genet-livery`/`buckram`/`livery`, plus Fleece 0.2 and 0.4. Genet's twelve
  already-published support commits are now merged on main at `a7d410c1ca8`,
  aligning the Livery family versions; Turnstone can therefore patch the
  published `genet-livery` edge to that same Git source without falsifying a
  version contract. Fleece was still a separate owner migration at that gate;
  the 0.4 follow-through below closes it.
- **2026-08-25, consumer build:** Netrender's published 0.1.2 release commits
  remained on a cleanable release worktree while git `main` declared 0.1.1.
  That split gave Turnstone source-distinct `Scene` types. The release history
  and regenerated lock are now on Netrender main at `6f1a4fe70`; the stale
  worktree and branch are pruned, and Turnstone patches the renderer family as
  one source.
- **2026-08-26, retained find:** Inker now owns the engine-neutral query,
  direction, reveal, match, and state vocabulary. The count is authoritative
  and match records may be sparse, which lets Livery disclose structural
  matches while a hosted engine reports only count and active ordinal
  (`src/document_find.rs`, `src/app/document_find_arms.rs`).
- **2026-08-26, hosted find:** CEF's fourth find argument is named `findNext`,
  but Chromium consumes it as `find_match`. Passing false returned a correct
  count with no active selection. The Weld adapter now always requests an
  active, revealed match while query changes still begin a new engine session
  (`src/shell/weld.rs`).
- **2026-08-26, headed D0:** Weld/CEF 151 delivers and accepts a held
  geolocation callback. Its `GetAuthCredentials` handler remains uncalled for a
  top-level Basic challenge, matching Welding's partial capability declaration.
  Turnstone therefore reports permissions as supported and authentication as
  partial instead of promoting contract wiring into a false live capability.
- **2026-08-26, E0.1 source alignment:** advancing Turnstone to Genet's shared
  capability contract while Mere still pinned the prior Genet revision created
  two source-distinct Inker contracts. Mere `caaa6dd2cdaf` aligns its Genet
  family to `633c7afc8a5a`; a clean Turnstone lock now resolves one Genet source
  and one Mere source. The ignored local Cargo override also names the Taffy
  fork explicitly as `genet-taffy`, so local development matches the published
  package identity instead of masking stale lock state.
- **2026-08-30, capture P1 source alignment:** Genet `da8762fd910` owns the new
  correlated contract, Mere `9667541261` aligns its 35 Genet pins, and
  Turnstone pins both plus Welding `844f949a9f` exactly. Turnstone's ignored
  local lock was produced under sibling-path redirects and cannot attest those
  published identities. Two bounded clean-source resolver runs wrote no fresh
  lock, so the P1 consumer compile remains open rather than being called green.

## Pitfalls and contradictions

- The historical gap map describes the shallows as a space property, but its
  decisive behavior is store admission. Styling a space without changing the
  persistence boundary would provide false privacy.
- "Keep" previously mixed general curation with feed-source retention. The
  shared `keep` tag is sound; feed scheduling must continue to require its own
  `feed` state.
- Find cannot be inferred from the Inspector's structural reading or the
  probe's first text target. Those are useful prior art, not the retained
  navigation contract.
- Save/print cannot reuse graph `FrozenScene` by name. Graph projection and
  observed page capture have different sources, replay properties, and privacy
  consequences.
- Web permissions and denizen or participant grants may share visual grammar,
  but their authorities and revocation rules must remain separate.
- A typed request event and answer method do not prove that an engine emits the
  callback. Capability status requires a live server or proxy receipt; CEF
  server authentication remains partial until one exists.
- A hosted surface must answer capabilities through its registered session.
  Turnstone DOM inspection would break engine modularity and the shared-device
  boundary.
- Root patches may collapse source identity only after package versions and APIs
  align. That precondition is now met for Livery, Fleece 0.4, and
  `genet-taffy 0.13.1`. Future bumps still require the same owner-first proof;
  a top-level patch is not evidence by itself.

## Synergies and sidequests

- K0 gives feeds, bookmarks, the regard, and shallows promotion one durable
  predicate without giving them one policy.
- T0's unavailable surface and retained-session machinery can host permission,
  arrivals, and extension views without adding pane-specific shell branches.
- F0's match model can serve accessibility and automation directly, avoiding a
  separate test-only search API.
- D0's decision component can share layout and interaction code with denizen
  grant review while keeping different typed actions beneath it.
- A0 can fold Steward custody, feed unread state, and place provenance into one
  arrivals view because all three already end in durable records.
- Network inspection should first be a redacted event projection. Export,
  HAR-like serialization, and performance analysis are sidequests until a real
  consumer forces them.
- Inline audio remains deferred until a host-owned playback seam and another
  document consumer force a reusable contract.
- The Fleece and Taffy dependency sidequests are closed. Fleece 0.4 now owns the
  extraction boundary directly in Mere's Gazette, Import, Crawl, and Knot
  consumers; `genet-extract` is retired. Genet's fork is published as
  `genet-taffy 0.13.1`, and consumers pin the matching immutable Genet source.
  Keep this owner-first pattern for later dependency moves.

## Progress

- **2026-08-25:** reconciled the historical gap analysis against the live tree
  and sequenced the remaining work into dependent lanes.
- **2026-08-25:** accepted T0 as landed evidence from `1e9dde1` and `a37e6a1`;
  no duplicate implementation was started.
- **2026-08-25:** landed K0 with target-bound action, graph-tag truth, shared
  catalog and observation projection, and retained **Keep** / **Kept** strip
  state. Focus-drift and idempotence unit receipts pass; the offline
  `scenarios/browser_keep.scn` headed receipt is `RESULT ok` and its capture
  visibly retains the disabled **Kept** state.
- **2026-08-25:** repaired the published-source Genet-Livery consumer boundary
  by landing Genet `94a4dc4155b` and adding Turnstone's explicit Parley patch.
- **2026-08-25:** fast-forwarded Netrender's published 0.1.2 stack onto main,
  pushed `6f1a4fe70`, pruned its release worktree/branch, and unified
  Turnstone's renderer source family.
- **2026-08-25:** the external published-source gate resolved and compiled from
  outside Turnstone's checkout, then passed
  `keep_action_is_target_bound_idempotent_and_observable`. It also exposed and
  recorded the remaining non-blocking Livery/Buckram and Fleece version islands.
- **2026-08-25:** merged Genet's twelve-commit published support release into
  main, pushed `a7d410c1ca8`, and pruned the release, reconciliation, recovery,
  and proof worktrees/refs. Turnstone now unifies the aligned Livery family at
  the root; only the incompatible Fleece and not-yet-republished Taffy lanes
  remain explicit dependency sidequests.
- **2026-08-25:** refreshed the external consumer to Genet `a7d410c1ca8`,
  removed the registry copies of `genet-livery`, Buckram, Livery, Host API,
  Document Resources, and `genet-taffy`, then recompiled and passed the focused
  Keep test. The final duplicate ledger names only the deliberate Fleece 0.2 /
  0.4 migration and unrelated Taffy API generations.
- **2026-08-26:** landed the shared retained-find contract in Genet
  `42941fe18bfa`, including Livery matching and reveal, Weld forwarding, and
  focused Inker, Livery, and adapter receipts.
- **2026-08-26:** completed Turnstone F0 with retained query and request
  correlation, stale-answer rejection, Ctrl+F and palette entry, next/previous
  wrap, controlled chrome, accessibility, observation, Livery integration, and
  the Windows Weld adapter. The published-source focused Weld test and binary
  build pass with local redirects disabled. Both headed engine scenarios are
  `RESULT ok` and their captures visibly select `documentation` with `1 of 1`.
  The broader library run passed 345 tests and ignored 5, but remains red on
  the pre-existing `place::lanes::tests::a_partition_heals_and_both_sides_converge`
  delivery-after-heal failure; that p2p gate is independent of F0.
- **2026-08-26:** completed D0's host-owned decision surface with retained
  exact request identity, durable permission versus process-only credential
  policy, controlled Cambium chrome, keyboard/pointer routing, accessibility,
  sanitized observation, and stale-request withdrawal. Focused model, policy,
  chrome, and scenario-parser tests pass. The real Windows Weld permission
  scenario and local server both finish `RESULT ok`; live server-auth delivery
  remains truthfully partial and is assigned to E0.
- **2026-08-26:** closed the dependency follow-through. Genet `138b6aca6e27`
  retires `genet-extract`, carries the corrected Livery text-fragment path, and
  consumes the published `genet-taffy 0.13.1` release (checksum
  `d53b4825b55d3d5103cec7f3fee5eadfadd21792e826a896ab3d0e3190124c22`).
  Mere `d7cd4a87f782` centralizes immutable Genet and Netrender sources, migrates
  its Fleece consumers to 0.4, and pins Vello tag `vello-0.10.0` plus the
  `mere-p2panda-net-0.7.2` source tag. Its clean locked focused gate passed 81
  tests, including the four Fleece consumers and Distillery.
- **2026-08-26:** accepted E0.1. Genet `633c7afc8a5a` adds typed find, page-zoom,
  page-capture, and navigation status to every retained and hosted document
  adapter; its Inker, Livery, Smolweb, Graft, Scrying, and Weld gates pass. Mere
  `caaa6dd2cdaf` realigns its immutable Genet pins. With local redirects removed,
  Turnstone's clean locked capability tests pass 3/3, its find-command tests
  pass 4/4, and `cargo check --locked --lib --features weld -j 1` passes through
  native CEF and the Turnstone Weld adapter. The Apparatus, Inspector,
  observation snapshot, probe fields, and command catalog now tell the same
  engine-specific truth.
- **2026-08-30:** landed capture P1's owner-side source: Genet `da8762fd910`,
  Welding `844f949a9f`, and Mere alignment `9667541261`, with their focused
  owner tests and metadata checks recorded in the capture plan. Turnstone
  `4a6ee8d`, `5b80580`, and `6b1870c` adopt and pin those seams while leaving
  capture honestly unsupported. The clean-source Turnstone compile
  done-condition remains open because bounded Cargo 1.96 and 1.97 resolution
  attempts did not produce a refreshed lock; P2 has not begun.
