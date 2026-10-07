# Unusual-protocols browser plan

**Date:** 2026-10-06
**Status (2026-10-06):** proposed. First-round forks ruled the same day (U1, U4 and U8; U2 as read, pending confirmation); the rest are with Mark. An assessment lane wrote
this plan. No code changed, and nothing was built or run (see "Evidence and
limits"). Stages S0 to S11 wait on the forks in §7.

## The bar

Mark, 2026-10-06, verbatim:

> Woodshed done right is worth $100,000 at least. Knot editor, too. Turnstone
> done right, $1,000,000. Even before genet is 50% standards-compliant. The
> bar for turnstone is basically 'can you use this as a web browser for the
> unusual protocols,' embedded webviews aside. It is meant to be a composition
> of pelt and graphshell first and foremost, and the other integrations are
> certainly possible, expected, and useful but those two capture the browsing
> and p2p axes. It would be really interesting if the apps of the stack could
> kinda compose together… runtime or built… hmm. But that sounds quite a tall
> order for a dude and an llm.

*Reading, not ruled:*

- The bar is set by the small-web and Reticulum lanes, not by HTML.
- Web content through Genet, Weld and Scry stays out of it.
- Pelt (browsing) and Graphshell (peer-to-peer and graph) are the shape.
- Composing the stack's apps is wanted, but it is not part of the bar.

Who "you" is in the bar, a source builder or a person who downloads a
package, is fork U1.

## Evidence and limits

- **Turnstone:** `main` at `a616ece`, one commit ahead of origin. The
  committed pins are Mere `3d1cdacc`, Genet `69a2383b`, Knot `92719898`,
  Woodshed `9e982b88` and Retinue `fa4f9250`.
- **Another session's work:** the working tree holds another active session's
  uncommitted browser-supplier work. It repins Mere to `db4ee312`, Knot to
  `91cb44a2` and Woodshed to `b613fc55`, and adds `src/shell/servo.rs`. Those
  files were not assessed or touched, and every Turnstone line reference
  below is to `HEAD`.
- **Mere moved during this assessment:**
  - `origin/main` reached `392630bb` at 21:36, merging the browser branch's
    ancestry. Turnstone's committed pin (`3d1cdacc`) and the working tree's
    pin (`db4ee312`) are both now on `origin/main`.
  - Mere's local `main` (`4e57913e`) carries eight unpushed commits, among them
    WS4's R0 to R5 (`b4f14f2c`).
  - Claims about Mere below name the revision they were read at.
- **Nothing was built or run, for three reasons:**
  - C: had 367 GB free, under the 500 GB line.
  - Another session was building Turnstone at the same time.
  - The claims a run could settle need new fixtures or tests, which this
    read-only lane may not add.

  So every "fails" below is a traced code path with citations, and it opens
  its stage as a failing test.
- **Receipts and tests used:**
  - [Smolweb acceptance](../docs/receipts/smolweb_acceptance_20260821/README.md),
    with seven headed Gemini cases plus Titan and Spartan mutations.
  - [NomadNet acceptance](../docs/receipts/nomadnet_acceptance_20260911/README.md).
  - The Scroll note in the
    [browser gap analysis](2026-08-17_smolweb_browser_gap_analysis.md).
  - The 2026-09-13 Micron form acceptance recorded in Mere's smolweb fidelity
    plan. Its artifacts were deleted by 2026-09-16, and the tests it cites
    still exist.
  - The Turnstone unit tests named per row.
  - The last full gate:
    [635 workspace tests, nine ignored](2026-09-29_shared_diagnostics_gloss_inspector_receipt.md).

## Findings (2026-10-06)

### 1. What works today, per protocol

The cell values mean the following:

- **working:** a checked-in receipt or a test shows it.
- **wired:** the code path exists but nothing in Turnstone exercises it.
- **partial:** some of it works.
- **missing:** there is no path.
- **fails:** a traced path ends in an error.

| Lane | Fetch | Render | Navigation | Forms and input | Trust | Evidence |
|---|---|---|---|---|---|---|
| Gemini | working | working: site palette, typography, streaming, inline images | working: links, per-node Back and Forward, Stop and Reload, hover preview | working: status 10 and 11 with masking; persona-derived client certificates per capsule | partial: durable TOFU pins and a changed-certificate decision; no posture shown | headed smolweb acceptance (7 Gemini cases); `gemini_trust.rs` (3 tests), `gemini_identity.rs` (2), `ui.rs` changed-certificate test |
| Spartan | working | working | wired: shares the gemtext link path, but no receipt follows a link | working: `=:` opens the body composer | missing: unauthenticated by design, but not shown | headed `spartan-mutation` receipt |
| Titan | n/a (write lane) | n/a | n/a | working: body, MIME, masked token, literal confirmation | shares Gemini's TOFU and identity | headed `titan-mutation` receipt |
| Scroll | working | working | working | missing: no client-certificate UI | partial: TOFU as Gemini | headed local TLS receipt (gap analysis, 2026-09-11); Scroll route test in `src/shell/mod.rs` |
| Gopher | wired | partial: links become paragraphs and info lines join one preformatted block, losing type and column; text files (type `0`) **fail** | wired | missing: a type-7 search is a bare link; after WS4 it becomes a submit that Turnstone refuses | missing | no Turnstone test or receipt; traces below |
| Finger | wired | wired: the body is typed `text/x-finger` | n/a | n/a: the query is the address | missing | no test or receipt |
| Nex | wired | wired: every response is typed `application/x-nex` | wired | n/a | missing | no test or receipt |
| Guppy | wired | wired | wired | wired: a prompt maps to the shared input status (`errand/src/guppy.rs:38`, `fetch/src/lib.rs:650`) | missing | no test or receipt |
| Feeds (RSS, Atom, JSON Feed, gemtext) | working through subscriptions | partial: `rss+xml`, `atom+xml` and `feed+xml` render; `text/xml` and `application/xml` route to Livery | partial: entries are graph nodes with unread tags; unread counts appear per source in Inspector | n/a | n/a | `feed.rs` (5 tests), `app/feed_arms.rs` (3); no headed receipt |
| NomadNet and Micron over Reticulum | partial: the TCP interface is set only by `TURNSTONE_NOMADNET_TCP`; a temporary identity per fetch | partial: the shared Micron reading subset | working: same-node links, exact Stop, Reload and stale-answer handling; the shared session's folds and anchors are pointer-only and unqualified in Turnstone | working: forms (headed 2026-09-13; artifacts lost) | missing: the destination identity is proven but not shown | headed NomadNet acceptance; `nomadnet.rs` (4 tests) |
| Misfin | n/a | **fails**: routed to `nematic.misfin`, which Turnstone does not register | n/a | missing: no mail surface | n/a | Inker routing at `3d1cdacc` lines 231 and 328; `src/shell/mod.rs:77-110` |
| Gemini over Reticulum | missing: Retinue example only | | | | | [Reticulum browsing plan](2026-08-03_reticulum_browsing_plan.md) |

Six gaps hold across every smolweb page, whatever the protocol:

- **Plain text and Markdown bodies fail.** The traced path is below.
- **No find, page zoom or capture.** Smolweb sessions report find in page,
  page zoom and page capture as unsupported
  (`mere/crates/system/document-lanes/src/session.rs`, lines 25-38).
- **No text selection or copy.** The smolweb document has none
  (`document-lanes/src/smolweb.rs`).
- **The keyboard reaches no link.** Turnstone sends a session only scroll keys
  (`src/shell/keys.rs:64`). It never calls the session's `focus_move` or
  `key_input` (`document-lanes/src/session.rs:178-203`), which already move
  focus through links and fields in document order.
- **No lane shows trust.** Every Nematic engine sets
  `DocumentTrustState::Unknown`, and nothing fills it (smolweb fidelity plan,
  §1 correction of 2026-10-06).
- **Assistive technology hears only an outline.** Document sessions publish a
  structural outline, marked `Partial` (`src/a11y.rs`, module doc). The human
  Narrator pass covered the Frozen Projection pane, not a page.

**The text-document trace.** Each step was checked in code:

1. A Gopher type-`0` reply is typed `text/plain` (`gopher-protocol` 0.1.1,
   `src/client.rs:150`). Fetch passes the reply's type through for every
   scheme except Finger, Nex, Guppy and Titan
   (`mere/crates/system/fetch/src/lib.rs`, lines 920-933 at `3d1cdacc`).
2. Inker routes by content type before scheme (`inker/src/routing.rs`,
   `route_filtered` from line 117). `text/plain` maps to `nematic.text`
   (line 270), and Markdown maps to `nematic.markdown` (line 260).
3. Turnstone calls the unfiltered `route` (`src/shell/effects.rs:1082`), so a
   decision can name an engine it never registered.
4. Turnstone registers Livery, Reader, Micron and eight Nematic ids: Gemtext,
   Gopher, Finger, Scroll, Nex, Guppy, Titan and Feed
   (`src/shell/mod.rs:77-110`).
5. Spawning an unregistered id returns `EngineNotFound`, which reads "session
   engine not registered"
   (`genet/components/shared/document-session-api/src/session_engine.rs:50`
   and `:1134`). The node then shows a failed tile.
6. Registering `nematic.text` alone would not fix Gopher. Document-lanes
   chooses its lowering by scheme only (`document-lanes/src/smolweb.rs:938`
   at `3d1cdacc`; unchanged at `b4f14f2c`), so a Gopher text file would be
   read as a menu.

**The Gopher search trace.** After WS4 (`b4f14f2c`, unpushed), a type-7 row
lowers to an `InlineSpan::Submit` with a `gopher://` target
(`nematic/src/gopher.rs:104`). Turnstone's submission handler accepts only
`titan://` and `spartan://`, and refuses everything else with a typed event
(`src/app/node_arms.rs:1328-1336`). A Gopher search also needs a query
appended to its selector, not a body upload.

### 2. Gaps to the bar, ranked

Ranked by how far each stops a person, today, from using Turnstone as their
browser for these protocols.

1. **G1. Plain-text and Markdown documents fail to open.** This covers Gopher
   text files, a Gemini capsule's `.txt` and `.md` files, and Misfin links.
   Gopher is unusable past its menus. See the trace in §1. Closed by S1.
2. **G2. The reading basics are missing on every smolweb page.** There is no
   find, no page zoom, no text selection or copy, and no keyboard path to a
   link. Lagrange, the gap analysis's benchmark, has all four. The evidence
   is in §1. Closed by S2.
3. **G3. NomadNet works only from a shell.** It needs
   `TURNSTONE_NOMADNET_TCP` and a reachable Reticulum TCP interface
   (`src/nomadnet.rs:240`). A temporary identity is minted for every fetch
   (`:262`). There is no node directory from announces and no interface
   setting. Closed by S5.
4. **G4. There is no package, no updater, and no non-Windows evidence.**
   Running Turnstone means building from source with MSVC, the Windows SDK,
   CMake and Ninja (`README.md`). No packaging configuration exists, and the
   untracked `.github/workflows/portable.yml` only checks the locked graph.
   The default build already carries no webview (`default = []`), which
   makes a protocols-only package simple. Its rank depends on U1. Closed by
   S9.
5. **G5. Trust is invisible.** No lane shows TOFU state, unauthenticated
   transport or a Reticulum-proven destination. Prompts exist only for a
   changed Gemini certificate and a client-certificate request. Closed by S3.
6. **G6. Gopher fidelity is poor.**
   - Search fails, and that failure gets worse after WS4 (see §1).
   - Item types and column alignment are lost until WS4 lands in Turnstone's
     graph.
   - CSO, telnet and the 8/T fix wait on `gopher-protocol` 0.2.0, which is
     prepared but unpublished.
   - Nothing qualifies Gopher in Turnstone.

   Closed by S4.
7. **G7. Product configuration lives in environment variables.** The
   download directory, inline images and their limits, the NomadNet
   interface, timeout and size cap, and the web timeout are all set this
   way. The Settings pane holds six entries: theme id, theme mode, UI zoom,
   shellbar edge, shellbar visibility and behavior cascade budget
   (`src/settings_provider.rs:191-250`). It has no reading typography, no
   hard-break choice and no per-protocol theme. Closed by S6.
8. **G8. There is no home and no search entry point.** First launch is the
   canvas plus a summoned omnibar, and non-address text only finds graph
   nodes. No search capsule can be reached from the omnibar, and bookmarks
   are kept nodes reached through the Roster. Closed by S6.
9. **G9. Revisits need the network.** Back and Forward refetch the revealed
   address (gap analysis, Navigation table). No body is retained for a
   visited smolweb page, and capture's offline replay is still open
   ([page capture plan](2026-08-28_page_capture_plan.md)). Closed by S7.
10. **G10. Page content reaches assistive technology as an outline only.**
    Links cannot be reached by keyboard (G2), and no human pass covered a
    page. Closed by S8.
11. **G11. Finger, Nex and Guppy are wired but unqualified.** Guppy input and
    Gopher binary downloads are untested. Closed by S4.
12. **G12. Feed viewing is partial.**
    - XML-typed feeds open in Livery.
    - Nothing gathers unread entries across feeds; Inspector shows counts per
      source.
    - WS4's article reader is not yet in Turnstone's graph.
    - No headed feed receipt exists.

    Closed by S1 (routing) and S4 (qualification).
13. **G13. Crashes leave no record.** There is no panic hook. The recovery
    receipts cover a failed fetch, a failed spawn and session restore
    (`scenarios/rung_recovery.scn`). Closed by S9.
14. **G14. The pins are stale.** This is a precondition, not something a
    user sees: 652 commits separate Turnstone's pin from Mere's
    `origin/main`, and `README.md`'s pin list disagrees with `Cargo.toml` (§5).
    Closed by S0.

### 3. Pelt and Graphshell, the two axes

**Graphshell is composed.** Turnstone links the `graphshell` port,
`graphshell-client` and `graphshell-endpoint`, and uses them for six things:

- **Places:** `graphshell::network_carrier` and `admission` in
  `src/place/lanes.rs`.
- **Projection hosting:** `graphshell::carrier` in
  `src/place/projection_host.rs`.
- **The G3 remote projection endpoint:** `src/remote_projection.rs`.
- **Knot authoring sessions:** `graphshell::client` and the
  `projection_editor` types in `src/knot_authoring.rs`.
- **Device receipts:** through Djinn's app broker
  (`src/device_receipts_service.rs`).
- **The Frozen Projection pane:** `graphshell_client::frozen`.

So the peer-to-peer axis is real code with receipts: place founder, joiner,
reader and reconnect scenarios, and the I3 receipts in Mere's suite census.

**Pelt is not composed.** Turnstone has no dependency on `pelt` or
`pelt-core`. `pelt-core`'s `PeltController` owns a retained session, its
engine registries, navigation history, input effects and frame production
(`mere/ports/pelt/core/README.md`). Its `PeltWorkspace` arranges one
controller per tile through the shared `TileTree`.

Turnstone does the same job with its own content port, and its version is
further along:

- exact request identity, Stop and Reload;
- input and trust conversations;
- downloads, inline images and streaming;
- per-node lineage;
- its own Workbench tiling.

Both sit on the same lower layers: Genet's `document-session-api`,
`mere-document-lanes`, Inker routing and Workbench. Mere's suite census names
Pelt "the tiled, nesting document viewer and browser"
(`mere/design_docs/2026-08-22_turnstone_suite_composition_and_capability_census.md`
§2). In code, then, Turnstone is Graphshell plus its own Pelt, which is the
kind of app-local copy the stack avoids.

**What is missing for the two axes to feel like one product:**

- **One browsing controller.** Either Turnstone's controller moves into
  `pelt-core` and both hosts consume it, or the duplication is ruled
  acceptable (U7).
- **Scenograph over Turnstone's own graph.** Mere ruled that the projection
  editor appears "in every application that has its own app graph" (census
  §3). In Turnstone it appears only inside Knot authoring.
- **Browsing that flows into places.** A visited capsule is a graph node, but
  sharing a kept smolweb page into a place still goes through the page
  lifecycle plan's open joins ([page lifecycle](2026-09-06_page_lifecycle_plan.md)).
- **One Reticulum.** Reticulum is a browsing lane (NomadNet) and a candidate
  place carrier (the P5 row in the
  [peer-web reframe](2026-07-28_turnstone_peer_web_reframe.md)). Nothing
  shares one transport, identity or interface set between the two.

### 4. Composition with Knot and Woodshed

**What exists:**

- **Knot, compiled in and over sessions.** Turnstone pins `knot-editor` and
  `knot-document` at `92719898`, which pins Mere `3d1cdacc`, and links Mere's
  `knot-editor-host`. `src/knot_authoring.rs` (3,803 lines) hosts Knot over
  resident endpoint sessions. `KnotDocumentProvider` is a contributed
  surface. The place port edits a founder-held Knot document by projection
  (I3h, 2026-09-15). Mere's djinn and its root pin Knot `ef89a186`, which pins
  Mere `e0cea3e0` and Genet `d851a9db`. Knot's in-Graphshell plan is
  historical; its K2 physical two-machine receipt passed on 2026-08-08
  (`knot-editor/design_docs/2026-08-02_knot_in_graphshell_plan.md`).
- **Woodshed, compiled in through Redshank.** Redshank is Woodshed's listening
  port, pinned at Woodshed `9e982b88`; its nested workspace pins Mere
  `3d1cdacc`. Its compact dock is Turnstone's fourth contributed-surface
  provider and the first from outside Mere
  ([Redshank episode surface](2026-09-14_redshank_episode_surface_plan.md)).
  The microphone and a headed receipt are open. Woodshed's own practice
  views (`woodshed-views`) build on `cambium`, `workbench` and
  `mere-surface-api`, the same contracts Turnstone admits.
- **The surface contract.** `mere-surface-api`'s descriptor vocabulary has
  been frozen as v1 since 2026-08-26 (`mere/crates/system/surface-api/surface.rs`).
  Turnstone registers four providers: Knot document, Sky, Distillery
  installed and Redshank episode (`src/shell/mod.rs`, `SurfaceProviderRegistry`).
- **Producers.** Cambium's `TextureProducer` is a same-process, same-device
  seam. It lets an application render into a slot the host lays out, with
  accessibility semantics and actions
  (`mere/crates/cambium/cambium-rootstock/src/producer.rs`). Turnstone does
  not use it.
- **Scenograph.** The projection editor is ruled to appear in every app with
  its own graph, holding a lens and never truth (family composition thesis,
  §3). Woodshed's "practice artifact beside a research page" is the brief's
  own example.

**The four composition modes:**

| Mode | What it would take | Cost | Risk |
|---|---|---|---|
| **Built: a crate pane.** The app ships a provider; Turnstone admits it. | A provider crate in the owning repo (descriptor, stylesheet, erased retained session), plus a Turnstone admission module. Redshank's was 480 lines, beside a 1,327-line authority host for the device. | Low to medium per app | Every composed repo joins the tested pin set, so each Mere move needs it moved first. This is already visible inside Woodshed: its root workspace pins Mere `8106c7c2`, while the Redshank workspace Turnstone consumes pins `3d1cdacc`. Device authority (audio, microphone, MIDI) needs one owner. Build weight grows. |
| **Runtime: a session projection.** The app serves a granted projection; the host mounts it and returns intents. | A Graphshell endpoint per app with its projection and intent vocabulary. The host draws it generically (FrozenScene, Scenograph) or with the app's surface crate compiled in, as Knot does now. | Medium per app | Generic drawing loses bespoke interaction (a fretboard is not a card). The holder must be online, as the place port plan records for Knot. The machinery has receipts: G3, K2 and I3h. |
| **Runtime: live pixels.** Another process's producer is shown live in the host's Workbench. | A cross-process producer protocol: shared GPU textures per OS (only D3D12 shared handles exist, from Weld and Scry), input, IME and focus forwarding, accessibility bridged across processes, and lifecycle and crash isolation. | High | High. Turnstone's Scry consumer shows how hard this class is even with a vendor runtime. The [2026-10-05 receipt](../docs/receipts/browser_scry_windows_20261005/README.md) found stale post-input images and blank reopened tiles, and clearing them took a dedicated qualification phase (another session's 2026-10-06 supplier integration receipt, uncommitted when this was written). |
| **Runtime: in-process plugins.** Surfaces arrive as Wasm components. | Turnstone's optional `wasm` feature runs `app-core` components through the action envelope (participant gate B3); no guest contributes a surface. It would need a component interface for retained views and paint, plus admission. | High | Medium to high: interface churn and paint cost. |

