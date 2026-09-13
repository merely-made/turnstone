# Reticulum browsing — NomadNet nodes and capsules, idiomatically

**Date:** 2026-08-03
**Status (2026-09-13):** native NomadNet page fetching and shared Micron preview
are implemented; full presentation and interaction remain scoped below. Doctrine,
stated by Mark for this lane and
matching the smolweb one: be honest about the content, optionally enrich the
presentation, and never collapse anything into formats that are not idiomatic
to the protocol.

**Relation to the peer-web reframe:** [P5](2026-07-28_turnstone_peer_web_reframe.md)
governs protocol/carrier expansion with a one-adapter-at-a-time rule and
headed receipts. This plan scopes the browsing lane so it is ready to enter
that queue; it does not jump it. Retinue-as-place-carrier and LXMF-in-Comms
are P5's other rows, not this plan.

## What exists (verified 2026-08-03)

- **retinue** (the Reticulum family workspace, MPL-2.0): links, announces,
  resources, destinations, all RF-proven on real hardware.
- **`gemini_over_reticulum` example** (retinue/crates/retinue/examples):
  a gemtext capsule served and fetched over an encrypted link with named
  announce-based addressing (`gemini://capsule/` resolved by
  recompute-and-match against announces, the NomadNet addressing model) and
  faithful end-of-response semantics (`LinkStream` shutdown maps to link
  close, the reader sees clean EOF). The bytes are exactly what
  `errand::gemini_exchange` writes and reads.
- **The gemtext lane end to end**: errand's gemini parse and the
  cambium-nematic views render gemtext today. For gemini-over-Reticulum,
  parse and render are already built; only the carrier differs.
- At the August 3 inventory there was no Micron parser, view or node-page
  client. That gap is now partially closed: `src/nomadnet.rs` registers a
  retained session over Nematic's source-preserving Micron engine;
  `src/shell/nomadnet_fetch.rs` drives native page fetches through Retinue.
  The [current receipt and limitations](2026-08-17_smolweb_browser_gap_analysis.md#micron-syntax-and-interoperability-2026-09-13)
  supersede the original inventory for this lane.

## The two content families

### 1. Gemini over Reticulum (near-free)

Same gemtext, different carrier. The transport swap is retinue's LinkStream
in place of TLS/TCP; everything above the bytes is the existing lane.

The one real design item is **trust**: there is no TLS and no TOFU pin.
The destination identity is the authority; a named capsule is whichever
announcer's identity yields that destination. That maps into the fidelity
plan's trust descriptor as its own posture (identity-addressed, stronger
than TOFU in one way, name-squattable in another: first-announce wins a
name the same way first-connect wins a pin). Surface it honestly in the
tile chrome; never borrow the TLS vocabulary.

### 2. NomadNet nodes (the real work)

NomadNet nodes serve **micron** pages (`/page/index.mu`) and files over
Reticulum request handlers, bodies delivered as Resources (windowed,
compressed, explicit COMPLETE; retinue already implements Resources at the
protocol level). Micron is its own markup idiom: sections and depth,
formatting and color codes, links to node pages and files, and input
fields that submit values back to the node.

The target is to render Micron idiomatically: preserve its sectioning,
alignment, colors and submit-to-node semantics. Currently the syntax tree
retains these facts, but the portable preview drops some styling and leaves
inputs inert. Recover those facts through shared presentation without rewriting
the author's source. Reader theming, focus and history remain host choices.

**Clean-room note:** Nomad Network is GPL. The micron implementation is
written from the version-pinned in-app Guide and controlled black-box outputs,
not NomadNet implementation source. The current fixture set identifies
NomadNet 1.4.2 and distinguishes documented syntax from candidate probes.

## Homes (reconciled with current code, 2026-09-13)

The original parser-in-Retinue proposal has been superseded by the
implemented shared engine. The format model remains independent of fetching;
moving it to another workspace is not a prerequisite for completing the view.
The general protocol ownership rule is recorded in
`smolweb/design_docs/technical_architecture/2026-08-03_smolweb_home_decision.md`.

- **Retinue**: node requests, links, Resources and static page serving in
  `crates/retinue/src/nomadnet.rs`. The current page API returns bytes; typed
  form request values remain open in `request.rs`.
