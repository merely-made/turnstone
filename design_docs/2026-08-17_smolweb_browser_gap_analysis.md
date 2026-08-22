# Browser gap analysis

Whether turnstone can stand where a browser stands, in two parts. Part 1 is
the anatomy: the taxonomy of a typical browser's chrome, controls, and
commands, expressed in mere/genet design language, with each element's state
in this tree. Part 2 is the smolweb plumbing: whether it can browse gemini the
way Geopard and Lagrange can. Motivated by the browser extension direction: a
satellite extension is only worth shipping if the native host behind it is a
browser someone would choose.

The benchmark is verified against Lagrange's own help document and both
projects' READMEs, and the turnstone column against this tree, not against our
plans. Lagrange is the bar; Geopard is the floor a pleasant minimal client
sets.

> **Status, 2026-08-22:** this is now a historical gap map, not a current
> implementation report. Native smolweb routing, Gemini input, persona-scoped
> client identities, durable TOFU review, and feed subscriptions have landed.
> Titan now has an explicit body/MIME/token/confirmation composer, and Spartan
> `=:` lines lower to a typed body-submission interaction using that same
> composer. Writes are separate actor commands: they are neither replayed as
> fetches nor followed through redirects inside the mutation. Linked raster
> images now render inline through the browser fetch actor. Binary and explicit
> attachment responses now enter durable download custody and project through
> Steward. Gemini text bodies now render incrementally before response
> completion and replace in place without losing the prefix. The gemtext
> typography pass now supplies site-derived color, readable measure, role-aware
> type, complex-script segmentation, and pixel-granular scrolling. Inline audio
> is deferred until a host-owned playback seam and another document consumer
> can force a reusable contract.
>
> **Acceptance, 2026-08-22:** live Gemini browsing and status-10 input, a local
> Gemini inline image, local Gemini download custody, local Titan and Spartan
> mutations, local prefix-before-EOF Gemini streaming, local multilingual
> typography, and Livery-to-Knot source evidence all have checked-in `RESULT ok`
> receipts and headed captures in
> [`docs/receipts/smolweb_acceptance_20260821`](../docs/receipts/smolweb_acceptance_20260821/README.md).
>
> **Dependency cutover, 2026-08-21:** the obsolete `genet-layout` and
> `stylo_taffy` consumer edges are gone. Turnstone owns its retained host
> adapter over `genet-livery`; Mere Canvas retains a `LiveryDocument`; and the
> Graphshell wasm surface uses `genet-render`. The clean-checkout and focused
> test receipt is in
> [`2026-08-21_livery_buckram_dependency_cutover.md`](2026-08-21_livery_buckram_dependency_cutover.md).

## Part 1: the anatomy of a browser, in this design language

The taxonomy of a typical browser, each element named in mere/genet terms,
with its state in this tree. The recurring pattern is that the conventional
element decomposes here rather than translating one-to-one, and the
decomposition is usually the stronger design; the gaps are mostly surfaces,
not concepts.

### Frame and session

| Browser element | Here | State |
| --- | --- | --- |
| Window | A lens over one app state; N windows are synced lenses, and the chrome is a window-root subtree of one shared Cambium forest | Live: `chrome_view.rs` consumes `push_forest_projection` + `layout_subtree`; lens windows carry caption chips |
| Tab strip | Decomposes three ways: the **node** (graph object) holds identity, the **tile** pins a member as a surface, the **tab** is a tile's handle inside a Workbench (`gs::TileTab`) | Live: Workbench has tab, split, stack, close, drag, tear-out, with headed scenarios |
| Drag a tab out into a window | The tear-out trichotomy | Live: `rung7_tile_tearout.scn` |
| New-tab page | No dead-start page; the Orrery plus a summoned omnibar is where address entry begins | Live by construction; whether an empty space wants a start projection is an open product question |
| Session restore | Pandect `graph.json` for graph truth; `frame.json` and `windows.json` for placement | Live, with restore scenarios |
| Private window | **The shallows** (working name): a space property, not a window mode. Unkept, unsealed, never persisted; any lens can look in; dashed everything is the tell; `keep` is the one exit, with a provenance confirmation. Ember is forbidden there, so the accent's absence is itself the privacy indicator | Designed in turn 2; unbuilt |