**The answer.** For a dude and an LLM, the first two modes are the reachable
ones, and both already have receipts: Knot runs in both, and Redshank is
built. The next compositions are cheap at that altitude:

- Scenograph over Turnstone's graph;
- a Woodshed practice surface as the fifth provider;
- Pelt convergence (U7).

Live-pixel and plugin composition are research. They should wait until a
consumer cannot be served by the first two (U8).

### 5. Staleness: what a Mere repin changes

Read at Mere `origin/main` `392630bb` and the unpushed `b4f14f2c`. Nothing
was compiled, and the repin's own gate decides.

**The branch question is gone.** `3d1cdacc` and `db4ee312` are now ancestors
of `origin/main`. The browser branch's commits are there, so a repin loses
nothing from that lane.

**The smolweb crates are unchanged on `origin/main`.** Inker, Nematic,
Errand, fetch, `knot-editor-host`, `mere-chrome`, `mere-surface-api`,
`servitor` and `pandect` have no commits since the fork point, and
document-lanes has one one-line change. So a repin to `origin/main` changes
no protocol behavior. WS4's local merge (`b4f14f2c`) does change them once it
is pushed:

- **R0** retires `cambium::nematic`.
- **R1** makes `Block` non-exhaustive and adds `Block::Menu`. Turnstone's one
  `Block` match already has a wildcard (`src/nomadnet.rs:134-167`).
