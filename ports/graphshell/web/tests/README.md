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


Browser presentation controls use `presentation-webgl-fixture.html` against the packaged viewer. `run-presentation-webgl.py --web <fixture directory with index.html, mount.js and pkg/{webgpu,webgl2}/> --out <new directory> --backend webgl2 --force-context-loss` runs the retained viewer's scenario/capture path. Add `--browser firefox --headed` for Firefox, or `--disable-webgpu --backend auto` for fallback. Run every browser inside the machine's owned memory-capped heavy-job guard; keep physical and software results separate.

`node ports/graphshell/web/tests/presentation-mount.mjs` checks policy precedence, invalid choices, removal/remount, a fetch completing after removal, the route artifact chooser and Auto's WebGL2 remount after a WebGPU-artifact boot refusal. The viewer accepts `data-render-backend` or overriding `?render_backend=` with `auto`, `webgpu` and `webgl2`; selected backend, context generation and compute availability are disclosed on the element. Explicit `?gpu=on` refuses on WebGL2.

Packaging uses `npm ci --ignore-scripts` in `web/scripts` and the pinned Terser CLI (`--terser <path>`). The viewer ships one artifact per route, `pkg/webgpu/` and `pkg/webgl2/` (the latter built `without-classic-vello`); `mount.js` loads exactly one, by explicit policy or, under Auto, by an actual WebGPU device probe, and remounts on WebGL2 if the WebGPU artifact refuses at boot. Each artifact's Wasm and glue is held to the unchanged gzip caps; the loaded artifact is disclosed as `data-viewer-artifact`. See the dated presentation receipts for exact measurements and remaining qualification gates.
