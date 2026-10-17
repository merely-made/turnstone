# Headed scenario receipts

`run-headed-receipt.py` uses the adjacent `receipt_sink.py` by default. The runner keeps its existing browser options, page geometry, capture handling, timeout, and cleanup behavior. Supply its existing required `--web`, `--out`, `--scenario`, `--query`, and `--node` arguments as before.

An optional `--origin http://127.0.0.1:PORT` selects a separate local page gateway. When the selected sink resolves to the tracked `receipt_sink.py`, the runner passes that exact page origin through `--allowed-origin`. The sink admits JSON POSTs and their preflight only on `/scenario-progress` and `/scenario-receipt`, only from that origin. It does not use wildcard origins or credentials. Without a separate gateway, the allowed origin is the sink's own loopback port.

`--sink PATH` retains support for an explicit external sink. If it resolves to another file, the runner keeps the previous sink command arguments and does not pass `--allowed-origin`. A custom sink serving a separate gateway must already provide compatible receipt handling; this runner does not assume it supports the tracked helper's origin option.

For direct helper use:

```bash
python3 receipt_sink.py --web WEB_DIRECTORY --out RECEIPT_DIRECTORY \
  --port 19940 --allowed-origin http://127.0.0.1:19930
```

The receipt parser, progress file, scenario result, decoded captures, and `scenario.done` format are unchanged. This helper only delivers receipts; successful receipt transport does not itself qualify a scenario, GPU, accessibility, or presentation result.


## Browser presentation qualification

Everything the WebGL presentation lane's qualification needs is in this directory: `presentation-webgl-fixture.html` (the actual packaged viewer under test), `run-presentation-webgl.py` (one browser route per run), `compare-presentation-captures.py` (the cross-backend comparator) and `presentation-mount.mjs`. Nothing else has to be copied in.

The fixture has three modes, chosen by the runner's `--mode` (the `?mode=` query):

- `presentation` (default): scenario picking and keyboard actions, three captures (`before-loss`, `after-restore`, `odd-resize`), WebGL2 context loss and restoration when `--force-context-loss` is set, zero-extent suspension and removal/remount. The route under test is `--backend webgl2|webgpu|auto`.
- `compare`: the same sequence with `data-reduced-motion` set so both backends draw the same frame, and the WebGPU route takes the graph-command click that the WebGL2 route makes during context loss, so both reach the same state before each capture. It also records the WebGPU adapter, console warnings and errors, the root's disclosed state, and each capture's extent and last pixel. The last pixel is checked against the background at device scale 1; at any other scale it only has to be opaque. Run WebGL2 with `--force-context-loss` and WebGPU without it. Captures from `presentation` mode on the two backends are not comparable, because the sequences differ.
- `auto-choice`: with `--backend auto`, checks that Auto loads the WebGPU artifact (`data-viewer-artifact`) and presents on WebGPU when a device exists. It needs a Vulkan device, so pass `--angle vulkan`.

`run-presentation-webgl.py --web <site> --out <new directory>` serves `<site>` (the fixture as `index.html`, `mount.js`, and `pkg/{webgpu,webgl2}/`), launches one browser, writes `result.json`, `summary.json`, the decoded `*.png` captures and `launch.json`, and exits nonzero unless the fixture passes. It requires the browser to be in the runner's own cgroup, so launch it under a memory-capped scope (below). `--device-scale-factor N` passes `--force-device-scale-factor=N` (Chrome only). `--browser firefox` selects Firefox, `--disable-webgpu --backend auto` the fallback, `--angle gl|vulkan|swiftshader` the ANGLE backend (default `vulkan` for `--backend webgpu`, otherwise `gl`), and `--port` the loopback port (default 8975, so run routes one at a time).

### Environment

- Hardware GL routes need the desktop display. From an SSH or bare tty session, set `DISPLAY=:0` and the session's Xauthority, which on this machine is `/run/user/$UID/.mutter-Xwaylandauth.*`; without them ANGLE cannot open a display and the page never becomes ready.
- Vulkan WebGPU needs X11: leave `WAYLAND_DISPLAY` unset (`env -u WAYLAND_DISPLAY`). Chrome is the Flatpak build at `/var/lib/flatpak/app/com.google.Chrome/...`; Firefox is `/usr/bin/firefox`.
- Builds and browsers are heavy. Run each one at a time under `nice` in a memory-capped scope, for example `systemd-run --user --scope --quiet -p MemoryMax=9G -p MemorySwapMax=0 nice -n 19 COMMAND`. The runner asserts that the browser shares its cgroup, so wrap the runner itself, not only the browser.
- Needs `node` (Terser, `presentation-mount.mjs`) and Pillow (the comparator). Keep physical-GPU and SwiftShader results separate.

### Sequence

Run from the repository root. `$OUT` is a new directory outside the tree.