### Navigation and address

| Browser element | Here | State |
| --- | --- | --- |
| Address bar | The omnibar, one line with three intents: **find** (graph nodes first: the graph is the history made spatial; committing a match selects, never refetches), **go** (address-shaped input engages the fetch lane), **do** (`>` prefix, the actions lane) | Live: retained chrome, suggestion rows with real `on_click`, `OmnibarCommitRow` through the spine |
| Back / forward | Per-node, not per-window: a committed navigation appends to that member's lineage (the web-platform contract's ruling), and the Trail pane is the chronological projection | Lineage vocabulary ruled; the two-button affordance on a focused node is unbuilt |
| Reload / stop | A re-command of the fetch actor against the node's address | Fetch exists; no reload control, no stop, no progress affordance |
| Status-bar link preview | Undesigned. Candidate: the hover face of the would-be node, since a link here is a prospective node rather than a location string | Absent |
| Favicon and page title | The node's face: favicon discovery already enriches the requesting node by member-keyed stamp | Live in `browse.rs` |

### Content and reading

| Browser element | Here | State |
| --- | --- | --- |
| The page | A member's document session behind an engine registration; HTML routes to the Livery lane, switchable per node via the viewer override when another registered lane exists | Live; the settings row's whole point |
| Reader mode | Not a strip-the-page hack: nematic lanes for grammars the engine owns natively, and genet's three-head Hekate negotiator (smolweb extract / middlenet / fullweb) for HTML | Nematic lanes live for cards/capture; Hekate planned |
| View source / inspect element | The Inspector pane reads document structure through nematic engines; genet-probe resolves elements by role and label | Live |
| Find in page | No surface. The structural read the Inspector already does is the index a find would walk | Absent |
| Zoom | Per-window scale is the host's; a per-node text-scale facet fits the Apparatus | Absent as a control |
| Save page / print | The frozen realization: `graphshell_client::frozen` renders a scene as navigable semantics (DOM tree, AccessKit tree, HTML table), and a captured representation attaches to the node rather than becoming a file in a folder | Machinery landed in mere (B1); no turnstone surface |
| Downloads UI | A completed download keeps its source URL and response metadata, deposits exact bytes in the session representation store, attaches their hash to the node, and records the collision-safe visible destination as metadata. Steward projects the durable custody record | Live; destination follows `TURNSTONE_DOWNLOAD_DIR`, the OS downloads directory, then the profile fallback |

### Collections

| Browser element | Here | State |
| --- | --- | --- |
| History | The Trail pane over graph truth: recent + history rows with provenance, and removal is a first-class row | Live |
| Bookmarks | A kept node with a face is a bookmark with an icon; folders are containment; the Roster is the manifest view | Live as concepts; no one-gesture "keep this" affordance named for it |
| Feed subscriptions | A property of a kept capsule node; the app-tier watches machinery (W3) is the refetch loop; entries arrive as nodes | Watches landed; the subscription wiring is Part 2's item 5 |

### Commands and input

| Browser element | Here | State |
| --- | --- | --- |
| Keyboard shortcuts | The host's `KeyPress` vocabulary end to end (now windowing-neutral); app policy hooks (`key_intercept`) own meaning | Live |
| Command palette | The omnibar's `do` lane; the filterable action list is the component catalog's flagship composition | Hint row live; the full list is the catalog's next move |
| Menus / context menus | **The regard** (working name): what you summon when you turn to face a node. Not a menu: a Peek with act-tier rows, floating transient or pinned flat into an Inspector seed; rows are the same command objects the omnibar's do lane files; one list from canvas node, roster row, trail row, or content link. Needs only a summoning verb, `regard(node)` | Designed in turn 2; unbuilt |
| Automation | genet-probe: resolver + Automatable/Driveable; apps self-drive via DOM-carried identity, never synthetic OS input | Live, with scenario receipts |

