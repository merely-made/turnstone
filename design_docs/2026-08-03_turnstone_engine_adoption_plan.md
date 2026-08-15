# Turnstone engine adoption — arbitrary engines, selectable in the app

**Date:** 2026-08-03
**Status:** scoped with Mark. The ask: arbitrary selection of engines,
possible and selectable within the app, including genet with its rungs
(placeholder where a rung cannot spawn yet, best effort, for testing and
design-to-shape purposes).

**Authority relationship:** the
[engine picker + pluggability plan](../../mere/design_docs/inker_docs/implementation_strategy/2026-06-15_engine_picker_and_pluggability_plan.md)
(mere, 2026-06-15) owns the model: three activation levels collapsing to one
`is_available` predicate, global default + per-session override, cargo
features as the build tier, no-handler legibility, and the picker/flip
ownership split with verso. Its shipped phases (0 to 3) landed in meerkat,
and meerkat is deleted, so the receipts are gone while the model stands.
Turnstone independently re-landed the foundation. This plan binds the
remaining model to Turnstone's actual state; it does not re-decide it.

## Current state (verified 2026-08-14)

- **Routing**: the content lane routes through `inker::EngineRoutePolicy`
  with `pinned_engine` support ([effects.rs](../src/shell/effects.rs)).
- **Session engines registered**: `genet.web` (StaticSessionEngine),
  `genet.livery` (LiverySessionEngine), and the Knot authoring engine, in a
  `SessionRegistry<netrender::Scene>` ([mod.rs](../src/shell/mod.rs)).
- **Picker**: the Apparatus pane's viewer radio respawns live content
  through the pinned engine. `VIEWER_OPTIONS` has Auto, genet.web, and
  genet.livery; a Windows `--features weld` build adds weld.chromium.
- **Surface engines**: the Windows Weld first cut is now wired behind
  `--features weld`: an inker `SurfaceEngineRegistry`, `weld.chromium`
  producer map, D3D12 transferred-handle import cache, primary-window
  composition and pointer/key routing. Scrying and graft remain unregistered.
  [2026-07-18_meerkat_harvest.md](2026-07-18_meerkat_harvest.md) and git
  history are the donors.
- **Compositor**: the shell already composes per-surface textures via
  `netrender` `compose_external_texture` ([render.rs](../src/shell/render.rs),
  [lens.rs](../src/shell/lens.rs)), so a GPU `SurfaceFrame` is another
  external-texture layer, not a new compositor concept.
- **Scripted lane**: `genet_documents::ScriptedSessionEngine` exists;
  Turnstone's `piccolo` feature pulls the script-engine crates but registers
  no scripted content engine.
- **Features**: `wasm` and `piccolo` only. No engine features.

## The model, restated for Turnstone

Three engine kinds, two of them live here today:

| Kind | Registry | Output | Turnstone state |
|---|---|---|---|
| Document | `EngineRegistry` | `EngineDocument` blocks | via nematic lanes (cards/capture) |
| Session | `SessionRegistry<Scene>` | paint scenes | live: static, livery, knot |
| Surface | `SurfaceEngineRegistry` | GPU texture stream | Windows Weld first cut behind `weld`; scrying and graft absent |

"Genet with its rungs" means the genet engine's capability ladder is exposed
as selectable lanes rather than one opaque entry: `genet.web` (static DOM +
stylo lane), `genet.livery` (clean-room CSS/layout lane), `genet.scripted.*`
(Boa/Nova/piccolo scripted DOM), and later rungs as they exist. A rung that
cannot spawn yet still appears in the picker as a disabled row naming why
(feature off, not registered, platform gap). That is the design-to-shape
placeholder: the selection UI carries the full ladder honestly instead of
hiding the unbuilt parts, and the no-placebo rule holds because a disabled
row never pretends to spawn.

## Steps

### E0. Registry-driven picker

Replace the hardcoded `VIEWER_OPTIONS` array: the Apparatus viewer enumerates
engine ids from the registries the shell actually holds, plus declared-but-
unavailable entries with their reason. Auto stays row zero. The picker shows
kind (document/session/surface) so a surface pick reads as the black-box tier
the fidelity axis says it is.

Done when: registering an engine in `shell/mod.rs` is the only step that adds
it to the picker, and an unavailable rung renders as a disabled row with a
reason string.

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

One cargo feature per crate (`scrying`, `graft`, `weld`), per the picker
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

Done when: with `--features scrying`, pinning `scrying.web` on a node shows
the system WebView's texture composited in that node's tile on Windows, and
a scenario receipt captures it; graft and weld repeat the shape (their
producers may land later, each behind its feature, disabled rows until then).

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
permissions, auth, popup placement, and snapshots remain unsupported here:
Weld has many of those operations, but Turnstone has not yet provided their
shared contract, callback/control UI, or durable policy.

The tail now follows Genet's
[web-platform host contract](../../genet/docs/2026-08-14_web_platform_host_contract_plan.md).
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

### E3. Activation model

The picker plan's decision 1 and 2: a global `EngineEnableSet` app setting
with per-session override, folded into routing's `is_available` closure.
Registered-but-disabled engines stay in the picker as switchable-off rows.

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