```bash
GUARD='systemd-run --user --scope --quiet -p MemoryMax=9G -p MemorySwapMax=0 nice -n 19'
WEB=ports/graphshell/web
T=$WEB/tests
OUT=$HOME/Code/testing/mere/presentation-$(date +%Y%m%d)
mkdir -p $OUT
export DISPLAY=:0 XAUTHORITY=$(ls /run/user/$UID/.mutter-Xwaylandauth.* | head -n1)
unset WAYLAND_DISPLAY
```

1. Package both route artifacts with the pinned tools: `wasm-bindgen-cli` 0.2.129 (`cargo install wasm-bindgen-cli --version 0.2.129 --locked --root <tools>`) and Terser 5.51.2 (`npm ci --ignore-scripts` in `$WEB/scripts`, then `$WEB/scripts/node_modules/.bin/terser`). The build refuses unless every cap passes (Wasm 3,670,016 B gzip per route artifact, glue 20,480, mount 8,192, history 131,072); it writes `viewer-delivery.json` beside the artifacts.

   ```bash
   $GUARD python3 $WEB/scripts/build-viewer.py --out-dir $OUT/pkg \
     --wasm-bindgen <tools>/bin/wasm-bindgen --terser $WEB/scripts/node_modules/.bin/terser
   ```

2. Assemble the site once; both modes read the same fixture:

   ```bash
   mkdir $OUT/site && cp $T/presentation-webgl-fixture.html $OUT/site/index.html
   cp $WEB/mount.js $OUT/site/ && ln -s $OUT/pkg $OUT/site/pkg
   ```

3. The six routes. Each must print `"result": "pass"` (33 controls on the WebGL2 routes, 27 on WebGPU, 3 on the Auto choice):

   ```bash
   run() { name=$1; shift; $GUARD python3 $T/run-presentation-webgl.py --web $OUT/site --out $OUT/$name "$@"; }
   run chrome-webgl2        --backend webgl2 --force-context-loss
   run chrome-auto-nowebgpu --backend auto --disable-webgpu --force-context-loss
   run firefox-webgl2       --browser firefox --backend webgl2 --force-context-loss
   run chrome-swiftshader   --backend webgl2 --angle swiftshader --force-context-loss
   run chrome-webgpu        --backend webgpu
   run chrome-auto-webgpu   --backend auto --angle vulkan --mode auto-choice
   ```

4. The same-state comparisons, at device scale 1 and 1.25. `compare-presentation-captures.py` takes the WebGL2 directory, then the WebGPU directory, and exits nonzero unless each of the three captures matches in size and holds the contract: full-frame RGB RMS at most 1.0, maximum RGB delta at most 16, alpha delta 0.

   ```bash
   run n-gl --mode compare --backend webgl2 --force-context-loss
   run n-wg --mode compare --backend webgpu
   python3 $T/compare-presentation-captures.py $OUT/n-gl $OUT/n-wg
   run f-gl --mode compare --backend webgl2 --force-context-loss --device-scale-factor 1.25
   run f-wg --mode compare --backend webgpu --device-scale-factor 1.25
   python3 $T/compare-presentation-captures.py $OUT/f-gl $OUT/f-wg
   ```

Recorded results on the ThinkPad (AMD Radeon Renoir, Mesa RADV), 2026-10-15, from the committed pieces alone: the six routes pass (33 controls on WebGL2, 27 on WebGPU, 3 on the Auto choice); both comparisons pass with alpha 0 and a maximum RGB delta of 11, at RGB RMS 0.252/0.300/0.299 at scale 1 and 0.258/0.332/0.328 at 1.25. Compare-mode console capture shows `missing #scenario-result` errors on every run; the fixture has no such element and the scenario path does not need it.

`node ports/graphshell/web/tests/presentation-mount.mjs` checks policy precedence, invalid choices, removal/remount, a fetch completing after removal, the route artifact chooser and Auto's WebGL2 remount after a WebGPU-artifact boot refusal. The viewer accepts `data-render-backend` or overriding `?render_backend=` with `auto`, `webgpu` and `webgl2`; selected backend, context generation and compute availability are disclosed on the element. Explicit `?gpu=on` refuses on WebGL2.

Packaging uses `npm ci --ignore-scripts` in `web/scripts` and the pinned Terser CLI (`--terser <path>`). The viewer ships one artifact per route, `pkg/webgpu/` and `pkg/webgl2/` (the latter built `without-classic-vello`); `mount.js` loads exactly one, by explicit policy or, under Auto, by an actual WebGPU device probe, and remounts on WebGL2 if the WebGPU artifact refuses at boot. Each artifact's Wasm and glue is held to the unchanged gzip caps; the loaded artifact is disclosed as `data-viewer-artifact`. See the dated presentation receipts for exact measurements and remaining qualification gates.
