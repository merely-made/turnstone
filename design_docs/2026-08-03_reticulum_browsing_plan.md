# Reticulum browsing — NomadNet nodes and capsules, idiomatically

**Date:** 2026-08-03
**Status (2026-09-13):** native NomadNet page fetching, shared Micron preview,
and a focused-page Micron form conversation are implemented; full presentation
and interaction remain scoped below. Doctrine,
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
  `crates/retinue/src/nomadnet.rs`. `StringMapRequest` in `request.rs` carries
  the bounded, observed Micron form map; it does not grant a handler authority.
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
File transfer, named discovery UI and durable client identity remain distinct
further gates. Turnstone's form actor opens a fresh ephemeral link and sends
only an explicitly confirmed typed map to the named native target.

### N4. Micron presentation and interaction (reading implementation; qualification remains)

The canonical completion scope is
`mere/design_docs/nematic_docs/implementation_strategy/2026-07-01_smolweb_fidelity_plan.md`,
section "Micron completion scope (2026-09-13)". Reading fidelity and horizontal
table navigation come first, then anchor/fold behavior; typed request evidence
gates forms, and partials/media/directives each need their own lifecycle proof.
The first promoted form surface is intentionally separate from the inert page
rendering: the command palette exposes **Fill Micron form** only for a focused
native Micron page. It walks actions and fields in the omnibar, masks masked
text, shows the resolved native target before a literal `send`, and retains no
submitted values in an address or history entry. The editor binds source member,
address and exact fetched body before preparing the map; a changed source or a
late receipt is dropped. The response remains a submission receipt, not an
implicit navigation.

Six focused form tests pass with the isolated Cargo home, clean `C:/t` cwd,
absolute manifest, Rust 1.97.1 and `--locked --offline --lib micron_form`.
They cover action selection (including fixed-variable-only forms), visible
validation errors, masked display, one confirmed effect, source/address
staleness, navigation/reload/cancel followed by a late reply, and redacted
debug output. An explicit-interface loopback test checks the actual typed map,
a 4096-byte Resource reply, and refusal under a smaller response cap.
The request deadline includes bounded graceful endpoint shutdown, so the
Resource acknowledgement reaches the server before the interface closes.
Response bounds remain after Resource reassembly, before further decoding;
outgoing request Resources remain unsupported by Retinue.
The two existing smolweb input tests also pass. The immutable dependency graph
uses one Mere `91c6238d`, Genet `101d9e9a`, Knot `d100402` and Retinue `2563202`
source identity each. The Retinue typed map receipt supplies the independent
stock-client interoperability evidence. These are automated seams rather than a
headed desktop acceptance; that acceptance was taken on 2026-09-13 and is
recorded below.

**Headed form acceptance, 2026-09-13.** Artifacts are at
`C:\t\micron-headed-20260913`, whose `RECEIPT.md` indexes the captures, logs,
scenarios and scripts. Two independent instruments observed the sends: a
controlled public-RNS `nomadnetwork/node` request handler appending every
received typed map to `handler/requests.jsonl` with a wall-clock timestamp, and
a real stock `nomadnet` 1.4.2 daemon under WSL whose stock executable page
records the `field_*` environment it receives to `stock-node/submissions.jsonl`.
Stock NomadNet stayed black-box. The binary was Turnstone `d710af9`, built from
`C:/t` with the isolated Cargo home, Rust 1.97.1 and an absolute manifest, using
`--offline` without `--locked`: Turnstone gitignores `Cargo.lock` by policy and
the in-repo lock records local path overrides, so a locked receipt needs a clean
worktree. The `--offline` build re-resolved the 14 path-patched packages to
their declared git revs, which is the clean-cwd resolution. The window drove
itself through `TURNSTONE_SCENARIO` scenarios with no OS input; because
`assert text` does not see omnibar text, outcome assertions use
`assert event smolweb-submission-succeeded|failed`.

Opening the page, opening the form and editing fields produced no request beyond
the ordinary page fetch — Turnstone sends empty bytes there where stock NomadNet
sends `nil`. Scenario s1 edited every field and sent once: exactly one observed
map with `field_hd_text="edited café 雪"`, `field_hd_empty="filled"`,
`field_hd_mask="secret"`, `field_hd_checks="red,blue"`,
`field_hd_radio="blue"`, and the reply visible in the omnibar. The three
cancellation paths — s2 Escape, s3 navigate away, s4 Reload — each sent one map,
showed `Cancelled locally; the remote outcome may be unknown`, and never
displayed the handler's late reply, which was sent 8 s later. s5 with
`TURNSTONE_NOMADNET_TIMEOUT_SECS=3` against an 8 s handler gave
`failed: Micron form request timed out after 3s; remote outcome may be unknown`
with one request and no retry. s6 with `TURNSTONE_NOMADNET_MAX_PAGE_BYTES=4096`
gave `failed: Micron form response exceeds TURNSTONE_NOMADNET_MAX_PAGE_BYTES`
for both a 64 KiB and a 5 MiB reply, because the cap is applied to received
bytes before decoding. A further scenario sent the edited map to the stock
daemon, which recorded one `submissions.jsonl` line of the same shape plus
`link_id`; the node passes the `str → str` map verbatim as `field_*`
environment and does no splitting or typing. Knot supplied the second consumer
half of the same acceptance, including a defaults send to the same daemon.

Still open after this acceptance: inline form widgets, partial refresh, outgoing
request Resources, multi-segment responses, authentication and dynamic page
hosting in Djinn. The cross-consumer follow-up is Knot's, not Turnstone's — it
compares its response cap only after unpacking, so a 5 MiB reply surfaces as an
invalid-response error when Retinue's single-segment `MAX_SEGMENT_SIZE`
(1,048,575 bytes) is hit; Turnstone's before-decode ordering is the reference
behaviour here.
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
