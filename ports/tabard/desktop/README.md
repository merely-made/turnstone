# Tabard desktop

This thin native host mounts `tabard-workshop`'s shared Cambium surface and
`WorkshopState` directly. It provides the window, keyboard and pointer routing,
native text/IME integration, and Mesquite's scenario/capture lifecycle.
The shared package lives in
[`crates/cambium/tabard-workshop`](../../../crates/cambium/tabard-workshop);
its [embedding guide](../../../crates/cambium/tabard-workshop/README.md)
describes the reusable model and host seams.
The shared graph leaf is registered with the host's leaf map. Reader and
isolated application-stylesheet scenes share the shared `cambium_genet_winit_host::SceneProducer`
on the host's existing render core/device, including retained textures,
distinct raster keys, resizing and library-reopen rebinding. Only source callbacks and raster keys remain in this port. The portable
workshop retains the authoring state and document scenes. See the
[workshop README](../README.md) for the controls, export semantics and remaining
product boundaries.

The header uses Cambium's shared `title_bar` composition: Tabard supplies its
mark, title and authoring actions. `WindowFrame::App` enables the host's existing
drag, resize, double-click and system-menu behavior. macOS keeps native traffic
lights; the component reserves their measured inset. Windows and Linux receive
shared keyboard-accessible caption controls bound to the host's command queue.
Caption Close goes through the same unsaved-work policy as native Close.

Run from the repository root:

```sh
cargo run -p tabard-desktop
```

The authored library defaults to `mere/tabard/themes.json` under the platform's
local application data directory (`dirs::data_local_dir()`). On macOS this is
`~/Library/Application Support/mere/tabard/themes.json`. Choose a separate file
for experiments or a supplied library:

```sh
cargo run -p tabard-desktop -- --library /tmp/tabard-workshop/themes.json
```

The initial window is 1180 × 800 logical pixels. `TABARD_WIDTH` and
`TABARD_HEIGHT` override its size. The workshop's Save and Reopen controls own
the authored-file workflow; startup reports malformed or unreadable libraries.
The last saved user-theme selection and preview mode are remembered in a
separate file formed by appending `.workshop.json` to the selected library path.
Experiments with another `--library` therefore have their own editor preferences.

Import uses the host's existing native open-file chooser. Export uses a native
save-path chooser and sends the captured artifact back to the shared state for
publication and any replacement confirmation. `hooks_with_exporter` lets an
embedding or acceptance host inject destination selection without replacing
the writing or authoring logic. Ordinary window-close and application-close
requests use the workshop's unsaved-work confirmation; a failed Save and close
keeps the window visible. The host's focused-text seam covers the name, hex
color and multiline CSS fields, and post-dispatch synchronization updates the
shared draft.

The windowless harness uses the same state, view, initialization, text seam and
host options as the native window. Acceptance tests route injected native text
to all three fields, return a real colliding theme file through `FileChooser`,
write captured exports to real temporary files, preserve Undo across export,
and exercise native/application close with invalid input and failed writes. A
narrow-window test delivers a wheel gesture through the host's production
scrolling path and checks that the specimens enter the visible body. These
windowless tests verify routing and file behavior; they do not verify GPU pixels
or a live platform screen-reader session:

```sh
cargo test -p tabard-desktop
```

For native frame captures, use a fresh library path and a desktop with a GPU:

```sh
TABARD_SCENARIO=ports/tabard/desktop/scenarios/workshop.scn \
TABARD_CAPTURE_DIR=/tmp/tabard-evidence \
TABARD_RECEIPT=/tmp/tabard-evidence/workshop.done \
  cargo run -p tabard-desktop -- --library /tmp/tabard-evidence/themes.json
```

That scenario uses actual controls, captures the initial view, seed edit, dark
preview, undo, discard, saved theme, explicit reopening and a 640 × 780 layout.
It rejects unchanged frame digests for the seed edit and mode switch. It reports failure
through the process exit status as well as the Mesquite receipt. The scenario
requires a fresh library because its initial draft is unsaved.

On macOS, run headed scenarios through LaunchServices so the operating system
launches an active application. The shared launcher wraps the built executable
in a temporary unsigned application bundle, preserves the normal host/render
path, and requires both launcher success and Mesquite's `RESULT ok` receipt:

```sh
python3 scripts/run_macos_scenario.py \
  --binary target/debug/tabard-desktop --prefix TABARD \
  --scenario ports/tabard/desktop/scenarios/shared_components.scn \
  --output /tmp/tabard-shared-evidence \
  -- --library /tmp/tabard-shared-themes.json
```

Use a new output directory and fresh library path for the first run. For a
fresh-process reopen, reuse that library with the matching reopen scenario and
a new output directory. Product profile isolation for other consumers belongs
in repeated `--env KEY=VALUE` arguments. The launcher is shared Mesquite tooling
and does not change rendering or permit unpresented redraws to advance scenarios.

The shared-component scenario checks all four mode previews, graph selection,
a keyboard seed edit and undo, and the real reader and graph in the narrow
layout. Run `ports/tabard/desktop/scenarios/shared_components.scn` with the same
environment variables and a fresh library path. This exercises Illume spans,
the extracted reader and the graph swatch on the native GPU host; retained
tests separately assert their source, palette and event behavior.

Then open the saved library in a second process:

```sh
TABARD_SCENARIO=ports/tabard/desktop/scenarios/reopen.scn \
TABARD_CAPTURE_DIR=/tmp/tabard-reopened \
TABARD_RECEIPT=/tmp/tabard-reopened/reopen.done \
  cargo run -p tabard-desktop -- --library /tmp/tabard-evidence/themes.json
```

To inspect the stacked editor and previews at 640 × 780, run
`ports/tabard/desktop/scenarios/narrow.scn` against the same saved library. It
captures the editor, then uses ordinary selector clicks and the host's
scroll-into-view path to reveal the mode picker and application-chrome specimen.
Mesquite's neutral pointer queue currently has no wheel verb; the separate
windowless test above verifies the actual wheel path.

`key tab-until <attribute>=<value>`, `key Enter`, `key Space`, `key Tab`,
`key Shift+Tab`, `key ArrowRight` (and the other arrows), `key Escape` and
`text <characters>` supplement Mesquite's selector clicks. Scenarios contain
no direct product-state mutation commands. Set `MESQUITE_CAPTURE_PAINT=1` to
save a resource-preserving paint-list sidecar alongside each native PNG.

Authored theme persistence belongs to `tabard-workshop`; this package selects
the path and does not maintain a second theme model or registry.

The standalone usability scenario, `scenarios/usability.scn`, starts with a fresh
library and exercises RGB entry, an authored high-contrast dark default, exact
CSS, save/reopen, clear/undo and a narrow layout. Then run
`scenarios/usability_reopen.scn` against its saved library to check a second
process. The [2026-10-08 receipts](receipts/2026-10-08_usable) retain the native
images, complete receipts and importable sample definition.

The [shared-kit receipts](receipts/2026-10-08_stack) cover the relocated workshop,
shared scene adapter and title-bar adoption. The authoring/native narrow-layout
scenario passes. The [LaunchServices acceptance](receipts/2026-10-08_stack/launchservices/README.md)
now passes all four mode previews, shared reader/syntax/graph interaction,
actual resizing, exact CSS authoring and fresh-process native capture: 192
presentations and 15 nonblank images on macOS. Historical background-child
launch failures remain recorded with their zero-presentation receipts.
Windows/Linux headed behavior and live native accessibility remain unverified.
