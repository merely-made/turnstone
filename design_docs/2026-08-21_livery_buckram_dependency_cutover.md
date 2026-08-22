# Livery and Buckram dependency cutover

**Status:** done, 2026-08-21.

The removed `genet-layout` facade was not recreated. Its consumers now use the
public Livery/Buckram stack at the authority boundary appropriate to each host.

## Landed boundary

- Turnstone owns a retained `LiverySnapshot` adapter over `StylePlane`,
  `LiveryLayout`, and `TextSystem`. It retains pane rebuild and scroll state,
  performs translate-aware geometry and hit testing, inserts host-painted
  custom leaves, and decodes host images without a retired layout helper.
- Mere Canvas retains `LiveryDocument<ScriptedDom>` and asks it for each frame's
  paint list. Canvas remains responsible for graph state and scene composition.
- Graphshell web asks `genet-render` to build its scene from the scripted DOM.
- The root consumer patches retain Genet's Taffy fork, which Livery/Buckram
  require. The `stylo_taffy` and `genet-layout` patches and dependencies are
  removed.

## Published revisions

- Turnstone: `b848349` (`e506c64` is the functional cutover; `b848349` restores
  explicit neutral-DOM imports in test modules).
- Mere: `04e303ac`.
- Genet dependency baseline: `b9457041`.

All were published directly to `main`.

## Gates

- `cargo test -p mere-canvas --lib`: 179 passed.
- `cargo check -p graphshell-web --target wasm32-unknown-unknown`: passed.
- Clean archive of Turnstone `b848349`, with no local Cargo overrides:
  `cargo check --tests`: passed against Mere `04e303ac` and Genet `b9457041`.
- The clean dependency tree contains `genet-livery 0.0.2` and `buckram 0.0.1`,
  with zero `genet-layout` or `stylo_taffy` package lines.
- Focused Turnstone suites for UI layout and scrolling, chrome, Apparatus,
  Cambium, Inspector, Workbench, Knot authoring, Publish, and Settings: 47
  passed, 5 environment-gated Knot tests ignored, 0 failed.

The first local Turnstone test attempt collided with an unrelated in-progress
Knot resident-source edit in the Mere working tree. That edit was left intact.
The clean archive gate separates the published dependency result from that
local source lane.

## Remaining boundary note

Turnstone currently applies its own translate-aware rectangle and hit-test
adjustment because its window forest uses CSS translate. General transform-aware
Livery hit testing belongs in Genet if consumers need more than Turnstone's
translate-only host convention. It is not a dependency-cutover blocker.
