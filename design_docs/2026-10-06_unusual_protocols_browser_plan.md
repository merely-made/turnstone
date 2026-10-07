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
  - Asked the identity session to report Knot's repin (commit, Mere and
    Genet revisions, S77). Asked the physics session who owns WS4 and the
    stack seams rounds (neither the fidelity plan nor Mere's log names a
    session) and which session holds Turnstone's dirty browser tree.
  - Nothing was built or run.

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