### Identity, policy, and settings

| Browser element | Here | State |
| --- | --- | --- |
| Profiles | Personae: sealed, device-carried; the roster is a view, not an address book | Live and wired |
| Permission prompts | `web_policy.rs` plus the participant gate (one gate for scripts/wasm/peers/agents) and servitor petitions | Gate designed and partially live; per-origin prompt UI unbuilt |
| Settings | The Apparatus / Inspector / Steward split, plus the Settings pane over the provider; configurability-over-defaults is standing doctrine, and changes are live swaps, never restart-required | Live; Steward now has its first durable projection, download custody |
| Extensions | Three lanes by trust: rhai command packs (host automation), Wasm component `DocumentScript` (untrusted portable), register-mod-loader (the registry seam) | Lanes ruled in mere; no turnstone extension surface |
| Devtools | The Inspector plus probe snapshots plus the trace diagnostics (`CAMBIUM_HOST_KEY_TRACE`) | Partial by design; a network pane over the fetch actor is unnamed |

### Turn 2: the control surfaces, designed

The verdict below ("anatomy built, wearing no controls") was answered by a
design pass in the workbench canvases (Projection Grammar Literature Review
folder, 2026-08-17): six surfaces, each a face over machinery this table
already cites, with an ember budget of one mark per strip.

1. **The chrome strip**: back/forward walk the focused node's lineage
   (per-node, never per-window; forward dims at the lineage tip); reload
   re-commands the fetch actor and becomes stop while running, the ember
   underline as the progress affordance; the omnibar keeps find/go/do with
   mode inferred, never a picker; `keep` sits beside it.
2. **The tab, decomposed and placed**: tabs live on the tile, not the window
   top, because the window is a lens and lenses do not own content. Closing a
   tab closes a handle; the node keeps living in the graph.
3. **Arrivals**: downloads, save, and print as one tray, because the contract
   already unified them: everything that arrives is a representation
   attaching to a node with its source URL kept. A frozen realization is an
   arrival from your own graph.
4. **Find over the index**: matches are structural rows (role and label) from
   the nematic read the Inspector already has, never a pixel scan; a link
   match previews as the node it would become.
5. **The regard** and 6. **the shallows**: the two formerly undesigned
   regions, now in the tables above.

Three contracts fall out of the pass, and they come before the chrome:

- Every control is a projection of scene or actor state that must be
  readable first (lineage depth, fetch phase, keep state, match count). If
  the scene cannot say it, the strip cannot wear it.
- The fetch actor's phase wants to be first-class scene data (requested,
  streaming, settled): stop and progress need it, and streaming render
  (Part 2, item 6) needs the same contract.
- `keep` appears in three places (strip, regard, shallows exit) and must be
  one command object with one receipt: a kept node with provenance.

### What the table says as a whole

The concepts are all present or deliberately transformed; almost every gap is
a missing control surface over machinery that exists, and as of turn 2 every
one of those surfaces has a design that cites its machinery. That is a
different readiness picture from "turnstone is not a browser yet": it is a
browser whose anatomy is named and largely built, whose controls are designed,
and whose remaining work is wiring faces to contracts that mostly exist.

## Part 2: the smolweb plumbing

## Where the stack already meets the bar

The transport and parsing story is not the gap. The thirteen protocol crates
are published and spec-faithful; errand fetches gemini (TLS, TOFU pinning,
`CertificateChanged` surfaced as a typed error), gopher, finger, spartan, nex,
guppy, and titan, and normalizes status and MIME; `mere-fetch` routes any
errand scheme through the fetch actor turnstone already runs. Lagrange's
protocol list and ours differ by one entry in each direction: Lagrange speaks
misfin as a mail lane; we have misfin's send path in errand but no mail
surface. Nobody in this comparison speaks Reticulum; we have a plan for it
(2026-08-03) and the gemini module is explicitly shaped so a Reticulum link
can drive the same path without TLS.

