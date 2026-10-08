# Tabard macOS native visual acceptance — 2026-10-08

**Accepted on macOS 15.8.1, x86_64, device scale 2.** Three native scenario runs
report `RESULT ok`, with 192 successful presentations and 15 nonblank captures.
All 15 images were visually inspected. Captures use the shared host's production
GPU readback path and successful-presentation accounting.

Source: published Mere `7d2a5f3cfe97174058368073d2bae809fffbf902`.
Built with `cargo build -p tabard-desktop --locked --offline`.
Binary SHA-256:
`f5f9e7b0838b818eb045340c2afc23946f17acaa7bf414a833b1cdb4a2063d27`.

| Lane | Scenario | Presentations | Redraws | Captures | Blank |
| --- | --- | ---: | ---: | ---: | ---: |
| [Shared components](shared/scenario.done) | shared_components.scn | 79 | 81 | 9 | 0 |
| [Authoring usability](usability/scenario.done) | usability.scn | 95 | 97 | 5 | 0 |
| [Fresh-process reopen](reopen/scenario.done) | usability_reopen.scn | 18 | 23 | 1 | 0 |

Shared components uses a fresh library. Usability uses another fresh library;
fresh-process reopen reuses the latter. No real authored library is touched.
Initial size is 1180×800 logical pixels; the scenarios resize the actual window
to 640×780. Device scale 2 produces 2360×1600 and 1280×1560 readbacks.

The [shared runner](../../../../../../scripts/run_macos_scenario.py) launches a copy
of the built executable in a temporary unsigned application bundle through
macOS LaunchServices (`open -n -W`). This supplies normal application launching
without changing the host, renderer or occlusion handling. Startup's unpresented
redraws are recorded and never advance the scenario. The runner requires a
successful launcher exit and Mesquite's exact `RESULT ok` receipt. Earlier
background-child launch failures remain in [presentation_wait](../presentation_wait/README.md).
This successful run qualifies this macOS launch path, not signed release packaging.

## Visual findings and limits

All four canonical mode previews render with neutral editor controls retained.
The shared reader has a bold heading and regular body; syntax uses distinct
Illume spans. Graph nodes, edges, bold selection labels and selection rings align.
Keyboard seed editing changes the authored color, and Undo restores it. Narrow
layout stacks the specimens and reaches reader/graph through the normal host
scroll-into-view route, with the action header intact.

The usability frames show entered `#2F7FFF`, authored high-contrast dark mode,
exact CSS applied to the isolated application preview, saved/reopened content,
clear-to-derived appearance and a narrow layout. Fresh-process reopening restores
the authored CSS, primary seed, selected mode and clean saved state. No blocking
visual defect was found. Scroll captures intentionally crop portions outside the
viewport; they do not claim the entire long editor fits at once.

These are rendered application-area captures. Native OS decorations, live
screen-reader/IME behavior, native chooser panels, Windows Snap and Windows/Linux
headed acceptance remain unverified by this matrix. This is not a formal contrast
audit. Other products and the browser fetch/host-loading work retain their own
acceptance boundaries.

## Reproduction

From the Mere root after the locked build, set `BINARY` to the executable and
`EVIDENCE` to a new output directory. Use a temporary library directory:

```sh
scratch=$(mktemp -d)
python3 scripts/run_macos_scenario.py --binary "$BINARY" --prefix TABARD \
  --scenario ports/tabard/desktop/scenarios/shared_components.scn \
  --output "$EVIDENCE/shared" --env CAMBIUM_HOST_FRAME_TRACE=1 \
  -- --library "$scratch/shared.json" || exit 1
python3 scripts/run_macos_scenario.py --binary "$BINARY" --prefix TABARD \
  --scenario ports/tabard/desktop/scenarios/usability.scn \
  --output "$EVIDENCE/usability" --env CAMBIUM_HOST_FRAME_TRACE=1 \
  -- --library "$scratch/authored.json" || exit 1
python3 scripts/run_macos_scenario.py --binary "$BINARY" --prefix TABARD \
  --scenario ports/tabard/desktop/scenarios/usability_reopen.scn \
  --output "$EVIDENCE/reopen" --env CAMBIUM_HOST_FRAME_TRACE=1 \
  -- --library "$scratch/authored.json" || exit 1
rm -rf "$scratch"
```

Each lane preserves full `native.log`, `launcher.log`, Mesquite `scenario.done`
and runner `process.json`, including executable and scenario hashes. JSON is
runner provenance rather than a second Mesquite outcome. Absolute capture paths
in raw receipts identify the original output directory. Every copied artifact is
byte-identical to its original; [artifact-sha256.json](artifact-sha256.json)
records all artifact hashes.