- **R2** lowers Gopher to typed menu rows, with a search `Submit` that
  Turnstone refuses (§1).
- **R3 and R4** add feed fields and the article reader.
- **R5** makes soft breaks a style setting.

**What reaches Turnstone's code on `origin/main`:**

- **Physics setters return a `Result`.** Pictograph's Canvas
  `set_physics_law` and `set_physics_overlays` now return
  `Result<(), OverlayRefusal>` (`mere/crates/canvas/pictograph/src/canvas/physics_catalog.rs`,
  line 1259).
  Turnstone's calls at `src/app/mod.rs:1157` and `:1168` and
  `src/app/session_lifecycle.rs:1050` and `:1062` would compile with
  `must_use` warnings and drop refusals silently.
- **A twelfth physics law.** `PhysicsLaw::ALL` grows from 11 to 12
  (Density). Turnstone parses laws by id, so this adds a choice and breaks
  nothing.
- **New canvas API.** The speed dial (`set_physics_speed`, step budget,
  `physics_pace`), `set_physics_display_rate` (to be fed from winit's refresh
  rate per the physics catalog plan), a host-shared `PhysicsDevice`, and G9's
  `ArrangementAction` (drag and pin as advertised actions, with
  `PermittedActions`) all arrive. Each is new consumer work, not a break.