Rendering is not the gap either, in kind: cambium-nematic projects gemtext,
gopher menus, feeds, and nex listings as Cambium views, and nematic lowers
thirteen formats into `EngineDocument`. Turnstone's content port routes by
engine registration, so smolweb joins by registering a lane, not by new
dispatch code. That is a better foundation than either benchmark has; neither
Geopard nor Lagrange has a second renderer to switch to per node.

And the graph-native surfaces map cleanly onto two of Lagrange's sidebar
features. History: the Trail pane builds real recent + history rows off graph
truth, which is stronger than Lagrange's `visited.2.txt`, since a visit is a
node with provenance rather than a timestamped URL. Bookmarks: a kept node
with a face is a bookmark with an icon; folders are containment. These need
smolweb addresses to flow into them, not new machinery.

## The gaps, ranked by how much they disqualify us today

### 1. The shell does not route smolweb at all

The single fact that decides "not ready yet": turnstone's shell registers one
static lane, `genet.web`, over a fetcher that speaks https and data URLs. The
comment at the registration site says smolweb rungs join by registration, and
none has. `mere-fetch`'s errand routing exists and turnstone does not call
it for content. A gemini URL in the omnibar today normalizes correctly
(`ui.rs` tests prove that much) and then has no lane to land in.

Everything else on this list is unreachable until this lands, and it is the
cheap one: the lane infrastructure, the parser, and the views all exist.

### 2. Interactivity statuses are errors, not conversations

`mere-fetch` maps `Status::Input` to `Err("input required")` and
`Status::CertRequired` to `Err("client certificate required")`. In Lagrange,
status 10/11 opens an input dialog with paste and sensitive-input handling;
a search on gemini is unusable without it. This is a UI conversation the
fetch actor cannot have on its own: the effect vocabulary needs an
`InputRequested { prompt, sensitive }` arm and a pane to answer it, and the
answer re-fetches with the query attached.

### 3. No client-certificate identities for gemini

Lagrange's identity management is the feature the gemini community actually
uses it for: create a self-signed cert, pin it to a capsule, never send it
elsewhere. errand's gemini fetch takes no client identity today (misfin's
`ClientIdentity` exists one module over, so the shape is in the house), and
turnstone has personae, a real identity layer the benchmark lacks. The work
is a seam through gemini-protocol and errand, then a personae-backed store
mapping capsule → certificate. Done right this is a differentiator: Lagrange
manages loose self-signed certs; we can seal them under a persona and carry
them across devices.

### 4. TOFU pins do not survive a restart

errand pins trust-on-first-use and `mere-fetch` ships
`install_in_memory_smolweb_tofu`, named honestly: pins die with the process.
Lagrange persists fingerprints with expiry and shows the user a decision when
a certificate changes. Until pins persist, our TOFU is ceremony rather than
trust, and `CertificateChanged` never reaches a human. Pandect or muniment is
the obvious pin store; the decision UI is small but must exist, because
silently re-pinning is worse than not pinning.

### 5. No feed subscriptions

Lagrange subscribes to gemini feeds by tagging a bookmark and periodically
refetching. cambium-nematic already renders feeds; what is missing is the
subscription loop (a schedule against kept nodes) and an unread surface. The
graph gives us a better home for this than a sidebar: entries are nodes,
subscription is a property of a kept capsule node, and the W3-landed watches
machinery (app-tier watches, 2026-08) looks purpose-built to carry it.

### 6. Content-surface conveniences

Individually small, collectively what "pleasant" means:

- Titan uploads: closed 2026-08-20. The omnibar composer accepts typed or
  dropped-file bodies, editable MIME, a masked optional token, and literal
  confirmation. Titan shares Gemini TOFU and capsule-scoped client identity.
- Spartan prompts: closed 2026-08-20. `=:` remains a typed submission through
  parser, document IR, hit testing, session click, app composer, and transport.