- **Mere / Nematic**: source-preserving syntax and lowering in
  `crates/nematic/nematic/src/micron/`. Inker/document-canvas own reusable
  presentation and document-lanes own the retained viewport. Knot and
  Turnstone share these implementations.
- **turnstone**: the lane wiring, through the
  [engine adoption plan](2026-08-03_turnstone_engine_adoption_plan.md)'s
  seams: a session engine whose fetch runs over retinue, appearing in the
  picker like any lane, with E4's no-handler fallback covering the schemes
  until it lands.

## Steps

### N0. Addressing (native form implemented)

`destinationhex:/absolute/path` is the native form accepted by
`nomadnet::parse_address`; it is not a private URL scheme. Same-node links
resolve against the fetched destination. Ordinary files gain no implicit node
authority. Name aliases and bare/relative target behavior remain separately
qualified; they must not replace the durable destination identity.

### N1. Gemini over Reticulum, end to end

Promote the example into a Turnstone lane: errand-shaped fetch over
LinkStream behind the session-engine fetch seam, existing parse and views,
trust posture surfaced. Headed receipt: a capsule served from a second
process (or second machine) renders in a Turnstone tile.

### N2. Micron syntax (shared model implemented; qualification remains)

Nematic retains sections, styles, links, controls and directives from the
stock Guide and original captured fixtures. Remaining malformed-input,
escape and section-exit cases require further independent observations.

### N3. Node browsing client (retinue workspace)

Static page requests and Resource delivery are implemented. Turnstone retains
the destination context and requests path discovery before opening the link.
File transfer, named discovery UI, durable client identity and typed form
requests are distinct further gates; the current bytes API does not establish
that full contract.

### N4. Micron presentation and interaction (reading implementation; qualification remains)

The canonical completion scope is
`mere/design_docs/nematic_docs/implementation_strategy/2026-07-01_smolweb_fidelity_plan.md`,
section "Micron completion scope (2026-09-13)". Reading fidelity and horizontal
table navigation come first, then anchor/fold behavior; typed request evidence
gates forms, and partials/media/directives each need their own lifecycle proof.
The shared reading implementation retains inline color/underline and block
alignment/indent through Inker and document-canvas, with horizontal viewport
navigation. Turnstone also descends through these retained wrappers when
refusing unresolved aliases, so presentation cannot hide an unresolved target.
Forms and further interactive behavior remain separate.

The 2026-09-13 consumer check uses Mere `dce5cc97`, Knot `cf3afe8`, Genet
`101d9e9` and Netrender `3961aca`, with one immutable Cargo source identity
each. With Rust 1.97.1, `--locked --offline` and an absolute manifest outside
the development override tree, all five NomadNet tests and both smolweb input
tests pass. This includes nested presentation-wrapper alias refusal, preserved
link labels, native destination/path identity and retained scene production.
It is a focused automated receipt, not a full-library or headed acceptance run.

Turnstone's done-condition is a headed native session using that shared behavior,
with selection/focus, correct link hits after viewport changes, retained address
authority and stale-result refusal. Knot supplies the second consumer receipt.

### N5. Turnstone lane + receipt

Register the lane, route the scheme(s) from N0, wire history/graph capture
so a browsed node page is a node in the graph like any other address.
Headed receipt against a real NomadNet node (or our own node stood up from
the retinue side) on the LAN.

## Not in scope

- **LXMF messaging.** Outrider's lane, enters through Comms under P5.
- **Serving our own pages** is owned by Knot's authoring/publication plan and
  Djinn's resident-services plan. In-process static serving already exists;
  persistent hosting is independent of full Micron interaction.
- **Propagation nodes and offline delivery.** Retinue roadmap, not browsing.
- **Enrichment beyond presentation** (graph annotations, illume passes over
  micron prose): welcome once N4 stands, settings-gated per the
  configurability rule.

## Ordering

The initial N0/N1-first proposal is superseded by the landed native NomadNet
lane. Continue N4's shared reading fidelity with a bounded reference-evidence
lane, then promote qualified interactions. Gemini-over-Reticulum remains a
separate adapter acceptance; the existing example does not prove a Turnstone
browser lane. Persistent serving and foreign theme export keep their own scopes.