- **The score version moves from 4 to 5.** `sceno::SCORE_VERSION` is now 5.
  Turnstone refuses any score whose version differs
  (`src/remote_projection.rs:384`), so pinned and repinned peers refuse each
  other until both move.
- **P1 reaches Turnstone only indirectly.** Turnstone makes no direct call to
  `ProjectionCompiler` or `ItemSizes`; its only scenomise use is
  `scenomise::solve` (`src/remote_projection.rs:341`). P1 reaches it through
  Graphshell and Knot.
- **The Graphshell items Turnstone imports look stable.** Of the nineteen
  checked, none shows a changed declaration line, though struct fields were
  not compared. This is despite 106 commits to the port.
- **Personae removals miss Turnstone.** `current_profile` and `slot` are
  gone, and Turnstone calls neither.
- **Burn and Genet change nothing.** Burn is 0.22 on both sides. Genet moves
  from `69a2383b` to `d851a9db` (25 commits). `image-decode` is in
  genet-livery's default features, and Turnstone takes defaults, so decoding
  stays on.
- **Knot and Woodshed must move with Mere.** A repin is a tested-set move.
  Knot must repin first onto a revision that pins the same Mere (djinn pins
  `ef89a186`; Knot `HEAD` `33cc855` pins Mere `9310518b`). Redshank must
  repin to the same Mere. The committed lock today holds one source per
  sibling.
