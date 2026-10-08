# Unusual-protocols browser plan

**Date:** 2026-10-06
**Status (2026-10-07):** ruled. U1 to U14 are ruled: U1, U2 (accessibility
on all three operating systems comes first), U3, U4, U5, U8 and U9 on
2026-10-06, and U6, U7, U10 and U11 to U14 on 2026-10-07 (§7). The
composition brief's AC1 to AC6, U15, U16 and U17 are ruled (§7, fifth round). The stage
order is in "Stage order (2026-10-07)" under §6. S0 is next; its lane is
written under S0 and waits on Knot's repin. A cloud
session owns the plan from 2026-10-07; an assessment lane wrote it, and no
code has changed (see "Evidence and limits").

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
| **Runtime: live pixels.** Another process's producer is shown live in the host's Workbench. | A cross-process producer protocol: shared GPU textures per OS (the full export, import and fence set exists only on Windows, as D3D12 from Weld and Scry; Linux, through DMABUF over Vulkan, and macOS, through `MTLTexture`, have import halves only, and nothing brokers a handle between two of the stack's own processes: corrected 2026-10-07 from the composition brief's F6), input, IME and focus forwarding, accessibility bridged across processes, and lifecycle and crash isolation. | High | High. Turnstone's Scry consumer shows how hard this class is even with a vendor runtime. The [2026-10-05 receipt](../docs/receipts/browser_scry_windows_20261005/README.md) found stale post-input images and blank reopened tiles, and clearing them took a dedicated qualification phase (another session's 2026-10-06 supplier integration receipt, uncommitted when this was written). |
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

### Stage order (2026-10-07)

From U2 (second round), U4, U7 and U8:

1. **S0**, the tested-set repin.
2. **S1**, every text document opens. *Reading, not ruled:* S1 stays ahead
   of SC because a page that doesn't open can't be read, and its Turnstone
   half (two engine registrations and filtered routing) is small enough to
   move with SC.
3. **SC**, one browsing controller in `pelt-core` (U7: before S2 and S8).
4. **S2 and S8 together**, keyboard, focus and page accessibility on
   Windows, macOS and Linux. This is the bar's first requirement (U2).
5. **S3, S4, S5**, then **S6, S7, S9 and S10**.
6. **Misfin, then Gemini over Reticulum**, after the accessibility bar is
   met (U2). Neither has a stage yet.
7. **The session seam (AC2 to AC5; this lane, U15)**: the Mere subtree helper, focus
   handback at a session's edges, panic containment, per-session key
   namespaces, and experiment E1. S8 joins page trees through the helper,
   so the helper and E1a come before S8. *Reading, not ruled:* they run in
   Mere beside SC, and E1b's headed walks come before S8's.
8. **S11** is research only (U8). S11a and S11b are not started; S11c
   became SC.

### S0. Repin as one tested set

**Status (2026-10-07):** closed by the browser session's coordinated repin
(Turnstone `e00869c`); see Progress.

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

**Lane (2026-10-07; U9, U12 to U14).** Inputs:

- **Mere:** the revision Knot's repin lands on (the identity session,
  vault lock ruling 55, "Knot's mere rows to current main"). It must be
  on `origin/main` at or after `ea74604b`, so it carries WS4.
- **Genet:** that Mere revision's pin (`d851a9db` at `ea74604b`).
- **Knot:** the identity session's repin, carrying stack seams S77 (a
  `Block::Menu` wildcard arm and a `..` in knot-editor's desktop preview).
  Knot `main` (`6cb57f1`) pins Mere `e0cea3e0` today.
- **Woodshed:** the browser session's Redshank repin to the same Mere (U13).
  Redshank pins `db4ee312` at Woodshed `06c2b13`; Woodshed's root pins
  `5011e2f9`, outside Turnstone's graph.
- **Turnstone base:** the browser session's pushed commit holding its repin
  and Servo work (U14). S0 starts from it.

Steps:

1. **Linux baseline (cloud).** `cargo check --workspace --locked` on
   Turnstone `origin/main` at today's pins, so Linux breaks that predate the
   repin are known before it.
2. **Wait** for the Knot, Redshank and Turnstone-base commits above.
3. **Repin (cloud).** Branch from the base. Set every Mere row, every Genet
   row, the Knot rows and the Woodshed rows; regenerate the lock with
   `scripts/cargo_mode.py`, never by hand.
4. **Consumer fixes.** Surface the four physics refusals
   (`src/app/mod.rs`, `src/app/session_lifecycle.rs`); score version 5 in,
   a typed refusal for 4; WS4 fallout (the Gopher search `Submit` stays a
   typed refusal until S4); `README.md`'s pins.
5. **Linux gate (cloud),** at `-j 4` under `nice`: `cargo check
   --workspace --all-targets --locked`, the library suite with `--locked`,
   one source per sibling in `cargo tree --locked`, and `cargo_mode.py
   verify`.
6. **Windows gate.** On Mark's yes, push the branch. A session on Mark's
   machine runs the same gate at `-j 4`, BelowNormal, and S0 merges to
   `main` only after it passes.

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

### SC. One browsing controller

**Depends on:** U7 (ruled: before S2 and S8). U11: this lane does the
`pelt-core` half, on a Mere branch with Mere's gates.

`pelt-core` already has what the accessibility stages need from a
controller: page zoom, the session's accessibility projection, click-target
revalidation and accessibility action dispatch (`mere/ports/pelt/core/src/lib.rs`,
lines 253-305 at `ea74604b`). Turnstone's content port has what Pelt lacks
(§3).

**Work:**

- Inventory Turnstone's content port against `PeltController`, piece by
  piece, and record which side each piece comes from. Whether Turnstone's
  Workbench drives one `PeltController` per node tile, or adopts
  `PeltWorkspace`, is settled by the inventory and put to Mark if both are
  viable.
- Move into `pelt-core` what Turnstone has and Pelt lacks: exact request
  identity, Stop and Reload, input and trust conversations, downloads,
  inline images, streaming and per-node lineage.
- Turnstone's content port drives document tiles through `PeltController`
  and keeps no controller copy of its own.

**Done when:**

- Turnstone and Pelt drive document tiles through one controller.
- Turnstone's existing smolweb, NomadNet, capture and recovery tests and
  scenarios pass through it, with the same exact Stop and Reload identities.
- Pelt's scenario suites, including its accessibility receipts
  (`ports/pelt/desktop/workspace_viewer/receipts/a11y.rs`), pass.
- S1's five cases still pass.

#### SC inventory (2026-10-07)

Read-only, at Turnstone `d7ecf33` and Mere `57b4893d`; nothing was built or
run. Turnstone's content port is `ContentStates` (data, `src/content.rs:275`)
plus the live handles on `Shell` (`content_sessions`, `surface_producers`,
`reader_appearances`; `src/shell/mod.rs:519-534`), driven by `Effect`s and
answered by `Update`s. Almost all of it is keyed by graph member (node) Uuid.
Pelt's side is `PeltController` (`ports/pelt/core/src/lib.rs`) and
`PeltWorkspace` (`workspace.rs`); its only consumer is Pelt itself.

