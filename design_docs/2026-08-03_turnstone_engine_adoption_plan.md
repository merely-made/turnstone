# Turnstone engine adoption — arbitrary engines, selectable in the app

**Date:** 2026-08-03
**Status:** in progress, rechecked 2026-10-07; the current browser consumer sequence is
the open work of the [user-agent taxonomy](2026-11-02_user_agent_taxonomy_plan.md) (the October appendix is in the [archived plan](archive_docs/2026-11-02/2026-08-03_user_agent_taxonomy_plan.md)).
This plan retains the engine-specific construction and sandbox evidence.
The ask: arbitrary selection of engines,
possible and selectable within the app, including genet with its rungs
(placeholder where a rung cannot spawn yet, best effort, for testing and
design-to-shape purposes).

**Rechecked 2026-11-02:** E0 is landed (step heading below). E1, E3 and E4 are open: `src/` has no `EngineEnableSet`.

**Authority relationship:** the
engine picker and pluggability plan (`mere/design_docs/archive_docs/2026-10-28/2026-06-15_engine_picker_and_pluggability_plan.md`)
(mere, 2026-06-15) owns the model: three activation levels collapsing to one
`is_available` predicate, global default + per-session override, cargo
features as the build tier, no-handler legibility, and the picker/flip
ownership split with verso. Its shipped phases (0 to 3) landed in meerkat,
and meerkat is deleted, so the receipts are gone while the model stands.
Turnstone independently re-landed the foundation. This plan binds the
remaining model to Turnstone's actual state; it does not re-decide it.

## Current state (rechecked 2026-10-06)

- **Routing**: the content lane routes through `inker::EngineRoutePolicy`
  with `pinned_engine` support ([effects.rs](../src/shell/effects.rs)).
- **Session engines registered**: `genet.livery` (LiverySessionEngine), Reader, the
  native smolweb lanes, and the Knot authoring engine, in a
  `SessionRegistry<netrender::Scene>` ([mod.rs](../src/shell/mod.rs)).
- **Picker**: Inspector owns viewer changes. The initial October audit found
  a hardcoded Auto/Livery/Reader list, with Weld added by feature, and unknown
  saved pins displayed as Auto. The local B0 patch replaces that with a registry-backed
  inventory and explicit unavailability; its verification is recorded in the
  taxonomy progress log. Persisted `genet.web` pins still migrate to
  `genet.livery` when their session is adopted.
- **Surface engines**: the Windows Weld first cut is now wired behind
  `--features weld`: an inker `SurfaceEngineRegistry`, `weld.chromium`
  producer map, D3D12 transferred-handle import cache, primary-window
  composition and pointer/key routing. The local Windows Scry consumer is
  wired behind `--features scry`; native qualification is recorded below.
  Servo/Graft construction is now implemented behind `--features servo`;
  the published three-engine baseline (`Code/testing/turnstone/receipts/browser_family_20261007/README.md`)
  qualifies the Windows build and scoped native pixel controls. Foreign
  accessibility qualification is the next consumer lane below.
  [2026-07-18_meerkat_harvest.md](archive_docs/receipts/2026-07-18_meerkat_harvest_receipts.md) and git
  history are the donors.
- **Compositor**: the shell already composes per-surface textures via
  `netrender` `compose_external_texture` ([render.rs](../src/shell/render.rs),
  [lens.rs](../src/shell/lens.rs)), so a GPU `SurfaceFrame` is another
  external-texture layer, not a new compositor concept.
- **Scripted lane**: `genet_documents::ScriptedSessionEngine` exists;
  Turnstone's `piccolo` feature pulls the script-engine crates but registers
  no scripted content engine.
- **Features**: `wasm`, `piccolo`, `scry`, `weld` and `servo`. The Weld producer is constructed
  lazily on Windows after the host device and CEF runtime path are available.
  A feature build alone is not a successful runtime construction receipt.

### October coordination and accessibility

U2 of the [unusual-protocols plan](archive_docs/2026-11-02/2026-10-06_unusual_protocols_browser_plan.md)
puts accessibility on Windows, macOS and Linux first. The scoped live-pixel
receipts below do not establish browser content trees or assistive actions.
U9 holds the final consumer repin for WS4 pushed and a tested set in order:
Knot, Redshank, then Turnstone. Older compatibility candidates remain useful
qualification evidence, with current-stack adoption coordinated separately.
Servo 0.7's new public accessibility-action method makes its coherent
Surfman/ANGLE migration a practical accessibility dependency.