- **The README pin list is wrong.** `README.md` names Mere `bd5912fb`, Knot
  `3dfb70b0` and Woodshed `cefc903d`. `Cargo.toml` pins `3d1cdacc`,
  `92719898` and `9e982b88`.

## 6. Stages

Each stage stops at its done-conditions. Where a stage needs a Mere change,
that half is Mere's work (U3), and Turnstone is the forcing consumer.

### S0. Repin as one tested set

**Depends on:** U9. It comes before any stage that needs a Mere change.

**Done when:**

- Turnstone pins one Mere revision on `origin/main`.
- Genet is at Mere's pin.
- Knot is at a revision that pins the same Mere.
- Redshank is at a Woodshed revision that pins the same Mere.
- `cargo tree --locked` shows one source for each sibling.
- The full library suite passes with `--locked` from a clean working
  directory.
- Physics refusals are surfaced, not dropped.
- Remote projection accepts score version 5 and gives a typed refusal for 4.
- `python scripts/cargo_mode.py verify` passes.
- `README.md`'s pin list matches `Cargo.toml`.

### S1. Every text document opens

**Mere half:**

- Document-lanes chooses its lowering by content type before scheme, for
  `text/plain` and `text/markdown`.
- XML-typed bodies that sniff as RSS or Atom reach the feed engine.