- Linked raster images: closed 2026-08-21. Turnstone opts its gemtext sessions
  into image-link promotion, requests the absolute URL asynchronously through
  the existing fetch actor and durable TOFU store, decodes within configurable
  count/byte/dimension limits, and paints through the shared image sidecar.
  The image remains a clickable link. Inline MP3/Ogg/WAV playback is deferred
  as of 2026-08-22: there is no host-owned playback seam, and restoring retired
  incumbent media dependencies for one consumer would freeze the wrong
  contract. Reopen it when a second document consumer can exercise the same
  transport, lifecycle, and control surface.
- Downloads: closed 2026-08-21. Binary and explicit attachment responses keep
  their exact bytes through the fetch actor, deposit them in the session's
  Muniment store, attach the content hash to the graph node, and write a
  collision-safe visible copy. Source, MIME, disposition, byte count, status,
  destination, and hash persist as one custody facet projected by Steward.
  Download-byte progress remains a chrome progress-surface concern.
- Streaming render: closed 2026-08-22. Gemini response bodies stream from
  Errand through the fetch actor and Smolweb session into the retained content
  surface. A local TLS acceptance capsule withholds its tail until the app
  captures the visible 65-byte prefix; the final frame preserves that prefix
  and adds the 57-byte tail. Replacement invalidates the keyed surface, and
  Netrender composes retained tiles in deterministic painter order.

### 7. Typography polish

Closed 2026-08-22. Gemtext defaults to a stable site-derived palette, a serif
reading face with monospace preformatting, a centered 720-pixel maximum measure,
1.5 line height, and a visible heading hierarchy. Same-capsule and external
links use distinct arrows, while quote and list roles retain their own spacing.
The document-canvas Parley edge enables dictionary-backed complex-script line
and word breaking, with font fallback covering the acceptance fixture's Latin,
Greek, and Japanese text. Wheel scrolling is pixel-granular. Animated smoothing,
if wanted, belongs to generic host polish rather than the smolweb protocol lane.

## What turnstone has that neither benchmark can answer

Worth naming so the analysis is not only deficit. A capsule visited in
turnstone is a node in a personal graph with provenance, relations, and carry;
"bookmark" and "history" are projections of something richer. Two renderers
are registered and switchable per node. Identity is a sealed persona rather
than a cert file. And the same views are heading into a browser via
cambium-genet-web-host, which neither GTK nor SDL will ever do. The gap is
real but it is a browsing-surface gap, not an architecture gap.

## Sequence

Items 1 through 5, item 6's mutation, image, download, and streaming paths, and
item 7's typography pass are closed. Inline audio is deliberately deferred at
its missing shared host seam. The smolweb gap map is otherwise complete; Part
1's browser controls remain a separate surface backlog.

## Done conditions

- A gemini capsule browses end to end in turnstone: omnibar → errand fetch →
  nematic lane → rendered gemtext, links followable, back/forward via trail.
- A status-10 search prompt is answerable from the UI and re-fetches.
- A capsule that requires a client certificate is usable with an identity
  minted and pinned under the active persona.
- TOFU pins persist across restarts and a changed certificate presents a
  human decision.
- A tagged capsule node refetches on a schedule and surfaces new entries.
- A `titan://` address opens a composer without issuing a zero-byte upload;
  confirmation sends one actor command and returns its receipt.
- A Spartan `=:` prompt opens the same composer and submits its body without
  being mistaken for a navigation link.
- A gemtext image link issues a second actor request, paints the decoded image
  inline, remains followable, and leaves a bounded placeholder if fetch or
  decode fails.
- A binary Gemini response deposits exact bytes in the session representation
  store, attaches their hash to its source node, writes one collision-safe file
  in the configured destination, survives session persistence, and appears in
  Steward with completed or failed custody status.
- A chunked `text/gemini` response paints its prefix before connection close,
  then replaces that live body with a final frame that preserves the prefix and
  adds the tail.
- A gemtext page renders a stable site palette, readable heading and body roles,
  distinct link classes, preformatted text, and Latin, Greek, and Japanese text
  without a complex-script segmentation diagnostic.