The October 7 foreign-tree lane (`Code/testing/turnstone/receipts/browser_a11y_20261007/README.md`)
adds real Servo callback admission and retained subtree composition on the
existing UI thread. Mere's four additive producer methods are published and
library-tested. The coherent Knot/Redshank/Turnstone set passes 704 Turnstone
library tests (nine ignored), all-targets and native builds, two real Servo
lifecycle runs, and limited Windows UIA heading/ancestry observation. The
upstream layout producer advertises no actions, despite the
public Click forwarder, so this host explicitly refuses assistive actions.
Its actual supplier scope is wrapper plus top-level document trees; recursive
consumer tests do not qualify iframe export. The fixture's body text and form
roles do not reach the Windows provider; its document ancestors have no
TextPattern. Hidden/script text also needs supplier correction. Human AT on
all three platforms and the independent pixel/synchronization gates remain open.
The pinned supplier's placeholder focus is projected explicitly to its
document root by configurable `TURNSTONE_SERVO_A11Y_FOCUS=document-root`;
`upstream` preserves raw focus. This supplies neutral subtree focus while
DOM focused-node reporting remains unsupported. Receipts preserve the raw
and projected focus distinction.
The raw-focus native negative control fails with absent/unreachable focus;
its failures and the earlier observer timestamp/traversal failures remain in
the receipt. A passed heading graft is not full browser accessibility.

The October 7 published-state checkpoint (`Code/testing/turnstone/receipts/browser_supplier_integration_20261006/current-stack-reconciliation/README.md`)
records WS4 R6 and the shared AccessKit helper/E1a on Mere main. The later
published family receipt (`Code/testing/turnstone/receipts/browser_family_20261007/README.md`)
qualifies the Genet decoder repair and ordered Knot/Redshank pins, ending at
Turnstone `e00869c`. The new foreign-tree lane repeats that order for Mere's
additive native semantic seam. Human assistive walks and complete browser
action support remain open.

### October 6 supplier integration history

The supplier integration receipt (`Code/testing/turnstone/receipts/browser_supplier_integration_20261006/README.md`)
qualifies that shared Weld adapter at compatible Mere `db4ee312`,
Woodshed `0433a21d5b` and Knot `a0c9740df4`, retaining Genet `450f45b694`.
Turnstone's local CEF mouse/text forwarding bridge is removed. The exact
two-engine executable passes 83 distinct focused tests and four native runs:
Weld input/find/zoom, Scry input/resize/reconstruction, Scry process restart
with retained storage, and Weld's retained permission Deny callback. The
earlier engine-specific sections below are dated implementation history;
their old pins and test counts do not describe this current receipt.

The selected Servo profile policy is one explicitly named, configurable
profile shared by its views in the process. `TURNSTONE_SERVO_PROFILE` selects
the name, defaulting to `Default`; `TURNSTONE_SERVO_PROFILE_DIR` optionally
selects an absolute directory. The UI-thread factory retains the process
root across view closures and session changes. Scry and Weld retain their
per-node profiles.

Servo B3's qualification candidate uses upstream `1d44e5dd` and tested Graft
`b25df3b706`, with GPU imports on
Turnstone's device and queue. Its source includes ordered callbacks, native
logical-key releases, explicit denial of unsupported permissions, view
retirement and process shutdown. Basic mouse fields, navigation without a
stop API, requested zoom and HiDPI scale projection retain explicit limits.
The older full three-engine set builds successfully with Graft `02ddc11ef4`;
the corrected-swap `b25df3b706` candidate also builds successfully. Its first
native control fails before creating a view because `graft.servo` reaches
the document-session dispatcher. The failed source and run remain archived;
the host dispatcher is repaired and all 64 focused shell tests pass. Its
native assertions and bounded full shutdown pass, but direct image review
rejects a white reopened Servo A. The same binary separately qualifies four
simultaneous Servo/WebView2/CEF pages, resize and cleared teardown, plus fresh
Scry/Weld input, restart and permission controls with loaded-module evidence.
The six-run synchronization comparison (`Code/testing/turnstone/receipts/browser_supplier_integration_20261006/sync-diagnostic/README.md`)
uses one frozen diagnostic executable: Existing passes once and reproduces a
white reopened A once; producer-only and normalization-only each pass; Both
passes twice. All six assertion/exit guards pass, demonstrating why the pixel
review is required. Its 98 distinct browser tests pass. The association with
waits does not establish visual causality; production defaults remain unchanged.
The temporary consumer config is removed and all 175 previous candidate inputs
are restored exactly, with the diagnostic host and supplier sources archived.
Default reopen correctness, foreign accessibility and the final U9 set remain
open. U14 authorizes committing and pushing the tested older candidate's
Cargo/Rust work first, so S0 starts from it. The final coordinated current
family and application release acceptance remain open.