**Turnstone half:**

- Register `nematic.text` and `nematic.markdown` sessions.
- Route with `route_filtered` against the registry, so no decision names an
  unregistered engine.
- Give `misfin:` an explicit tile saying the lane is not offered (per U2).

**Done when:**

- A failing test exists first for each of five cases:
  - a Gopher text file;
  - a Gemini `text/plain` page;
  - a Gemini `text/markdown` page;
  - a Misfin link;
  - an RSS feed served as `application/xml`.
- All five then pass.
- A headed local receipt shows a fixture Gopher server's menu opening a text
  file.
- The receipt also records each engine id chosen.

### S2. Reading basics on every smolweb page

**Turnstone half:**

- Tab and Shift-Tab call the session's `focus_move`.
- Enter and Space call `key_input`.
- The focused stop is visible.

**Mere half:**

- Smolweb sessions gain find through the shared find field.
- They gain per-node page zoom.
- They gain text selection with copy.
- WS4's R5 soft-break setting reaches Turnstone's Settings.

**Done when:**

- A headed local Gemini receipt shows keyboard-only following of three links.
- In the same receipt, find counts and steps through matches.
- Page zoom persists per node across a restart.
- Selected text reaches the clipboard.
- The engine capability disclosure for find, zoom and selection reads
  supported.

### S3. Trust posture shown

**Mere half (WS2):**

- The transport produces a posture per carrier:
  - TLS TOFU: first seen, matched, or changed and accepted;
  - TCP: unauthenticated;
  - Reticulum: destination proven.
- The posture fills `EngineDocument.trust`.

**Turnstone half:**

- The chrome shows the focused node's posture.
- Inspector shows it too, and opens the TOFU review from it.

**Done when:**

- A Spartan page reads unauthenticated.
- A Gemini page reads its pin state.
- A NomadNet page reads destination-proven.
- The card and the tile show the same posture.
- No successful load reads Unknown.

### S4. Gopher and the small lanes qualified

**Depends on:** WS4 pushed; R6 (`gopher-protocol` 0.2.0) published on Mark's
sign-off.

**Turnstone half:**

- A Gopher search row opens the input prompt and fetches the selector with
  the query appended, as a fetch, not a body upload.
- Binary item types enter download custody.

**Done when:**

- A headed local receipt shows menu columns aligned.
- In it, a type-7 search returns results.
- A text file opens.
- A binary item becomes a download.
- Finger, Nex and Guppy each have a focused test and a local fixture receipt,
  with Guppy's prompt answered.
- A feed receipt subscribes, refreshes, marks read, and opens an entry
  through the article reader.

### S5. NomadNet without environment variables

**Depends on:** U5.

**Work:**

- Settings holds interface configuration, a TCP host and port first.
- A persistent Reticulum identity derives from the persona.
- An announce-fed node directory appears as graph nodes; keeping one is
  bookmarking it.
- Node `/file/` paths enter download custody.
- The `TURNSTONE_NOMADNET_*` variables remain only as overrides for
  scenarios.

**Done when:**

- On a fresh profile with no `TURNSTONE_NOMADNET_*` set, a person can
  configure an interface in Settings.
- They see a stock node appear from its announce.
- They open the node's index page, submit a form, and download a file.
- A restart keeps the interface, the identity and the directory.

### S6. Settings and home

**Depends on:** U6.

**Work:**

- Move every product setting now read from the environment into the settings
  provider.
- Add reading settings: body face, size, measure, hard breaks and
  per-protocol theme.
- Build a home pane from graph truth:
  - kept capsules;
  - subscriptions with unread counts;
  - the recent trail;
  - a search field bound to a configured search capsule.

**Done when:**

- Every moved setting changes live and persists, and the environment only
  overrides.
- First launch on a fresh profile shows the home pane.
- Non-address text offers "search with <capsule>" in the omnibar.