| Piece | Turnstone | Pelt | Comes from |
|---|---|---|---|
| Session per document, shared registries | node-keyed map; one `SessionRegistry` and one `SurfaceEngineRegistry` on `Shell` (`shell/mod.rs:93-114`, `:533`) | `PeltController::new_shared` over `Arc` registries (`lib.rs:148`) | Pelt |
| Session identity | none; no generation | `PeltSessionIdentity` (`lib.rs:48`) | Pelt |
| Hidden sessions | never calls `set_hidden`; every live session pumps each frame (`shell/render.rs:761`) | `set_hidden` (`lib.rs:385`) | Pelt |
| Engine routing | routes on every spawn; viewer override; Micron and Knot auto-pinned (`shell/effects.rs:1161-1353`) | fixed `engine_id` per controller; routing only in `PeltWorkspace::install_route` (`workspace.rs:669`) | Turnstone (per-spawn routing) on Pelt's `PeltRouteState`/fallback vocabulary |
| Fetch | async `fetch` actor; host hands the body to the spawn | engines fetch synchronously inside `spawn` through a blocking `ResourceFetcher` (`desktop/workspace_viewer.rs:578`) | Turnstone |
| Exact request identity, Stop | `FetchRequestId` per fetch, `PageFetchPhase`, supersede/cancel (`content.rs:229-388`, `browse.rs:58-425`) | Stop is a documented no-op for documents (`lib.rs:457`) | Turnstone |
| Reload | new request id for the same node, URL, owner URL and Gemini identity (`app/node_arms.rs:517`; test `app/tests.rs:3399` asserts the id changes) | respawns the stored request (`lib.rs:432`) | Turnstone |
| History | graph truth: `navigate_member` and member back/forward in Mere's canvas; a link click mints or selects a *different node* (`canvas.visit`) | in-controller `Vec<SessionSpawnRequest>` (`lib.rs:122`) | Turnstone; Pelt's in-controller history stays Pelt's standalone mode |
| Input, identity and trust conversations | Gemini 10/11, client certificates, TOFU, Titan/Spartan, Micron forms (`browse.rs:240-322`, `node_arms.rs:848-1465`) | none; POST submissions refused (`lib.rs:412`) | Turnstone |
| Downloads | `is_download_response` diverts after the request gate (`download.rs:70`, `app/updates.rs:89`) | none | Turnstone |
| Inline images | `subresources()` / `provide_subresource`, deduped across nodes (`browse.rs:144`, `shell/events.rs:179`) | none | Turnstone |
| Streaming | exact-byte prefix, `replace_body` on a live smolweb session (`content.rs:467`, `effects.rs:1316`) | none | Turnstone |
| Reader lineage | `ExtractionLineageFacts` mirrored to `web.reader-lineage` (`effects.rs:358`) | none | Turnstone |
| Zoom, scroll, links, text targets, frames | session calls at node level | same calls (`lib.rs:253-378`) | either; both call the same session seam |
| Find | `document_find` / `document_find_step`; async on Weld (`effects.rs:1480`) | none | Turnstone |
| Accessibility of a document | spawn-time outline only: no bounds, focus or actions (`a11y.rs:282-315`); never calls the session's projection | `accessibility_projection`, `accessibility_click_target`, `dispatch_accessibility_action` (`lib.rs:283-305`) | Pelt; this is S8's Turnstone half |
| Web surfaces | per-node producers, per-node profile directories (`effects.rs:652-774`) | private `PeltSurfaceController` inside the workspace (`workspace.rs:272`) | Turnstone, unless pelt-core exposes a surface controller |
| Reader appearances | one source session plus per-`SurfaceId` appearance sessions (`shell/mod.rs:978-1019`) | none | Turnstone; stays host-side |
| Tiling | platen `TileLayout` of GraphMemberIds per graph and forme; one node can sit in several workbenches, lenses, a tile pane and the inset, sharing one session; content outlives tile close | cambium `Workbench`/`TileTree` of `TileId`; one controller per tile, inactive tabs included | Turnstone |

**Design (SC1 and SC2, ruled 2026-10-07).** Each side built the opposite
half of a browser. Pelt has the better session half and Turnstone the better
loading half, so the controller takes both. Each host keeps only what makes
it that host.

Turnstone drops its versions in favor of Pelt's:

- **Accessibility.** The session's own projection, revalidated click
  targets and action dispatch replace the spawn-time outline. This is S8's
  Turnstone half.
- **Session identity.** `PeltSessionIdentity` replaces having no
  generation at all.
- **Hidden sessions.** Pelt's `set_hidden` replaces pumping every live
  session every frame.
- **Input.** Neutral `SessionInput`, with pointer capture, focus, text and
  IME, replaces the pointer-down/up click translation.
- **Web surfaces.** Route-state dispatch between document and surface, with
  a stated fallback, replaces `node_uses_web_surface`, which recognizes only
  Weld.
- **Surface polling limits.** `SurfaceResourcePolicy` replaces polling
  every surface every frame.

Pelt takes Turnstone's versions:

- **Loading.** Async loading with request identity, supersede, exact Stop
  and Reload, streaming and inline images. Pelt's engines fetch
  synchronously inside `spawn` today.
- **Typed outcomes:** input required, identity required, certificate
  changed, download.
- **Find, reader lineage and per-node web profiles.**
- **Content lifetime.** Content lives as long as it is on, not as long as a
  tile shows it.

The shape:

1. **One controller for documents and surfaces.** It holds a document
   session or a surface producer behind one command and routing API, and
   re-routes per load.
2. **Host-supplied transport.** The controller owns the load state: request
   ids, a pending state, supersede, Stop, streaming into `replace_body`, and
   subresource delivery. A transport trait does the fetching. Turnstone's
   fetch actor implements it, and Pelt's desktop wraps its present fetchers
   on a worker thread.
3. **History out of the controller.** The controller returns a navigation
   intent with a disposition, and a history policy decides. Pelt ships a
   linear history, today's behavior. Turnstone's policy is the graph: a link
   opens or mints a node and records the lineage edge.
4. **A store seam** (SC2) for client identities, trust, downloads and
   cookies. Pelt gets simple defaults, and Turnstone plugs in personas,
   Muniment and its trust store. The conversations' UI stays in each host.
5. **A controller pool keyed by the host's key, with placements over it.**
   Pelt keys by `TileId`, with `PeltWorkspace` as its placement layer.
   Turnstone keys by node, with platen as its placement layer, so Forme stays
   the arrangement authority. Hidden follows from whether any visible tab
   places a key.
6. **Accessibility per controller,** grafted with `uxtree::graft` and
   namespaced by key and `PeltSessionIdentity`. Both hosts need the Genet
   `AccessKitBridge` fix first, which puts it on SC's critical path.

Turnstone's content port shrinks to:

- mapping nodes to keys;
- the graph history policy;
- the conversations' UI;
- the store implementations;
- Reader appearances, until the pool can hold them as placements of one key;
- capture.

Steps (each keeps both suites green):

1. The transport seam and the load state in `pelt-core`. Pelt's receipts
   pass on its wrapped fetchers.
2. One controller for documents and surfaces.
3. History out to a policy, with linear history as Pelt's default.
4. Turnstone drives one controller per node, with its fetch actor as the
   transport. The gate is the smolweb scenarios, with exact Stop and Reload
   identities.
5. Turnstone's documents take the controller's projection (S8). This needs
   the Genet bridge fix.
6. The pool and placements, with hidden derived from them.
7. The store traits.

*Reading, not ruled:* Reload mints a new request id in the controller, as
Turnstone's tests require, so Pelt's Reload changes with it.

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
- Add a start-view setting (U6) with three values: the home pane (default),
  a home address (a capsule or a local `.gmi` file), or the canvas.