## The model, restated for Turnstone

Three engine kinds, two of them live here today:

| Kind | Registry | Output | Turnstone state |
|---|---|---|---|
| Document | `EngineRegistry` | `EngineDocument` blocks | via nematic lanes (cards/capture) |
| Session | `SessionRegistry<Scene>` | paint scenes | live: livery, smolweb, knot |
| Surface | `SurfaceEngineRegistry` | GPU texture stream | Windows Weld behind `weld`, Windows Scry behind `scry`; Servo/Graft source behind `servo`, native qualification pending |

"Genet with its rungs" means the genet engine's capability ladder is exposed
as selectable lanes rather than one opaque entry: `genet.livery` (clean-room
CSS/layout lane), `genet.scripted.*`
(Boa/Nova/piccolo scripted DOM), and later rungs as they exist. A rung that
cannot spawn yet still appears in the picker as a disabled row naming why
(feature off, not registered, platform gap). That is the design-to-shape
placeholder: the selection UI carries the full ladder honestly instead of
hiding the unbuilt parts, and the no-placebo rule holds because a disabled
row never pretends to spawn.

## Decisions

### turnstone_engine_adoption D1: an engine arrives compiled in or as a separate process, in one inventory (2026-11-12)

Mark, 2026-11-12: "Both, one inventory (Recommended)", to "when you say 'if you load that mod', how should an engine arrive?". The registry-backed inventory treats an engine as a producer that is either compiled in behind its cargo feature or run as a separately installed process sharing textures with the host. Features ship first; the out-of-process seam is reserved now. Refines the model's "cargo features as the build tier": the build tier is one of two ways in, not the only one. Nothing is built: E3's `EngineEnableSet` is still absent, and no out-of-process producer exists.

Links: area engines · amends E3 · draws on graphshell_host D1 · summary engines load compiled-in or out-of-process through one inventory