### S7. Offline and revisits

**Depends on:** U10.

**Work:**

- Keep the last body of each visited smolweb address, in a bounded store.
- Back and Forward show the kept body, then refresh by policy.
- An offline visit shows the kept body, marked stale.

**Done when:**

- With the network disabled, previously visited Gemini, Gopher and NomadNet
  pages open and read.
- The stale marker shows.
- The store stays within its configured bound.

### S8. Accessibility of pages

**Work:**

- Smolweb sessions publish their content into the stitched AccessKit tree
  (headings, paragraphs, links and fields), not just an outline.

**Done when:**

- An `assert a11y` scenario lists link names in document order.
- Mark, at the machine with Narrator, reads a Gemini page and a Gopher menu,
  follows a link by keyboard, and the receipt records it.

### S9. Distribution and resilience

**Depends on:** U1.

**Work:**

- Package the default (no-webview) Windows build with `cargo-packager`.
- Add signed updates through Mere's `luggage`, meeting Hocket's update
  requirements: configurable policy, honest status, verification before
  apply, and safe rollback.
- Install a panic hook that writes a crash record before the process ends;
  the next start restores the session.
- Add a cross-target `cargo check` of default features for Linux and macOS.

**Done when:**

- On a clean Windows account, the package installs and browses.
- It updates to a test release and rolls back.
- A forced panic writes its record, and the next start restores the session.
- The cross-target checks pass.
- Headed runs on other systems follow U1.

### S10. Bar acceptance

**Done when:**

- An installed build with no environment variables browses each U2 protocol
  against a local fixture, with a dated receipt.
- A real-world reference is included where one exists (network-dependent,
  kept from its first clean run, as the August receipts did).
- Mark rules the bar met after using it as his browser for these protocols.

### S11. Composition

This stage runs independently of S1 to S10, per U8. Three independent parts:

- **S11a, Scenograph over Turnstone's graph.**
  - Turnstone hosts the projection editor over its own graph, holding a
    lens.
  - Done when an authored projection of Turnstone's graph is saved, reopened
    and restored across a restart, and Graphshell reads the same projection.
- **S11b, a Woodshed practice surface.**
  - Woodshed's lane ships a provider over `mere-surface-api`, and Turnstone
    admits it as the fifth provider. One process-wide owner holds the audio
    device.
  - Done when a staged practice Set renders beside a page and its Rehearsal
    plays. The same surface also runs in `woodshed-genet`.
- **S11c, Pelt convergence (per U7).**
  - Done when Turnstone and Pelt drive document tiles through one
    controller, and both hosts' scenario suites pass.

## 7. Forks for Mark

The recommendation is listed first.

**U1. Who is "you" in the bar?**

- **(a)** A person who downloads a Windows package (S9). Linux and macOS get
  cross-target checks only, until a headed lane is opened.
- (b) Mark, and anyone who builds from source. Packaging drops below every
  reading gap.
- (c) A person on Windows, macOS and Linux. This needs headed lanes on the
  iMacs, Fedora and Mint.

**U2. Which protocols are in the bar?**

- **(a)** Every lane Turnstone already wires: Gemini, Spartan, Titan,
  Scroll, Gopher, Finger, Nex, Guppy, feeds and NomadNet. Misfin gets an
  explicit "not offered" tile.
- (b) Option (a), plus a send-only Misfin composer (Errand has the send
  path).
