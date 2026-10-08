# Shared workshop and title-bar receipts — 2026-10-08

The standalone Tabard desktop now mounts `crates/cambium/tabard-workshop`,
Cambium's four-slot title bar, native caption adapters and the native host's
shared `SceneProducer<T>`. These receipts cover that integrated source.

## Passed checks

All 285 tests pass: 70 Tabard, 40 workshop, 10 desktop and 165 native-host
windowless tests. The host total includes six new title-bar tests and two scene
callback/rebinding tests. The desktop adds a caption Close test proving that
the host's real command queue reaches the workshop's unsaved-work policy.

The title-bar tests exercise pointer commands, Tab/Enter/Space activation,
drag/no-drag regions, configurable labels, platform caption policy, and left
and right native reservations plus minimum height. The broader host tests
include existing decoration, input, scrolling, file and accessibility paths.

Commands, using `CARGO_TARGET_DIR=/Users/markik/Code/targets/mere-pelt-host-load`:

```sh
cargo test --offline -p tabard -p tabard-workshop -p tabard-desktop \
  -p cambium-genet-winit-host --lib --tests
cargo test --offline -p cambium-genet-winit-host --test title_bar
cargo clippy --offline -p tabard-workshop -p tabard-desktop \
  --all-targets --no-deps -- -D warnings
cargo clippy --offline -p cambium-genet-winit-host --lib --test title_bar --no-deps
cargo build --offline -p tabard-desktop
python3 scripts/check_port_boundaries.py
```

Strict Clippy passes for the workshop and desktop. Native-host Clippy completes
with seven inherited warnings in decorations, windows and the existing idle
redraw path; no warning points to the new components. Scoped formatting,
workspace metadata, README links and diff checks pass.

## Native macOS evidence

`scenarios/usability.scn` passes with 95 scenario/redraw ticks and five captures,
all nonblank with distinct digests. `usability.done` preserves the receipt;
the five PNGs here are unchanged copies. Wide captures are 2360 × 1600 physical
pixels; `usable_narrow.png` is 1280 × 1560 (640 × 780 logical). The header
reserves native traffic-light space and keeps authoring actions visible in the
narrow layout. Render captures contain app pixels; the operating system's
traffic-light pixels are outside this capture seam.

The controls enter `#2F7FFF`, set high-contrast dark as the authored default,
apply exact CSS, save, reopen, clear the override, undo and resize. The retained
`themes.json` and workshop sidecar are test fixture data from an explicitly
separate `/tmp/tabard-stack-native-20261008` library.

## macOS presentation acceptance

The subsequent [LaunchServices matrix](launchservices/README.md) passes on the
published `7d2a5f3cfe97174058368073d2bae809fffbf902` source: 192 actual
presentations and 15 nonblank captures across shared components, usability and
fresh-process reopen. All images were visually inspected. Normal application
launching succeeds with the same production host and rendering path; no
occlusion override or presentation-counter bypass was added.

## Historical background-launch presentation boundary

The separate fresh-process scenario passes its restoration assertions
(`hc_dark`, `#2f7fff`, clean draft), but its visual capture does not complete.
`reopen.done`, `reopen-retry.done` and `reopen-resized.done` retain the failed
receipts. A diagnostic with an actual 920 × 840 resize and longer settling also
fails (`reopen-resize-diagnostic.done`, 173 redraw attempts, zero captures).
A fresh empty-library control fails the same way
(`fresh-resize-diagnostic.done`, 157 attempts, zero captures). Both diagnostic
state checks pass, and the saved authored library remains unchanged.

In these runs wgpu surface acquisition repeatedly returns `Occluded` before
drawable acquisition/presentation. The display was awake; the evidence does not isolate an
OS/session issue from intermittent AppFrame startup. The prior five-capture
pass establishes that this configuration can render, not that fresh-process
presentation is reliable in the current session.

The shared host now distinguishes successful presentation identities from
redraw attempts. Mesquite pauses native scenario progress, settling and capture
grace while acquisition is unsuccessful, polls asynchronous readback on every
turn, and uses a separate ten-second continuous presentation deadline. The
subsequent fresh, saved and native-state diagnostic runs record zero
presentations and zero captures; their complete failed receipts, structured
JSON projections, concise logs and owned-window findings are preserved in
[presentation_wait](presentation_wait/README.md). macOS reports an inactive
application and an occlusion state without the Visible bit despite valid,
visible window geometry on the active Space. A one-time focus experiment was
ineffective and removed. This fixes receipt/progression accounting; reliable
fresh-process native presentation was unverified in those attempts. The later
LaunchServices matrix above supplies that missing proof on macOS.

Windows/Linux native frames, Windows Snap interaction, native chooser panels
and a live screen-reader session remain unverified. Their portable/native
command routing is covered by the windowless tests.

The documentation judgment audit retains its inherited snapshot digest,
duplicate browser-receipt README and missing D2 record errors (257/258 active
records). No new active design document was introduced.