2026-11-14 follow-up to [graphshell_host D2](2026-10-28_graphshell_host_plan.md#graphshell_host-d2-native-turnstone-first-then-the-browser-extension-2026-11-14). The desktop product is one native Turnstone per supported platform. Installing and switching engine packs in the running app is a proposal for E3. Its first proof would install and start one separately packaged engine, route a new surface to it, then switch away and stop it without restarting Turnstone. Live page state migration between engines is a separate question. [Graphshell host D3](2026-10-28_graphshell_host_plan.md#graphshell_host-d3-compose-the-frame-from-attached-producers-2026-11-14) carries the one-inventory model into general producer attachment. E3 owns engine activation.

## Steps

### E0. Registry-driven picker - LANDED 2026-10-05

Replace the hardcoded `VIEWER_OPTIONS` array: the Inspector viewer enumerates
engine ids from the registries the shell actually holds, plus declared-but-
unavailable entries with their reason. Auto stays row zero. The picker shows
kind (document/session/surface) so a surface pick reads as the black-box tier
the fidelity axis says it is.

Done when: registering an engine in `shell/mod.rs` is the only step that adds
it to the picker, and an unavailable rung renders as a disabled row with a
reason string.

Landed as the registry-backed engine inventory (`src/shell/project_engine_inventory.rs`,
`src/shell/shell_publish_engine_inventory.rs`, `scenarios/browser_engine_inventory.scn`);
qualification receipt `Code/testing/turnstone/receipts/browser_engine_inventory_20261005/README.md`.

### E1. The genet rungs

Register `ScriptedSessionEngine` behind the existing `piccolo` feature (and
sibling features per script engine as they are adopted), id
`genet.scripted.piccolo` etc. Add declared placeholder rows for rungs that
exist in genet but are not yet registrable here, sourced from a small static
manifest in Turnstone (the honest list of the ladder), so the picker shape is
testable before every rung is real.

Done when: a scripted page renders under the scripted rung when the feature
is on; with the feature off the row is present, disabled, and names the
feature.

### E2. Surface engines (scrying, graft, weld)

One cargo feature per consumer (`scry`, `servo`, `weld`), per the picker
plan's build-tier decision. Work:

1. A `SurfaceEngineRegistry` beside the session registry in the shell.
2. The host `ProducerFactory` hooks: parent window handle, wgpu device,
   fence handle, resolved `user_data_dir` from persona context (the
   `EngineProfileBinding` seam already defined in inker). This is the part
   meerkat's X1 pool owned; harvest technique from the meerkat harvest doc
   and history, code stays Turnstone-shaped.
3. Frames enter the existing compose path as external-texture layers keyed
   by node, beside the rasterized scenes.
4. Input routing and resize forwarding to the producer, per the
   `SurfaceEngine` contract.

Route integration is already there (`scrying.web` is a routable id, kept out
of default policy, reachable by pin), so E0's picker is the activation
surface.

Done when: with `--features scry`, pinning `scrying.web` on a node shows
the system WebView's texture composited in that node's tile on Windows, and
a scenario receipt captures it; graft and weld repeat the shape (their
producers may land later, each behind its feature, disabled rows until then).

#### E2-Scry Windows consumer (October 5 history, superseded by October 6 receipt)

The initial optional adapter came from Turnstone's Mere `d6e7601209` pin and
uses published `scrying` 0.7.1 with wgpu 30. Existing family pins stay intact.
`scrying.web` is constructed lazily on the UI thread against the primary
window's device and queue. Producers share an owned offscreen composition root;
each captures its own visual and persists an explicit per-node profile at
`scry/webview2-profiles/<node>` under the data root.

Each producer has a dedicated DX12 fence. Scry's producer-local fence counters
cannot safely share one fence across pages. Owned frame payloads pass through
the Scry/Graft importer, with an exact fence/metadata check and a wait for every
paint, including a reused allocation. Replacement and teardown retire cached
textures and synchronizers. Lazy capture/import failures retire the producer
and report failed content rather than a blank live page.

The host corrects the pinned adapter's mouse translation by using WebView2's
mouse API for mouse events, retaining touch/pen routing and actual button state.
Text and key forwarding use the existing producer seam; CDP input does not
qualify OS IME. Browser shortcuts remain owned by Turnstone. Find, typed page
zoom, correlated capture, credential answers, and an accessibility tree remain
unavailable until the adapter forwards them. Native rehosting and different-size
appearances of one node require separate gates.

The Windows consumer receipt (`Code/testing/turnstone/receipts/browser_scry_windows_20261005/README.md`)
owns the two-page, input, resize, engine-switch, teardown and restart evidence.
The full native script completes without external repaint and fails only four
cookie assertions; separate-process restoration passes pins, localStorage and
saved text while failing two cookie assertions. Manual review also finds stale
post-input images and blank reopened tiles despite live counters. That phase
kept B1 gated on cookie retention and current pixels. The October 6 consumer
receipt closes those Windows gates with fresh native captures and actual
cookie restoration; source compilation and frame counts alone did not close
them. The trio's release gates remain separate.

The pinned adapter's Windows keyboard helper blocks while pumping messages.
Early native runs stopped advancing, without establishing a blocked native
stack. Turnstone uses the
already-locked `webview2-com` 0.39.1 callback binding to queue CDP key commands
with one request in flight. Completion callbacks update weak queue state;
normal host polling submits the next command and reports errors or timeouts.
This preserves command completion order without nesting a Windows message
loop in the host handler. A retained host deadline restores progression without
external repaint. Observed CDP Enter/text DOM outcomes are in the receipt;
physical-keyboard and OS IME acceptance remain open. The October 6 receipt
supersedes this phase's current-pixel failures.

#### E2-Weld Windows first cut (implemented, headed receipt 2026-08-14)

`--features weld` adds the fourth Apparatus viewer choice, `weld.chromium`.
At process start, `TURNSTONE_CEF_PATH` (or `CEF_PATH`) causes the required
CEF subprocess probe before tracing or winit. Selecting the viewer then
initializes one process-wide CEF runtime and a per-node `RequestContext`
profile at the direct CEF-root child
`<data_root>/weld/cef-cache/<node>`. CEF rejects a nested profile child as its
global Default profile, so the direct-child shape is a correctness constraint,
not a cosmetic path choice. The renderer forces its Windows wgpu host to D3D12
before device creation, imports Weld's transferred D3D12 handle on that same
device, retains the texture by `resource_epoch`, and composes it in the
ordinary surface-plan order. A CEF callback-copy mailbox replacement closes
its old handle, so it does not leak one Win32 handle per paint.

CEF browser creation is asynchronous. Weld records visibility requested before
`on_after_created` and applies it when the `BrowserHost` exists; an eager call
must not mistake the not-yet-populated handle for a missing CEF runtime.

The concrete host projects mouse and keyboard input, focus, accelerated frame
composition, committed URL and title changes, auxiliary-navigable requests,
failures/crashes, and cursor callbacks. A committed in-page navigation updates
the same graph member and appends its per-member lineage. Cursor answers update
the native winit cursor, including hidden. S1 subsequently added Pointer
Events-shaped mouse/touch input and HTML DataTransfer-shaped drag/drop. PDF and
native printing, downloads, cookies, script results, standard automation,
permissions, auth, popup placement, and snapshots were unsupported in this first cut:
Weld has many of those operations, but Turnstone has not yet provided their
shared contract, callback/control UI, or durable policy. October 6 subsequently
qualified the retained native permission Deny callback, find and requested
zoom; the other operations retain their separate consumer gates.

The tail now follows Genet's
web-platform host contract (`genet/docs/2026-08-14_web_platform_host_contract_plan.md`).
That contract is standards-derived and shared by Weld, Scry, Graft, Genet's
rungs, and Smol. Turnstone projects committed resources into graph navigation;
cookies/permissions/auth into origin/profile registries and associated facets;
and PDF/screenshot output into representations attached to a source node unless
the user explicitly imports them. CDP remains a Weld implementation detail
behind WebDriver/BiDi-shaped automation.

`TURNSTONE_WELD_USER_AGENT` replaces the process-wide agent string;
`TURNSTONE_WELD_USER_AGENT_PRODUCT` replaces only its product token. They
are mutually exclusive. Neither is set by default.

Windows receipt: [e2_weld_windows.scn](../scenarios/e2_weld_windows.scn)
ran from a fresh `TURNSTONE_ROOT` with `RESULT ok`. Its first capture shows
the `example.com` Chromium tile composited between Turnstone's graph and
Apparatus pane. It moves over that tile's visible **Learn more** link and
requires a `surface-cursor` callback, then clicks. The second capture shows
IANA's Example Domains page in the same tile; the scenario requires both the
new focused URL and `content-navigated`. The run also created the UUID-named
direct-child CEF profile without a CEF profile-creation error.

Done: a Windows scenario pins `weld.chromium`, captures a composited Chromium
page, and records cursor, input, and same-member navigation round trips.
Lens-window composition, CEF wake-driven redraws, and the still-unprojected W8
operations above remain outside this first cut.

#### E2-Weld Windows sandbox bootstrap (2026-09-09)

Mark ruled Chromium's process sandbox default for Weld on Windows
(2026-09-09). The first cut above always ran CEF with
`CefSandboxMode::UnsandboxedTrustedContent`: Windows' ordinary re-executed-
binary entry point (`CefRuntime::execute_process_from` /
`CefRuntime::initialize`) rejects `Sandboxed` outright, because CEF 151 can
only create the sandbox context inside `bootstrap.exe` calling an exported
`RunWinMain` in a client DLL (wgpu-weld `61c3c0598c`, "Wire the Windows CEF
sandbox bootstrap"; `welding::CefWindowsSandboxContext` and
`validate_direct_entrypoint` in `welding/src/runtime.rs`).

Turnstone now has both routes:

- **Direct** (`turnstone.exe`, unchanged): `src/launch.rs::run_direct` probes
  the CEF subprocess role and, if the process reaches `ensure_weld_engine`,
  `shell::weld::initialize_runtime` initializes `CefSandboxMode::
  UnsandboxedTrustedContent`. This route cannot create a sandbox context, so
  it stays the explicit fallback.
- **Bootstrap** (sandboxed, new default): a second crate,
  `sandbox_bootstrap_win` (workspace member, `crate-type = ["cdylib"]`),
  exports `RunWinMain`. It is a separate crate rather than a second
  crate-type on `turnstone`'s own `[lib]`, which would give the cdylib and
  the `turnstone` `[[bin]]` the same output name and collide on
  `turnstone.pdb` (cargo#6313). `RunWinMain` calls
  `turnstone::launch::run_bootstrap`, which borrows CEF's sandbox context via
  `CefWindowsSandboxContext::from_raw`, runs the subprocess probe under it,
  records the route (`shell::weld::WeldSandboxRoute::Bootstrap`, thread-local
  — the context wraps raw non-`Sync` pointers, but everything that touches
  it runs on the one thread that called `RunWinMain`), and starts the
  ordinary app. `initialize_runtime` then initializes
  `CefSandboxMode::Sandboxed` through that context.

`TURNSTONE_WELD_SANDBOX` (`sandboxed` / `unsandboxed`) overrides the
route's default. An unsupported combination — `sandboxed` on the direct
route, or `unsandboxed` on the bootstrap route — is a clear error out of
`initialize_runtime`, never a silent downgrade to unsandboxed.

**Bundle layout** (mirrors wgpu-weld's `demo-weld-win`/`cef::build_util::
win::bundle`): a directory containing

- `turnstone.exe` — CEF's `bootstrap.exe`, copied and renamed.
- `turnstone.dll` (+ `.pdb`) — `sandbox_bootstrap_win`'s built
  `sandbox_bootstrap_win.dll`, copied and renamed to sit beside it (the
  raw build output keeps the crate's own name; only the bundle stage
  renames it to match the bootstrap executable).
- The full CEF binary distribution (`libcef.dll`, `icudtl.dat`, `*.pak`,
  `v8_context_snapshot.bin`, `chrome_elf.dll`, etc.) and its `locales/`
  directory, copied beside them.

No automated bundler binary was added in this slice (out of scope); the
layout above is assembled by copying those files from a CEF distribution
(`TURNSTONE_CEF_PATH`/`CEF_PATH`) into a staging directory.

**Launch commands:**

```text
# Direct, unsandboxed fallback (unchanged):
set TURNSTONE_CEF_PATH=<path to a CEF binary distribution>
cargo run --features weld

# Bootstrap, sandboxed (new default): from the assembled bundle directory
<bundle>\turnstone.exe
```

**Verification, 2026-09-09:** `cargo check --offline --features weld` and
`cargo check --offline --no-default-features --lib` both pass clean (only
pre-existing warnings); `cargo check --offline -p turnstone-sandbox-
bootstrap-win` passes with no PDB-collision warning; `cargo test --offline
--features weld --lib` passes the new `shell::weld` unit coverage (6/6).

A CEF 151.3.24 distribution with `bootstrap.exe` was available locally
(`Code/cef-cache/wgpu-weld/151.3.24/cef_windows_x86_64`), so the bundle above
was assembled from it (`sandbox_bootstrap_win.dll`/`.pdb` renamed to
`turnstone.dll`/`.pdb`, `bootstrap.exe` copied to `turnstone.exe`, the rest of
the CEF distribution and `locales/` copied alongside) and the bootstrap route
was run headed: `turnstone.exe` launched, logged
`turnstone::launch: turnstone starting` (confirming `RunWinMain` →
`run_bootstrap` → `CefWindowsSandboxContext::from_raw` → the subprocess probe
returning the browser-process `None` → `set_sandbox_route(Bootstrap)` all
ran), then booted the ordinary sample-graph window with no errors in the
log. Windows Firewall prompted to allow "CEF Bootstrap Application" network
access — declined (Cancel), leaving firewall state untouched, since granting
it would be a system-security-settings change outside this task. This
receipt covers the process-role probe and bootstrap boot; it does not
additionally exercise `ensure_weld_engine`/`context.initialize` by pinning
`weld.chromium` in the running window (unchanged from the existing E2
receipt above, and covered by the new unit tests' route-selection logic
rather than a fresh headed capture here).

### E3. Activation model

**Smolweb packaging, checked 2026-11-19.** Turnstone links `mere-lanes/smolweb`
in its native build and registers the Nematic session engines at startup.
Standalone Pelt selects that lane at build time with `smolweb`. The Gemtext
receipt exercises a host-held document and link navigation through the
registered engine. Errand supplies protocol transport beneath the document
lane, in its own repository under engine-split D32.

The host plan's single native app can support separately installed engine
packs by attaching their processes through the same inventory as built-ins.
The built-in Smolweb viewers are still compiled in. E3 owns runtime enablement and
per-session selection, and the Verso charter owns carrying live state across
a swap. Installing a producer, choosing it for new content, and transferring
an existing session have separate completion conditions.

#### Local Gemini pack proof

The first pack is Gemini Gemtext. The standalone `turnstone-smolweb-pack`
executable lowers Turnstone's fetched body into Inker's `EngineDocument`.
Turnstone still owns Gemini trust, fetching, and the retained document pane.
The Inspector's Viewer picker offers the pack when installed and can select
it for a Gemini page. Auto continues to use the built-in lane.

Build the pack with `cargo build -p turnstone-smolweb-pack`. Put the built
executable beside `ports/smolweb-pack/pack.json` in a bundle directory. Run
`turnstone engine-pack install <bundle>` with a trusted local bundle and
`turnstone engine-pack remove smolweb` to detach it. Installation and removal
apply to the current Turnstone data root. The running native host refreshes
its picker on its next inventory poll.

The acceptance scenario installs and removes the pack while the native
window stays open. It checks that the pack renders a loopback Gemini page
with accessible links and that Auto can use the built-in lane after removal.
The proof uses the ordinary desktop process permissions and a versioned
local request. Pack distribution, the general enable set, and live state
transfer remain open.

**Windows receipt, 2026-11-19.** `scenarios/smolweb_engine_pack.scn` passed
in one native window. The picker marked the saved choice unavailable after
the worker was removed. The vacant bundle control could not select the
pack, and the invalid executable control reported a session spawn failure.
Raw output and captures are under
`Code/testing/turnstone/receipts/smolweb-pack/`.

#### Separate Smolweb protocol and parser switches

The version 2 bundle offers Gemini and Spartan connections and Gemtext and
Markdown parsers. Turnstone admits each connection through its native fetch
port. The worker receives the fetched body and runs only the selected
parser. Gemtext and Markdown use the response media type, including when
Turnstone fetches the body over HTTP or HTTPS. Gemini and Spartan can default
to Gemtext when the response has no media type. Auto still uses built-in
viewers.

Use `turnstone engine-pack status smolweb` to list the switches. For
example, `turnstone engine-pack disable smolweb protocol spartan` blocks new
Spartan browse requests and submissions in this Turnstone profile.
`turnstone engine-pack disable smolweb parser markdown` turns off the pack's
Markdown viewer in Inspector. Use `enable` with the same arguments to turn
either back on. Changes apply while the window is open and survive restart
for the installed bundle. Removing the bundle removes its profile defaults.
Version 1 bundles retain the original Gemini/Gemtext path.

Done when a Gemini response still renders with Spartan off, a Gemtext
response can use the pack with both Smolweb protocols off when acquired over
HTTP, and disabling Markdown leaves Gemtext selectable while refusing
Markdown work in the pack.

**Windows receipt, 2026-11-20.** The Gemini and Spartan fixtures rendered
with the other protocol switched off. The HTTP fixtures rendered with both
protocols switched off and only the requested parser enabled. The live
Spartan switch kept its server at zero requests, and the enabled control
reached it. The live Markdown switch made its picker row refuse selection.
The version 1 and 2 bundles both passed install, status, enable, disable and
remove checks. See `Code/testing/turnstone/receipts/smolweb-pack/granular-headed-first/`.

The shared fetch actor takes Turnstone's protocol policy and reads it before
each Smolweb transport hop (Graphshell `9f43ea3f`). Page loads and binary
subresources share the redirect loop. Explicit submissions check admission
before sending, and their returned redirect targets go through ordinary
navigation.

The redirect gate counts connections at both ends. With Spartan off, an
allowed Gemini source may reply with a redirect, and the Spartan target must
receive zero connections. This includes switching Spartan off while the
Gemini response is in flight. With both protocols enabled, the redirect
must reach Spartan. A Gemini-to-Gemini redirect must still work with
Spartan off. The earlier `granular-redirect/` receipt captured the gap.

**Redirect receipt, 2026-11-20.** Windows reached the Gemini source and
made zero connections to disabled Spartan, including when Spartan was
switched off after the Gemini request. With both protocols enabled, the
Spartan target received the redirected request. An enabled Gemini-to-Gemini
redirect still rendered with Spartan off. The old build failed the blocked
case and connected to Spartan once. The two HTTP parser cases and direct
protocol cases also passed. The matrix passed again over Turnstone
`f143bf23`. See `hop-final-headed/` and `hop-broken-control/` under
`Code/testing/turnstone/receipts/smolweb-pack/`.

#### Smolweb controls in Settings

Settings shows the installed pack's individual protocol and parser switches.
**Profile default** updates the same values as the CLI. **This session** can
follow that default, turn the capability on, or turn it off. Turnstone saves
explicit overrides alongside the session and restores them when it reopens.
A new session inherits the current profile defaults. Other sessions keep
their own overrides. An override applies only while the installed pack offers
the capability. A saved override survives pack removal and takes effect again
if a bundle offering the capability is installed.

The fetch worker for the departing session retires at session switch. The new
worker reads the adopted session's effective protocol setting before each
hop. When an effective pack parser setting turns off, Turnstone rebuilds
only open pages using that parser. It uses the built-in parser for the same
format and the response it already holds, without requesting the page or
its subresources again. The saved Viewer choice stays on the node. If the
response or fallback is unavailable, the existing view stays visible and
Settings reports the reason. Turning a parser back on makes it selectable
again. Carrying live view state into the replacement remains with Verso.

Done when Settings can change a profile default and the CLI reports it, a
session can override that default in either direction, a second session
inherits independently, and returning to the first session restores its
choices. Verify that a session-level protocol change stops an in-flight
redirect and that the parser switches remain independent of connections.
For open-page rerouting, turn off each parser while a page uses it. The
built-in view must render the held response with zero additional requests,
and the other parser switch must leave the current view alone.

**Windows receipt, 2026-11-20.** Keyboard and pointer edits passed the
Settings tests. The native Settings run set the profile default to Spartan
Off, which the CLI confirmed, and set an explicit On override in the first
session. The second session made zero Spartan connections. Switching back
and restarting restored the first session's override. Turning Spartan off
in Settings during a pending Gemini redirect kept its target at zero
connections. The enabled control connected once. See
`settings-headed-first/` and `settings-headed-restart/` under
`Code/testing/turnstone/receipts/smolweb-pack/`.

**Windows reroute receipt, 2026-11-22.** Markdown and Gemtext fell back
to the built-in parser with zero additional HTTP requests. Switching the
other parser left the live view on its pack parser. Switching the target
back on kept the built-in view open and the saved Viewer choice in place.
The older executable kept using the disabled parser after Settings saved
the override. See `open-page-reroute-winit-headed/` and
`open-page-reroute-control-confirmed/` under
`Code/testing/turnstone/receipts/smolweb-pack/`.

The picker plan's decision 1 and 2: a global `EngineEnableSet` app setting
with per-session override, folded into routing's `is_available` closure.
Registered-but-disabled engines stay in the picker as switchable-off rows.
Under D1 the same set covers an engine that is installed as a separate
process: absent, installed and enabled are states of one row.

The first activation slice reads Pandect's existing `disabled_engines`
through Turnstone's application settings projection. New content routes
through Inker's availability predicate, and installed viewers disabled by
the global setting stay visible in Inspector. Internal pages, graph ingest,
external protocol dispatch and host-owned kept content keep their routes.
Generic engine controls and session overrides for the global enable set
remain open along with rerouting open content outside the Smolweb pack.
The Smolweb Settings section edits the installable pack's capabilities.

**Activation receipt, 2026-11-20.** Four routing tests and eleven engine
inventory tests passed on Windows. The new case projects Pandect's setting
through the app, routes past a disabled Reader pin to Genet, and leaves
Reader visible as a disabled choice. This slice has unit-level evidence.

Done when: disabling an engine globally reroutes existing content through
the next rule, and a session override brings it back for that session only.

### E4. No-handler legibility

`host.external-protocol` stays the fallback; make it legible in Turnstone:
route degradation surfaces on the node (not a silent blank), and the picker
offers "open externally" for schemes nothing handles. This is also where the
Reticulum and eepsite lanes will land later as engines or handlers, so the
fallback UX is the seam they arrive through.

Done when: an unhandled scheme shows a labeled fallback state and the
degradation is visible in the observation snapshot.

## Not in scope

- **The verso flip** (carrying live state across an engine swap). Verso
  charter work, sequenced after the picker per that charter.
- **wasm builds of tier-2 engines.** Surface engines are native-only by
  contract; the wasm build keeps tier 1 and the disabled rows.
- **New engines.** This plan wires selection and the three kinds; adding a
  fourth engine is E0's one-step registration from then on.

## Ordering

E0 then E1 are small and unblock the design-to-shape ask immediately. E2 is
the heavy item and is independent of E1. E3 and E4 ride on E0's picker and
can land in either order after it.