- (c) Option (a), plus Gemini over Reticulum (only Retinue's example exists).

**U3. Who does the Mere halves of S1 to S4?**

These are document-lanes lowering, session find, zoom and selection, WS2
trust, and the WS4 tail.

- **(a)** Mere's smolweb fidelity lane, with Turnstone as the forcing
  consumer and one dated note in each plan.
- (b) This Turnstone lane, in Mere worktrees.
- (c) Turnstone works around them locally. This contradicts
  consolidate-into-the-stack.

**U4. Stage order.**

- **(a)** S0, then S1, S2, S3, S4 and S5 (reading correctness before reach),
  then S6 to S10.
- (b) S5 first, since NomadNet is the most unusual lane.
- (c) S9 first, so a package exists early for others to try.

**U5. Where does NomadNet's interface and identity live?**

- **(a)** In Turnstone for now. Its settings and its persona-derived
  identity are shaped to move into a device-resident Reticulum service.
  Signalman is not built yet, and Djinn owns resident lifetimes.
- (b) Found the device-resident Reticulum service first, and have Turnstone
  consume it.
- (c) Keep the environment variables. NomadNet stays a developer lane.

**U6. What does the home look like?**

- **(a)** A home pane built from graph truth (kept capsules, subscriptions,
  recent trail, a search capsule), with an optional home address as a
  setting.
- (b) Only a home address: a capsule or a local `.gmi` file.
- (c) No home. The canvas stays the start.

**U7. Pelt and Turnstone's browsing loop.**

- **(a)** After S4, move Turnstone's proven controller pieces into
  `pelt-core`, then have both hosts consume it.
- (b) Adopt `pelt-core` now, before the protocol stages.
- (c) Rule Turnstone's content port the browsing axis's home, and leave Pelt
  as the standalone reference viewer.

**U8. Composition modes and timing.**

- **(a)** Crate panes and session projections only. S11a first, then S11b.
  Both run alongside the bar stages.
- (b) Composition waits until S10 is accepted.
- (c) Open a live-pixel research lane now, beside the crate panes.

**U9. Repin timing.**

- **(a)** S0 now, onto `origin/main` once WS4 is pushed. The tested set
  moves together: Knot first, then Redshank, then Turnstone. It is
  coordinated with the browser session that owns the dirty tree.
- (b) Repin to `origin/main` `392630bb` now, before WS4, and repin again for
  S4.
- (c) Stay on `3d1cdacc` until S1 needs a Mere change.

**U10. What keeps revisited pages?**

- **(a)** A bounded per-profile body cache. It stays separate from captures,
  which remain deliberate acts with provenance.
- (b) Every visit becomes a source capture under the page capture plan.
- (c) No offline reading in the bar.

### Rulings (2026-10-06, first round)

- **U1, who "you" is.** Options: a person who downloads a Windows package;
  source builders only; all three operating systems. Mark: **"All three
  operating systems"**. *Follows:* S9's packages and updater cover Windows,
  macOS and Linux.
- **U2, the protocols in the bar.** Options: every wired lane, with Misfin
  shown as not offered; that plus a Misfin composer; that plus Gemini over
  Reticulum. Mark answered in his own words: **"misfin work from the comms
  work does exist already. gemini over reticulum doesn't seem too bad. i am
  curious about accessibility in nomadnet; meshchatx is rough for linux users
  with a screen reader, my friend told me. so i would not like to repeat that
  characteristic if possible"**. *Reading, not ruled (put back for
  confirmation):*
  - Misfin enters the bar through mere's existing comms and errand Misfin
    work;
  - Gemini over Reticulum enters too;
  - NomadNet pages must be usable with a screen reader on Linux (AT-SPI,
    Orca), a requirement of S5 and S8, so as not to repeat MeshChatX's
    reported weakness.
- **U4, stage order.** Options: reading correctness first; NomadNet first;
  packaging first. Mark: **"Reading correctness first (Recommended)"**.
- **U8, composition.** Options: crate panes and session projections
  alongside the bar work; wait until the bar is met; also a live-pixel
  research lane now. Mark: **"start the live-pixel research lane but don't
  start integration yet... if somehow the forest dom can accommodate as long
  as the app is built on cambium... that would be pretty nice"**.
  *Follows:* a research lane only, with no integration. It weighs live
  pixels from another process against composition at the tree level: a
  Cambium app's tree mounted into another Cambium host's forest document
  (stack seams P2, `cambium-rootstock/src/multi_host.rs`). Mark also pointed
  it at the archived tear-out plans: mere
  `archive_docs/2026-07-04_completed_plans/2026-06-19_tearout_composability_plan.md`
  (C2's external-texture bridge, C3's cross-window pane resolution, C4's
  cross-graph composition) and
  `archive_docs/2026-10-06_completed_plans/2026-06-24_tearout_gestures_plan.md`.

## Progress

- **2026-10-06:**
  - Assessment written by a read-only lane.
  - Findings were read against Turnstone `a616ece`, Mere `3d1cdacc`,
    `392630bb` and `b4f14f2c`, Genet `69a2383b`, Knot `92719898`, `ef89a186`
    and `33cc855`, and Woodshed `9e982b88`.
  - No code changed, and nothing was built or run.

## Cross-references

- Mere smolweb fidelity plan:
  `mere/design_docs/nematic_docs/implementation_strategy/2026-07-01_smolweb_fidelity_plan.md`
  (WS1, WS2, WS4 R0 to R6).
- Mere S14 record: `mere/support/doc-audit/d2/batch_51_s14_phase_b13.md`.
- Mere stack seams plan:
  `mere/design_docs/mere_docs/implementation_strategy/2026-10-04_stack_seams_plan.md`
  (P1, S62, S66, S70 to S75).
- Mere suite census:
  `mere/design_docs/2026-08-22_turnstone_suite_composition_and_capability_census.md`.
- Mere family composition thesis:
  `mere/design_docs/2026-08-12_family_composition_thesis_brief.md`.
- Mere auto-update brief: `mere/design_docs/2026-07-22_auto-update_brief.md`.
- Hocket auto-update plan:
  `woodshed/ports/hocket/design_docs/2026-07-24_auto-update_plan.md`.
- Turnstone plans this one feeds:
  - [browser gap analysis](2026-08-17_smolweb_browser_gap_analysis.md)
  - [browser surfaces](2026-08-25_browser_surface_implementation_plan.md)
  - [Reticulum browsing](2026-08-03_reticulum_browsing_plan.md)
  - [page capture](2026-08-28_page_capture_plan.md)
  - [pane registry](2026-08-08_pane_registry_and_graph_panes_plan.md)