- Build the home pane from graph truth, as an accessible list-shaped pane:
  - kept capsules;
  - subscriptions with unread counts;
  - the recent trail;
  - a search field bound to a configured search capsule.

**Done when:**

- Every moved setting changes live and persists, and the environment only
  overrides.
- First launch on a fresh profile shows the home pane.
- Switching the start view to a home address, then to the canvas, takes
  effect on the next start and persists.
- The home pane's entries are reachable and named by keyboard and screen
  reader (an `assert a11y` scenario lists them).
- Non-address text offers "search with <capsule>" in the omnibar.

### S7. Offline and revisits

**Depends on:** U10 (ruled: memory plus capture); the page capture plan's
offline replay.

**Work:**

- Hold the last body of each smolweb address visited in this run, in a
  bounded in-memory store. Nothing from it reaches disk, so page lifecycle
  L1 and the shallows hold.
- Back and Forward show the held body, then refresh by a configurable
  policy.
- A visit with the network down shows the held body, marked stale.
- Across restarts, offline reading comes from capture: a captured page
  opens from its latest capture through the capture plan's offline replay.

**Done when:**

- With the network disabled, Gemini, Gopher and NomadNet pages visited
  earlier in the same run open and read, marked stale.
- After a restart with the network disabled, captured pages open from their
  captures, and unkept pages say they need the network.
- No body of an unkept page is written to disk (a test inspects the
  profile).
- The memory store stays within its configured bound.

### S8. Accessibility of pages

**Runs with S2, after SC** (U2, U7).

**Mere half (this lane, U16):**

- Smolweb and Micron sessions implement `accessibility_projection`
  (headings, paragraphs, links and fields), as the Reader session already
  does (`document-lanes/src/reader.rs:608` at `ea74604b`; it is the only
  document-lanes session that does).

**Turnstone half:**

- Content-port tiles publish the session's projection, through SC's
  controller, into Turnstone's AccessKit tree as a subtree through the Mere
  helper (AC2), in place of the `Partial` outline. Turnstone's path-hash
  stitching for contributed panes (`src/contributed_a11y.rs`) moves onto
  the same helper. Today Turnstone publishes a `DocumentA11yProjection` only for
  contributed surfaces (`src/contributed_surface.rs:384`, `src/ui.rs:1659`).
- Accessibility actions on a link or field route to the owning session.

**Done when:**

- An `assert a11y` scenario lists link names in document order for a Gemini
  page, a Gopher menu and a NomadNet page.
- On each of Windows (Narrator or NVDA), macOS (VoiceOver) and Linux (Orca),
  a person reads a Gemini page, a Gopher menu and a NomadNet page, follows a
  link and submits a field by keyboard, and a dated receipt records it.

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
- **S11c, Pelt convergence (per U7).** Moved to SC on 2026-10-07, ahead of
  S2 and S8.

### Browser sidecar scope (2026-10-07; proposal)

**Status:** scope proposal from the site/composition review. No browser code
changed, no browser target was built, and these delivery cuts are not new
rulings or additions to the stage order. SC, the session seam, E1 and S8 stay
with their existing owner under U15 and U16; accessibility keeps U2's priority.

**User intent:** the host browser supplies the fullweb lane. Turnstone supplies
the unusual-protocol reading and browsing experience, either attached to the
person's native Turnstone session or owning a browser-local session that can
be kept and exported without installing a native application. An extension
adds host-browser integration; installation as a PWA is not required to own a
local session.

#### Existing decisions and the boundary to reconcile

The June extension/companion plan already describes rendering in the browser
and raw-socket fetch outside it. Its delivery phases and product framing were
superseded, first by the capture-first browser lane and then by the Graphshell
reference-host plan. Those historical plans now live at:

- `mere/design_docs/archive_docs/2026-10-06_superseded_plans/2026-06-23_browser_extension_companion_plan.md`
- `mere/design_docs/archive_docs/2026-10-06_superseded_plans/2026-06-24_orrery_browser_lane_plan.md`

The current reference-host plan (§§1, 3 and 6) assigns the browser portal and
consented browser-wide capture to Graphshell, and browsing/page lifecycle to
Turnstone. Proposal: Turnstone's browser experience consumes that portal and
the shared browsing controller rather than duplicating either. This does not
rename Graphshell, reverse its product ruling, or claim that Turnstone's native
binary is already WASM-portable. The product-facing entry point and packaging
still need reconciliation between the two plans before implementation.

#### Two independent choices

Session authority and browser wrapper are independent:

| Capability | Browser-local session | Attached native session |
|---|---|---|
| State and actions | Local Mere session through its browser store; Turnstone owns product actions. | Native owner accepts admitted typed intents; the browser holds a disclosed projection and presentation state. |
| Unusual-protocol reading | Portable presentation of imported or fetched bodies. A raw-protocol fetch needs a separately granted companion or gateway. | The owner fetches through its admitted product endpoint and discloses the result; attachment alone grants no arbitrary fetch. |
| Session persistence | IndexedDB through Muniment, with the browser's persistence answer shown; export remains a separate action. | Native persistence remains authoritative; a browser cache never becomes a competing session. |
| Identity | Report the local subject and available credentials honestly; a local browser session is not automatically admitted to a native owner. | Reuse Notochord admission and advertised action scopes; native vault secrets stay native. |
| Disconnect | Imported, retained and available local material remains usable; unavailable fetch actions explain the missing provider. | Keep only explicitly disclosed/cacheable material and local presentation. Refuse unavailable owner actions; reconnect revalidates the grant and revision. |

| Integration | Ordinary web page / PWA | Extension |
|---|---|---|
| Fullweb | Ask the host browser to open an ordinary address. Do not infer that it loaded or that its contents were captured. | The host-tab handler can use extension APIs; report only lifecycle facts actually observed. |
| Host browsing intake | Explicit import or another separately supported user action; no browser-wide tab/history access. | Optional, consented intake under the existing capture policy. An ordinary visit record is not a page-body capture. |
| Native connection | Admitted browser carrier, currently WebRTC; no WebExtensions native-messaging API. | The existing registered native-messaging bridge, or the same browser carrier for a remote owner. |

These wrapper boundaries follow Graphshell's reference-host plan §6 and its
implemented extension (`mere/ports/graphshell/web/extension/README.md`). Native
messaging requires an installed native application and extension permission
([MDN](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging)).
Neither profile imports host-browser cookies or another site's authenticated
state into the Mere session by implication. Supported HTTP fetches remain
subject to the host browser's access rules. The companion/gateway is an
explicit provider, with its identity, reachability, trust decision and granted
protocol actions visible; the June raw-socket split is not a public gateway
deployment decision.

#### Shared seams and portable artifacts

- Consume SC's controller and store seam when qualified; assess their actual
  WASM dependency cone first. Do not recreate Turnstone's content port in
  JavaScript. Use the portable document presentation and Cambium browser
  accessibility path; host-browser fullweb stays in host tabs.
- Reuse Graphshell's client/session protocol for snapshots, diffs, resources
  and typed intents. Reuse the WebRTC carrier plan's C4 proof and keep C5 public
  rendezvous as its own deployment gate. A successful connection does not
  establish page fidelity or Turnstone product acceptance.
- Reuse the browser storage and product export/import contracts, assessing the
  selected build profile. The site's plain viewer intentionally excludes
  product persistence and remote sessions; it is not the browser-app profile
  (`mere/ports/graphshell/web/Cargo.toml`).
