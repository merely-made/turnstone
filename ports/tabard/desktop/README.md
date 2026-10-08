# Tabard desktop

This thin native host mounts `tabard-workshop`'s shared Cambium surface and
`WorkshopState` directly. It provides the window, keyboard and pointer routing,
native text/IME integration, and Mesquite's scenario/capture lifecycle.
The shared graph leaf is registered with the host's leaf map. The reader scene
uses a texture producer and the host's existing render core/device, including
its retained texture, resizing and library-reopen invalidation. Neither
specimen creates another renderer or product model.

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

The windowless harness uses the same state, view, initialization, text seam and
host options as the native window. Its native text seam is verified by editing
the draft name through injected keyboard input and discarding that edit. A
narrow-window test delivers a wheel gesture through the host's production
scrolling path and checks that the specimens enter the visible body:

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