- A Shelfmark cites a scene against named authorities and generations; it is
  not a session backup. A graph codicil is an immutable, scoped graph artifact;
  editing a thaw creates a live session. Export must name scope, omitted
  resources/private attachments and its import semantics. Do not promise a
  complete native session, credentials or every page body from a scene link.
  Homes: `mere/design_docs/mere_docs/technical_architecture/2026-08-16_shelfmark_format_note.md`,
  `mere/crates/system/pandect/src/graph_codicil.rs`, and
  `mere/design_docs/mere_docs/implementation_strategy/2026-09-23_reservoir_plan.md`.
- Preserve this repository's [page lifecycle](2026-09-06_page_lifecycle_plan.md):
  visits record addresses, Keep retains the node, and explicit Capture retains
  admitted bytes. Do not turn opening a host tab into an implicit capture.

#### Bounded delivery proposals and acceptance

1. **Capability inventory first.** Name the exact app feature profile,
   controller/presentation dependency cone and supported protocols. Publish
   available, unavailable and provider-dependent actions. A WASM compile proves
   the dependency boundary only; it does not prove browser usability.
2. **Local reading and recovery.** Open user-selected unusual-protocol fixtures,
   follow supported links, navigate Back/Forward, Stop and Reload, and exercise
   input with exact request identities. Keyboard and screen-reader actions use
   the same actions as pointer input. Reopen the same origin's local session;
   distinguish granted, refused and unknown persistence, and test export/import
   into an empty store, including invalid input and declared omissions. Clearing
   storage and a changed hosting origin cannot be presented as successful
   recovery. `navigator.storage.persist()` is a request, not a guarantee against
   user deletion ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/StorageManager/persist)).
3. **Attached browsing.** Join one native Turnstone session through admitted
   existing seams, show its granted browsing projection, send a supported
   owner action, and request a fullweb address in the host browser. Prove denial,
   stale revision, disconnect/reconnect and revocation; a refusal changes no
   native state. Private keys and vault material never reach the browser.
4. **Extension integration.** Reuse the existing permission/capture model and
   bridge. Prove default-off intake, refusal and revocation, bounded acknowledged
   intake, and explicit separation of visit metadata from captured page bodies.

These are proposed browser cuts, not authorisation to duplicate the native
lane or deploy a gateway/public rendezvous. Browser acceptance needs headed
interaction and browser assistive-technology receipts; native AccessKit or
Graphshell carrier receipts alone do not close it. Offline support must name
which application assets and retained bodies are actually available rather
than promising fresh raw-protocol browsing without a reachable provider.

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

**SC1. Which controller shape does Turnstone adopt?** (From the SC
inventory, 2026-10-07.)

- **(a)** One `PeltController` per node, in Turnstone's node-keyed map. A
  node shown in several places shares one controller. Pelt keeps
  `PeltWorkspace` for its standalone tiling.
- (b) One `PeltController` per tile, as SC's text first said. A node tiled
  twice gets two sessions and two histories.
- (c) Adopt `PeltWorkspace`. This needs Turnstone's arrangement to move onto
  cambium's `TileTree`, and history onto per-tile controllers, giving up
  graph-truth navigation.

**SC2. How much of the fetch side moves into `pelt-core`?**

- **(a)** The load state machine: request identity, Stop, supersede,
  streaming, and typed outcomes (input, identity, certificate change,
  download). The conversations' UI and stores stay in the host.
- (b) All of SC's list, including the conversations' UI and a store seam
  for personas, trust and downloads.
- (c) Only what accessibility needs (projection, actions, identity). Fetch
  stays in Turnstone, and the duplication is ruled acceptable for now.

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

### Rulings (2026-10-06, second round)

- **U2, confirmed and reshaped.** Asked to confirm the reading above (Misfin
  through mere's comms work; Gemini over Reticulum; NomadNet usable with a
  Linux screen reader), Mark: **"oh. i would hope we could meet the
  accessibility bar on all platforms. i would prioritize that over pretty much
  anything, because that's the difference between a browser that works and one
  that doesn't for real folks, no matter the protocol. misfin and
  gemini/reticulum (and probably other stuff tbh) should come after that."**
  *Follows:*
  - **Accessibility on all three operating systems is the bar's first
    requirement**, for every protocol's pages: AccessKit's adapters on
    Windows (UIA), macOS (NSAccessibility) and Linux (AT-SPI), checked with
    Narrator or NVDA, VoiceOver and Orca.
  - S8 is no longer a late stage. It leads, together with S2's keyboard and
    focus path, which a screen reader needs.
  - Misfin (through mere's comms and errand work) and Gemini over Reticulum
    come after the accessibility bar is met, as does "probably other
    stuff".
  - *Reading, not ruled:* this outranks U4's "Reading correctness first"
    where the two compete. S1 (text documents open) still comes first,
    because a page that doesn't open can't be read by anyone.
- **U3, the mere halves of S1 to S4.** Mark: **"Mere's smolweb fidelity lane
  (Recommended)"**: mere's smolweb fidelity plan owns them, with Turnstone as
  the forcing consumer and one dated note in each plan.
- **U5, NomadNet's interface and identity.** Mark: **"In Turnstone for now
  (Recommended)"**: settings and a persona-derived identity in Turnstone,
  shaped to move into a device-resident Reticulum service later.
- **U9, repin timing.** Mark: **"S0 once WS4 is pushed (Recommended)"**: a
  tested set (Knot, then Redshank, then Turnstone), coordinated with the
  browser session that owns the dirty tree.

### Rulings (2026-10-07, third round)

- **U6, what Turnstone shows at start.** Options: the start view is a
  setting (home pane by default, or a home address, or the canvas); a home
  address only; no home. Mark: **"Start view is a setting (Recommended)"**.
  *Follows:* S6 adds the start-view setting with the home pane as its
  default. The pane is list-shaped, which also makes it the easiest first
  surface for a screen reader.
- **U7, Pelt and Turnstone's browsing loop.** Asked with the finding that
  `pelt-core`'s controller already has page zoom, the document
  accessibility projection and accessibility action dispatch, while
  Turnstone's is ahead on request identity, Stop and Reload, prompts,
  downloads, inline images, streaming and lineage. Options: converge after
  S4; converge first, before S2 and S8, so keyboard and accessibility code
  is written once; Turnstone's content port owns browsing and Pelt stays a
  reference viewer. Mark: **"Converge first"**. *Follows:* stage SC, between
  S1 and S2. Who does its `pelt-core` half is U11.
- **U10, what keeps a visited page's body.** The plan's options (a) and (b)
  collided with page lifecycle rulings: L1 ("History remembers where you
  went, never what you saw"; content exists only past Keep), L2 (a capture
  is a Keep, so "every visit is a capture" would keep every visit) and L3
  (Keep is a bookmark, with no bytes). They were reframed before asking:
  memory for this run plus capture; kept pages also keep their last body
  (reverses L3); a durable cache of every visit as a setting, off by
  default (reverses L1 while on); no offline reading. Mark: **"Memory +
  capture (Recommended)"**. *Follows:* S7 holds bodies in memory for the
  running process only, and offline reading across restarts comes from
  capture. L1 to L3 stand.

### Rulings (2026-10-07, fourth round)

- **U11, who does SC's `pelt-core` half.** `pelt-core` lives in Mere, and
  U3 covered only S1 to S4. Options: this Turnstone lane, on a Mere branch
  with Mere's gates; Mere's smolweb fidelity lane; a new Pelt lane in Mere.
  Mark: **"This Turnstone lane (Recommended)"**. *Follows:* this lane needs
  push access to Mere, or a local session makes the Mere commits.
- **U12, where S0 builds.** Options: the cloud session repins and gates on
  Linux, then a session on Mark's machine runs the Windows gate; all on
  Mark's machine; cloud only. Mark: **"Cloud first, Windows gate
  (Recommended)"**. *Follows:* the Linux run is also U1's first Linux
  evidence. The cloud container has 4 cores, 15 GB of memory and about
  30 GB of disk, which may not hold a full test build.
- **U13, who repins Redshank.** Options: the browser session, which moved
  Redshank last; this lane; Woodshed's own lane. Mark: **"The browser
  session (Recommended)"**.
- **U14, the browser session's uncommitted Turnstone repin.** Options: it
  commits and pushes its repin and Servo work first, and S0 starts from
  that head; S0 absorbs the repin rows; the browser session carries S0.
  Mark: **"It commits first (Recommended)"**.

### Rulings (2026-10-07, fifth round: the composition brief)

The U8 research lane's brief is Mere
`design_docs/cambium_docs/research/2026-10-06_app_composition_brief.md`
(Mere `5919dc64`). Its forks AC1 to AC6 were put to Mark as written, with
notes on how AC2 and AC3 meet SC and S8. The brief's own §9 should carry
the same record once this lane can push to Mere (U15).

- **AC1, how one Cambium app appears inside another.** Options: same
  process, each guest a retained session with its own document, with
  authority that must stay in its own process behind a Graphshell session;
  mounts in the host window's forest document; live pixels from another
  process. Mark: **"1, but there is a utility to 2 that has yet to be
  articulated. for now, 1 is good"**. *Follows:* retained sessions are the
  mechanism. Forest mounts keep a use Mark has yet to articulate, which
  AC5's reservation keeps open.
- **AC2, joining accessibility trees.** Options: AccessKit subtrees through
  one Mere helper every host uses; Turnstone's path-hash stitching moved into
  Mere unchanged; each host on its own. Mark: **"One mere subtree helper
  (Recommended)"**. *Follows:* S8 and Pelt's combined tree (through SC) use
  the helper.
- **AC3, experiment E1.** Options: E1a (windowless tests in Mere) and E1b
  (headed walks with Narrator or NVDA, VoiceOver and Orca on Fedora and
  Mint); E1a only, with the headed walks inside S8; not now. Mark: **"E1a
  and E1b (Recommended)"**.
- **AC4, a guest's failure.** Options: catch unwinding panics at every call
  into a guest session and retire it; a separate process for any guest
  built outside the host's repository; no containment. Mark: **"Catch panics
  (Recommended)"**. Turnstone's profiles do not set `panic = "abort"`, so
  catching works.
- **AC5, room for forest mounts.** Options: reserve room now with three
  additive changes (a public `build_at` form, a stylesheet scope class,
  per-session key namespaces) and build the mount when a consumer needs it;
  build it now; nothing until needed. Mark: **"Reserve room now
  (Recommended)"**.
- **AC6, a future cross-process transport.** Options: paint lists plus
  shipped AccessKit trees, research only after AC1's path is proven; shared
  GPU textures plus AccessKit trees; session projections only. Mark:
  **"Paint lists + a11y trees (Recommended)"**.

- **U15, who builds the session-seam work in Mere** (AC2's subtree helper,
  focus handback, AC4's panic containment, key namespaces, AC5's
  reservations and E1). Options: this lane, which already works in Mere for
  SC (U11) and whose S8 waits on the helper; the stack seams owner (the
  local "Graph database concept for Mere" session); a new composition lane.
  Mark: **"This lane (Recommended)"**. *Follows:* this lane owns SC, the
  session seam and S8, and needs Mere attached with push access (attached
  2026-10-07).
- **U16, who does S8's Mere half** (smolweb and Micron sessions in
  document-lanes implementing `accessibility_projection`; U3 covered S1 to S4
  only). Options: this lane, which owns the accessibility path end to end;
  Mere's smolweb fidelity lane, which owns document-lanes' smolweb sessions.
  Mark: **"This lane (Recommended)"**. *Follows:* this lane owns SC, the
  session seam, E1 and both halves of S8.
- **U17, keeping composed sessions' leaf keys apart.** Asked after finding
  that no contributed session hands leaves to a host yet, and that Woodshed's
  fretboard uses fixed keys. Options: per-session registries (each session
  owns its leaf and producer registries through the session seam; keys stay
  as written); namespaced keys in one host registry (AC5's wording, with key
  translation on every paint and accessibility lookup); both. Mark:
  **"Per-session registries (Recommended)"**. *Follows:* this refines AC5's
  "keys carry a per-session namespace" for retained sessions; a namespace
  returns only if forest mounts come to share one registry. Producers stay
  host-side for now, because `ProducerRegistry` sits in `cambium-rootstock`
  above the seam and holds the host's GPU device.

### Rulings (2026-10-07, sixth round: SC)

- **SC2, how much of the fetch side moves into `pelt-core`.** Options: the
  load state machine with the conversations' UI and stores host-side; all of
  SC's list, including that UI and a store seam; accessibility only. Mark:
  **"the conversations ui doesn't have to come, but a store seam seems
  prudent"**. *Follows:* the load state machine and a store seam for
  identities, trust, downloads and cookies move to `pelt-core`. Each host
  keeps its own conversations' UI.
- **SC1, the controller shape.** Options: one `PeltController` per node;
  one per tile; adopt `PeltWorkspace`. Mark asked to "compare 1 and 3", then
  reframed: "what should pelt take from turnstone, to replace the parts of
  turnstone that are worse versions than what pelt brings? How do we
  integrate pelt in a manner that simplifies and improves the architecture,
  bringing it closer to a usable browser?" On the integrated design (SC
  section), Mark: **"agreed with 1. I am open to further changes,
  augmentations, and refactoring"**. *Follows:* one controller per node in
  Turnstone, inside the integrated design. The pool and placements converge
  Pelt and Turnstone on one controller without either owning the other's
  arrangement. Further refactoring is in scope where it simplifies.

## Progress

- **2026-10-06:**
  - Assessment written by a read-only lane.
  - Findings were read against Turnstone `a616ece`, Mere `3d1cdacc`,
    `392630bb` and `b4f14f2c`, Genet `69a2383b`, Knot `92719898`, `ef89a186`
    and `33cc855`, and Woodshed `9e982b88`.
  - No code changed, and nothing was built or run.
- **2026-10-07:**
  - A cloud session took the plan over from the physics coordinator
    session. The four local plan commits were not on origin, so the text
    came in by paste.
  - WS4 is on Mere `origin/main`: `b4f14f2c` (R0 to R5), recorded by
    `4e57913e`. The head, `ea74604b`, opens stack seams S76: publish
    `gopher-protocol` 0.2.0, then repin. U9's trigger has fired. S0 waits
    on the identity session's Knot repin.
  - Read at Mere `ea74604b`: Pelt's desktop viewer owns one combined
    AccessKit tree. Each focused document lane contributes a namespaced
    child subtree, and typed actions route to one tile
    (`ports/pelt/desktop/workspace_viewer/accessibility.rs`, 1,215 lines;
    receipts in `.../receipts/a11y.rs`, 1,131 lines). Only the Reader
    session implements `accessibility_projection`, so for small-web pages
    both hosts wait on S8's Mere half.
  - U6, U7 and U10 ruled (third round); U11 to U14 ruled (fourth round).
  - Mark relayed the physics session's answers. WS4 and the stack seams
    rounds belong to the local "Graph database concept for Mere" session
    (rulings S66 and S70 to S75; R6 still on the smolweb branch; S77 is the
    Knot pattern fix at Knot's next repin). The dirty browser-supplier tree
    in Turnstone is most likely a Codex agent, not a Claude session: 16
    files, about 8,800 lines added and 4,500 removed, across `weld.rs`,
    `effects.rs`, `surface_frames.rs`, the manifests and the lock. U13 and
    U14's "browser session" is that agent; Mark relays to it.
  - Mark pushed the plan's four commits; Turnstone `origin/main` is
    `032463a`. This lane's ruling commits were moved onto it.
  - The composition brief landed (Mere `5919dc64`); AC1 to AC6, U15 and
    U16 ruled. Mere `e1bd641` records the AC rulings in the brief's §9 and
    gives the smolweb fidelity plan its dated U3 note. §4's live-pixel row corrected from its F6.
  - S0 step 1, Linux baseline: the first `cargo check --workspace --locked`
    stopped at `alsa-sys` because the container lacked ALSA's development
    headers. That is an environment gap, not a code break. With
    `libasound2-dev` and the usual X11, Wayland, xkbcommon, udev,
    fontconfig, OpenSSL and D-Bus development packages installed, the
    rerun passed: `cargo check --workspace --locked` at Turnstone
    `032463a`'s pins (Mere `3d1cdacc`) finished clean on Linux with 82
    warnings in `turnstone`'s lib. This is Turnstone's first Linux compile
    evidence (U1); tests were not run.
  - The session seam (U15) and E1a, on Mere branch `turnstone-session-seam`
    from `fde06dc`:
    - `uxtree::graft`: AC2's subtree helper. It checks a frame against the
      rules the AccessKit consumer enforces by panicking, routes actions by
      tree, and orders updates so focus entering a guest is one move.
    - Cambium focus exits: Tab leaves a session at its edges through
      `SurfaceEffect::FocusExit`, opt-in per session.
    - `ContainedSession` (AC4): a guest's panic retires it as
      `Unavailable(Unhealthy)`.
    - Per-session leaf registries (U17) and a public `OwnedLayout::new`.
    - E1a passes windowless against `accesskit_consumer` 0.35, 0.36 and 0.38:
      27 tree-half checks and 18 session-half checks, with the controls
      failing as they must. `uxtree` (19 unit tests) and `cambium`'s library
      (249) pass; clippy is clean on the changed files.
    - Findings: two focus-ordering gaps in the first `Grafts`, each caught by
      a test before the fix; in-process Genet sessions never share AccessKit
      ids (the brief's reading corrected); the seam cannot name
      `ProducerRegistry`.
    - E1b, the headed walks on Windows, macOS and Linux, is next. SC and S8
      build on this seam.
    - Merged to Mere `main` as `a59e4c47` on Mark's word, over 15 newer
      `main` commits with no conflict. Gates on the merged tree, `--locked`:
      `uxtree` 19 and E1a tree half 27, `cambium` library 249,
      `cambium-winit-a11y` 3 and E1a session half 18. The S45 license-header
      check fails on `crates/system/framing/src/tests.rs` (no MPL Exhibit A),
      already on `main` before the merge and outside this lane; the new files
      pass it.
  - Next in this lane, ruled by Mark: the E1b probe for the headed walks, and
    S8's Mere half (smolweb and Micron accessibility projections). SC's
    inventory follows once S8's shape is set.
  - S8's Mere half landed on Mere `main` (`f2ad105a`, `35bd7d77`):
    `SmolwebDocumentSession` publishes an accessibility projection from the
    engine document and the retained document-canvas packet. Blocks carry
    their roles and laid-out boxes; a block's text runs and links come out
    as children in reading order, so no link is read twice. Links,
    submissions and collapsible headings take Click (a revalidated point for
    the host's pointer path), Focus and ScrollIntoView. Menus read one row per
    item with its kind and its own line's box. Bounds are unclipped
    viewport coordinates, and the revision moves with layout, scroll, size
    and focus. It is declared Partial: text runs carry their block's box, and
    preformatted blocks are one node.
    - Micron pages take the same projection: Turnstone's
      `MicronSessionEngine` spawns `SmolwebDocumentSession`. Two NomadNet
      1.4.2 guide probes read with headings and working links.
    - 41 lane tests pass. `smolweb_streaming_render` needs a GPU adapter
      and fails the same way without the change.
    - Findings for S8's Turnstone half and SC: Micron fields render as inert
      source in the page, and the live form is Turnstone's own form editor,
      so its accessibility is Turnstone's. `SmolwebDocument::focus_move`
      wraps at the ends, as Cambium runners did, so a composed smolweb tile
      needs the same edge exit.
  - U14 is met: the browser session's work reached Turnstone `main` (`2b84e68`,
    `4e217ef`; merged into this branch as `cb78d44`). Its pins are Mere
    `edf175f9`, Genet `679d8314`, Knot `211ff57a` and Woodshed `24f196f4`.
    Mere `edf175f9` sits on Mere branch `codex/browser-input-compat`, not on
    Mere `main`, so S0's first done-condition (one Mere revision on
    `origin/main`) needs that branch merged to Mere `main` first, or S0
    carries its fixes. S0 starts from `cb78d44` once Knot's repin lands.
  - The E1b probe is on Mere `main` (`d041cc69`):
    `cargo run -p cambium-winit-a11y --example e1b_two_sessions`. It has a
    host menu and two `ContainedSession` panes joined by `uxtree::graft`,
    published through AccessKit's own winit adapter. `--omit-a` and
    `--unboxed` are the controls; `--self-check` runs it without a window. The
    windowless self-check passes on Linux. The headed walks with Narrator or
    NVDA, VoiceOver and Orca are Mark's.
    - Finding for S8's Turnstone half: `genet_winit_host::AccessKitBridge`
      drops an action's `target_tree` and answers activation with the latest
      update, a guest's under subtrees. Turnstone and Pelt need that bridge
      changed in Genet before their trees can graft.
  - Asked the identity session to report Knot's repin (commit, Mere and
    Genet revisions, S77). Asked the physics session who owns WS4 and the
    stack seams rounds (neither the fidelity plan nor Mere's log names a
    session) and which session holds Turnstone's dirty browser tree.
  - Nothing was built or run.
  - A local session took the lane over from the cloud session.
  - S0 is closed, by the browser session's coordinated repin rather than this
    lane's steps 2 to 6. Turnstone `abb349cf` (runtime) and `8d1f907f`
    (test-only follow-up) pin Mere `57b4893d`, Genet `965b64e2`, Knot
    `0096591a` and Redshank `82271df2`; `e00869c` records the gates. Windows
    passes 681 library tests and Linux 650 (nine ignored on each), with
    `cargo_mode.py verify` and one source per sibling. The receipt is
    `docs/receipts/browser_family_20261007/final-s0-audit.json`.
    - Mere `57b4893d` is on `main` and carries the session seam
      (`a59e4c47`), S8's Mere half (`f2ad105a`, `35bd7d77`), the E1b probe
      (`d041cc69`) and the side-branch fixes (`edf175f9`), so the side-branch
      blocker is gone.
    - Knot `0096591a` was published by the browser session, not the identity
      session. The browser plan records that the identity session's
      seed/vault repair is not yet a published baseline.
  - Stale branches already merged to `main` were pruned from GitHub,
    including Mere's `turnstone-session-seam`.
  - SC inventory written (SC section). Turnstone's content port is keyed by
    node and its history is graph truth, so `PeltWorkspace` does not fit.
    The recommended shape is one `PeltController` per node, which needs a
    host-fetched load mode and host-owned navigation in `pelt-core`. The
    largest gain is S8's: Turnstone's documents expose only a spawn-time
    outline today, while the controller carries the session's full
    projection and actions. Asked Mark SC1 and SC2.
  - SC1 and SC2 ruled (§7, sixth round). The SC section now holds the
    integrated design and its seven steps. Step 1 (the transport seam and
    load state in `pelt-core`) starts on a Mere branch.
  - SC step 1 is on Mere `main` (branch `pelt-host-load`, `35e61050` and
    `eca623bc`, merged as `1360d771`):
    - The new zero-dependency crate `page-load` (`mere-page-load`) holds the
      fetch vocabulary, which `mere-fetch` now re-exports unchanged, plus a
      sans-IO `PageLoad`. That is `ContentStates`' fetch half per document,
      with the clock passed in, and Turnstone's download classifier.
    - It is sans-IO because Turnstone fetches pages for graph truth even
      when content is off, and keeps fetch state in its pure App model.
      Turnstone's App can own `PageLoad`s directly, and Pelt's controller
      composes one.
    - `pelt-core` gains a host-loading mode: Fetch and Cancel commands out;
      progress and outcomes in, gated on their exact request; held bodies
      only; history committed on the first answer; a new request on Reload;
      Stop cancelling the exact request and keeping the page; an optional
      in-place body replacer for streaming; conversations typed in
      `PeltDocumentState::Awaiting`; downloads in the host effect. Engine
      loading is unchanged. Pelt desktop presents the two new states.
    - Tests: `page-load` 10, `mere-fetch` 20, and `pelt-core` 20, including
      9 for host loading, all pass on macOS. `pelt-desktop` and `pelt` pass
      `cargo check --all-targets`.
    - `pelt-desktop --lib` passes 57 of 57 after a separate fix
      (`06246afe`, merged as `7e5ca10c`): six Livery receipts built their
      fixture paths with Windows separators and failed on macOS and Linux.
    - `check_port_boundaries.py` fails on `main` already: `crates/mere`
      depends on the `tabard` port.
    - Deferred: `PeltWorkspace` passthroughs for host loading, and Pelt
      desktop's switch to the `mere-fetch` actor. A trait-level
      `replace_body` in Genet would retire the downcast replacer.
  - SC step 2 is on Mere `main` (`831b5a82`, merged as `938c64be`).
    `pelt-core` gains `PeltContent`, one routed piece of browsing content: a
    document lane (a `PeltController`) or a surface lane (a live web-engine
    producer), behind one command, input, frame and routing API.
    - Routing moves into `PeltRegistries::choose` and runs for every new load.
      A document lane changes engine by address and by the response's media
      type. History reopens each entry with the engine it was shown with.
    - A load that routes to a surface swaps lanes, and a surface asked for a
      document address swaps back. A failed swap keeps the current lane.
      History does not cross lanes.
    - Content can carry its own surface profile, for Turnstone's per-node
      web profiles. Stop, Reload, Back and Forward reach a surface's web
      plane whichever engine it is, which is the structural fix for
      `node_uses_web_surface` recognizing only Weld.
    - `PeltWorkspace` is now a map of `PeltContent`, with its public API
      unchanged.
    - Naming: the per-node object Turnstone will hold is `PeltContent`. It
      wraps a `PeltController` on a document lane.
    - Tests on macOS: `pelt-core` 25 (5 new for content), all pass, and the
      routing and workspace tests pass unchanged. `pelt-desktop --lib`
      passes 57 of 57. Pelt desktop and `pelt` pass `cargo check
      --all-targets --locked`.
  - Request for the next coordinated Mere repin, from the Scenograph editor
    lane (Mark's SE35 assigns it to this lane). The brief is §3 of
    `mere/design_docs/mere_docs/implementation_strategy/2026-10-07_scenograph_editor_plan.md`.
    Mere main at `16c1ef8d` and later offers `cambium::CommandSet`, the
    pictograph context gestures (`take_context_request`) and pandect's
    `CommandMenuView`. Turnstone's side:
    - `available_actions` goes through the command set.
    - The `>` lane gains kept and recent commands, with keep and drop.
    - `shell/input.rs` opens the palette on the right release that
      `take_context_request` reports, not on the press.
  - Knot handoff from the identity session, relayed by Mark, for the next
    coordinated repin:
    - Knot `eabd443` lends the signing seed rather than copying it (vault
      lock ruling 49). It sits on Knot `14cd06e`'s repin, which Turnstone
      already pins (every Mere row at `f1d169c7`, Genet `965b64e`, S77
      included).
    - Mere `93880980` pins Knot `eabd4434` in its workspace and djinn.
    - The old `f68af0d` / `ea74604b` / `d851a9db` set was never pushed and
      is retired.
    - The only public signature change is that `signing_seed()` now returns
      `&[u8; 32]`. Turnstone never calls it: its Knot use (write grants,
      endpoints, effect authority, publish and share-read types) is
      unchanged, so moving to `eabd443` or later needs no source change.
    - Exposure under ruling 60: Mere's mDNS-on transport leaves four copies
      of the seed. Turnstone turns mDNS on explicitly in
      `src/publish_service.rs` and `src/share_reader_service.rs`
      (`MdnsDiscoveryMode::Active`). The fix is the identity session's next
      item, in mere-transport. Turnstone takes it at the repin after it lands.
  - SC step 3 is on Mere `main` (`e536d2a3`, merged as `c32e4c03`). A
    controller's history now has two modes:
    - Linear, the default and unchanged: links push, Back and Forward
      traverse.
    - Host (`with_host_history`, or `PeltRegistries::with_host_history` for
      routed content): the controller keeps only its current entry. A link
      or GET form returns a `PeltNavigationRequest`, with its cause and the
      modifiers held, and nothing loads. Document Back and Forward return
      unhandled, and the host's Address command opens in place.
    - `open(request)` replaces the current entry and routes like any new
      load. A held body opens directly even under host loading, cancelling
      any fetch in flight. `PeltContent::open` does the same in either lane.
      Web surfaces keep their own engine history.
    - This is Turnstone's seam for step 4. Its App keeps the graph history
      (`canvas.visit` opens or mints a node; member back and forward) and
      the per-node `PageLoad`. Its Shell turns each decision into `open`,
      with the fetched body held, on that node's `PeltContent`.
    - Tests on macOS: `pelt-core` 30, including 4 for host history and 1
      for host-history content, all pass, and Linear-mode tests pass
      unchanged. `pelt-desktop --lib` passes 57 of 57, and Pelt desktop and
      `pelt` pass `cargo check --all-targets`.
- **2026-10-08:**
  - SC step 4, pass A (documents), with the coordinated family repin that it
    needs. Mark ruled that this lane does the published repin, and that step 4
    adopts documents first, leaving web surfaces (pass B) to be coordinated
    with the browser lane's live work in `src/shell`.
    - Mere `3ded2cd7` adds the controller's session seam (`session()`,
      `session_mut()`), so features the controller doesn't wrap yet (find,
      subresources, Turnstone's own pointer path) reach the live session.
    - Family, in U9 order: Knot `6e66f1ab` and Redshank/Woodshed `3ef71040`,
      both on Mere `3ded2cd7` with Genet `965b64e2` unchanged, then Turnstone.
      Each lock adds only `edit-history` and `mere-page-load` (plus
      `pelt-core` in Turnstone). `cargo tree --locked` shows one source per
      sibling.
    - App: `FetchedDocument` and `PageFetchPhase` are now page-load's types,
      and `ContentStates` delegates to one `PageLoad` per node, so the load
      state machine exists once, in Mere.
    - Shell: each node's document is a `PeltContent` around a host-history
      controller, host-loading when the App holds the body. Routing and the
      spawn-time facts are unchanged. Existing call sites reach the session
      through the seam, so behavior is unchanged.
    - Gates, macOS (Turnstone's first macOS run):
      - Knot: the workspace check passes; `knot-editor` and `knot-desktop`
        pass 525 tests in 31 suites; `knot-document` passes 45.
      - Redshank: desktop and wasm checks pass; the port suite passes 193 of
        201 (7 ignored as before). The one failure loads a fixture image
        from a fixed Windows path. The IPC consumer test passes 3.
      - Turnstone: main itself passes `cargo check` on macOS.
      - Turnstone's library suite under load: 657 passed, 11 failed. Seven
        of the failures pass on rerun (place lanes and workers under heavy
        compile load). The other four fail identically on unchanged main
        on macOS, so they predate this work and are macOS-specific: row
        count by viewport, the two probe-clicked inspector tests, and
        sky-surface time-zone text.
      - Final gates on the published lock:
        - `cargo_mode.py verify` passes: 1,283 packages, lock unchanged.
        - `cargo check --workspace --all-targets --locked` passes.
        - The library suite passes 660 with 9 ignored. Of its 8 failures,
          the four place tests pass alone, and the other four are the
          macOS failures above.
        - So the gates are 664 passing and 4 macOS-only failures that
          predate this change.
        - The Windows run and Linux CI are Mark's, as for S0.
    - Next, still under SC: pass B (web surfaces onto `PeltContent`'s
      surface lane, with the browser lane); controller-level input instead
      of the legacy pointer path; then step 5 (S8).
  - The Scenograph lane's command-menu follow-up (its SE32 and SE35) landed
    in Turnstone, on the pins step 4 already moved past Mere `16c1ef8d`.
    Mark's choices for Turnstone (2026-10-08):
    - A bare `>` (or a right click) shows the contextual rows, then the kept
      commands, then the recent ones, then "All commands…", which expands to
      the whole catalog in its composed order.
    - The default kept commands are the browsing eight: Back, Forward,
      Reload, Stop loading, Toggle live content, Open node in Workbench, Fit
      view and Save session.
    - Keep and drop work through a control on each command row and through
      Ctrl+D on the highlighted row, both exposed to assistive technology.
  - What changed in Turnstone:
    - `available_actions` is still the single composition, for the
      snapshot, the automation runner and the lane. The `>` lane reads it
      through `cambium::CommandSet`: rows register under their labels, and
      the rows composed ahead of the static registry form the `context`
      category, so they still lead. Search covers every command.
    - Choices (kept, dropped, recent) are saved in the session's view
      sidecar (`ViewIntentV1.command_menu`), never in graph truth (SE31). An
      older sidecar opens with the defaults. Keeping or dropping saves the
      session; recent commands ride the next save.
    - On a graph pane, the right press now goes to the canvas, so a
      right-drag selects. The palette opens on the release the canvas
      reports as a click (`take_context_request`), selecting the node under
      it first. Elsewhere, the menu still opens on the press.
    - The palette's rows were missing from the accessibility tree; only the
      input and the install review were projected. Each row is now a button
      that commits it, and each command row has a "Keep …" or "Drop …"
      button. Both use the same actions as a click (`A11yRoute::Action`).
    - Ring: "All commands…" is in Dispatch. Keep and drop are HostOnly,
      since Dispatch is granted to every participant by default and would
      let one rewrite the person's menu.
    - Not done: a headed check of right-drag selection and the menu in the
      window (the scenarios are Windows-gated), and zoom scaling for the
      Keep/Drop control's font, which the themed sheet does not yet size.
    - Gates on macOS:
      - New tests: 10 for the command menu, including right click against
        right-drag and the accessible rows; 1 for the sidecar; and 1 chrome
        click test where the label commits and the control toggles.
      - Two existing tests changed. The snapshot test searches `>res`,
        because search now ranks kept and recent commands first. The
        expanded lane is compared with the catalog's own order.
      - `cargo check --all-targets --locked` and `verify` pass, with the
        lock unchanged.
      - The library suite: 675 passing (five place tests needed a rerun
        alone), and the four macOS-only failures recorded for step 4.
  - Request for the next coordinated repin past Mere `18404a4c`: the
    Scenograph lane's C2 brief. Mark ruled SE45 to SE50 in the Scenograph
    editor plan:
    - One store for the menu's choices. Turnstone's `ViewIntentV1` keeps
      `cambium::CommandChoices` (same JSON field names, so stored sidecars
      read back unchanged) in place of `CommandMenuV1`.
    - The `>` lane's in-progress state (query, selected row, expanded) reads
      through `cambium::MenuSession`.
    - `CommandSet::menu` now returns `Vec<&Command>`; rows draw with
      `CommandItem::from`.
    - The shared verbs Turnstone offers register under catalogue ids,
      through `catalogue::command(id)`, so their labels follow the
      catalogue: nav:back/forward/reload/stop, view:fit, physics:toggle,
      node:delete, session:save, pane:settings/trail/workbench,
      palette:open, node:viewer_auto, and workbench:split_beside,
      split_out and stack_onto if offered.
    - Choices stored under the old label ids fall away; there are no
      alias tables (SE50).
    - Labels change ("Fit view" becomes "Fit to view", "Play/pause
      physics" becomes "Play or pause physics"). Turnstone resolves those
      by label in `scenarios/available_actions.scn`,
      `proof3_physics_capability.scn` and `proof3_recency.scn`, and in
      `src/action.rs`, `app/palette.rs` (`DEFAULT_KEPT_COMMANDS`),
      `app/tests.rs`, `session.rs`, `remote_projection.rs` and
      `shell/drive.rs`.
    - Labels will be revised again (SE51). Registering through the
      catalogue absorbs that without code.
    - Done when: the sidecar stores `CommandChoices`, the shared verbs use
      catalogue ids, the lane reads through `MenuSession`, the command-menu
      tests pass, and a headed check opens the palette, searches, keeps and
      drops a command, and reloads with it kept.
  - Checked for the browser sidecar scope: `pelt-core` and `page-load`
    compile for `wasm32-unknown-unknown`, with a dependency cone of 80
    packages. That proves the dependency boundary only, not browser
    usability.
    - Found in passing: `node_uses_web_surface` (`src/app/node_arms.rs:1701`)
      matches only Weld, so Scry and Servo nodes take the document Back and
      Reload path, and Stop does nothing for them.
    - Nothing was built or run.

- **2026-10-07, site/composition review:** recorded the browser-sidecar scope
  proposal under §6, separating session authority from browser wrapper and
  naming the existing carrier, storage, citation and lifecycle owners. Read
  Turnstone `50d44f6` and the current local Mere plans/implementation; no browser
  code changed, and nothing was built or run for this scope review. The proposed
  browser cuts do not change U2's priority, U15/U16's ownership or stage order.

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
- Mere app composition brief (U8; AC1 to AC6, E1):
  `mere/design_docs/cambium_docs/research/2026-10-06_app_composition_brief.md`.
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
