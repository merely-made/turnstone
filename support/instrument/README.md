# Instrument qualification tools

Maintained tools are separate from the frozen October 12 packet, now at
`Code/testing/mere/support/receipts/instrument_workspace_20261012/`. The locator
`Code/testing/mere/receipt-relocation-2026-11-04/batch-07-reader.md` records original
tool bytes and current entry points. Never execute the tools in a frozen packet.

Every command takes an explicit `--repo` and a new `--output` outside the source
checkout and its input trees. Existing output directories are refused. Retain
failed runs separately. Default retained-byte cap is 100 MB per command; adjust
`--max-bytes` only as the lane brief requires. Commands do not install dependencies.

1. `python support/instrument/run-cpu.py --repo REPO --target EXISTING_TARGET --output NEW_CPU`
   renews the bounded CPU/configuration gates with the checkout's toolchain and
   the existing stable Cargo target, including its `build` directory. This is
   separate from browser/native execution. For a machine's mandatory launcher,
   pass `--cargo-wrapper BUILD_STORAGE_PY --cargo-lane EXISTING_LANE
   --wrapper-python LAUNCHER_PYTHON`. Web checks run from the standalone web
   crate so its committed target configuration applies; current scopes are
   `presentation-webgpu` and `product,applets,presentation-webgpu`. Independent
   gates continue after a failure, and the final exit remains nonzero.
2. Build the web site/package and native binary under their owning lane. Run
   wasm-bindgen as that build requires before freezing; supply the complete site
   with `workspace.html`, `pkg/graphshell_web_bg.wasm` and scenario assets.
   `python support/instrument/freeze.py --repo REPO --web BUILT_SITE --native-binary BUILT_BINARY --build-source FULL_COMMIT --compiler BUILD_COMPILER --output NEW_PACKAGE`
   copies and hashes those artifacts. It checks committed source inputs and
   records the caller-declared build identity. It does not prove or run a build.
3. In an owned, unlocked macOS headed lane:
   `node --require ./support/instrument/observe-browser.cjs support/instrument/verify-browser.cjs --repo REPO --package NEW_PACKAGE --output NEW_BROWSER --playwright MODULE_PATH`
   retains the eight existing scenarios and real AMD adapter constraint. Node
   and Playwright must already be available. `--channel` defaults to `chrome`.
   The eight-minute run deadline and bounded close attempts remain separate.
   Each scenario can use up to five minutes within that overall limit. The
   observer records the launched Chrome child, sampled descendants, actual
   exit, private-profile removal and reuse of the owned listener port. It does
   not close resources. Forme action navigation uses today's Map/Read controls
   and Panes/Read selected; the original assertions, captures and numeric
   controls remain intact, with both script hashes and the exact adaptation
   retained. Inspector and Graph controls are opened through public controls
   before each initial/reopened baseline.
4. In a separately owned, unlocked macOS native lane:
   `python support/instrument/run-native.py --repo REPO --package NEW_PACKAGE --output NEW_NATIVE`
   runs the two source-bound cases serially with fresh authored libraries,
   120-second scenario deadlines and a 300-second inhibitor ceiling.
   `--launcher MAINTAINED_SCRIPT` explicitly selects a newer harness when the
   frozen product source differs; its actual hash is recorded. The launcher
   keeps its temporary application bundle until the observed owned app exits,
   even when macOS `open -W` returns early. Timeouts retain bounded samples and
   terminate only the exact owned executable. A surviving app retains its bundle.
5. `python support/instrument/collect.py --repo REPO --package NEW_PACKAGE --browser NEW_BROWSER --native NEW_NATIVE --output NEW_COLLECTION`
   verifies candidate bindings, source/package hashes, actual scenario outputs,
   PNGs, neutral-frame pixels, actual process/profile/port and bundle release,
   executable/scenario hashes and explicit release records. Pillow must already
   be installed. The expected adapter remains `amd` / `gcn-5`; the two named
   options permit another lane to declare its own scope explicitly. Collection
   copies actual supplied runs and does not perform a replay. It does not copy
   October's hardcoded failure history into a new qualification.

`audit_docs.py --repo REPO --baseline REF --output NEW_AUDIT` compares changed
document contents in the same checkout. Added/deleted files require a full-tree
comparison and are explicitly refused by this read-text-only helper.

Q465: the maintained browser driver now records rejected, still-open and timed-out
closes as failures; collection requires the versioned release record and all
three successful closes. On 2026-11-04, the exact published finalizer also passed
five source-bound success/failure controls, and the published release helper
passed one owned headless Chrome/macOS contract probe: actual context/browser/HTTP
closure, exit of all seven owned process IDs, removal of the temporary profile
and reuse of its loopback port. The original false-success defect is repaired;
this probe does not retroactively validate October's unconditional booleans.
Receipts and exact runtime/source hashes are in
`Code/testing/mere/closure-audit/batch-08-q465-owned-headless-probe.json` and
`batch-08-q465-finalizer-controls.json` in the same directory.

On 2026-11-05, fresh WebAssembly/product and native packages at
`1c064dfeff73e352c8e0c80fd92913c688407f2b` passed all eight headed Chrome/AMD
WebGPU cases, both macOS native scenarios, collection and individual review of
all 20 PNGs. Actual release includes all nine observed browser processes gone,
temporary profile removal, listener-port reuse, both native processes gone,
temporary bundles removed and the inhibitor closed. Q465's bounded Instrument
pilot and closure qualification are complete at that source. The finalizer's
five controls, release helper's five controls and nine Python controls pass.

The [checkpoint](../../../../testing/mere/closure-audit/batch-11-checkpoint.md)
binds successful builds, frozen packages and execution receipts. The earlier
29 CPU tests, native workspace Clippy and viewer check at `4c82462ec` are reused
only for unchanged input cohorts, with Git-object proof; the owning product
WebAssembly Clippy and complete 1,437-package metadata audit were renewed at
`1c064dfef`. The separate repository header audit still fails with 268 missing
Exhibit A notices. No repository-wide licensing pass is claimed.

Browser reading panes use seed nodes without document bodies; those captures
qualify shell layout and Forme persistence, not browser article rendering.
Native authored stylesheet and reading specimens have their own captures.
Physical pointer, human AT, cross-app interoperability, other consumers and
other platforms retain their own gates. October's missing Wasm/native inputs
remain historical; these fresh packages do not relabel that evidence. Later
integration commits are outside this exact-source runtime receipt.

Controls: `python -m unittest discover -s support/instrument -p test_paths.py` and
`node --test support/instrument/test_release.cjs`. Also run
`python -m unittest discover -s support/instrument -p test_native_launcher.py`
for early-launcher-return and exact-owner controls, and
`node support/instrument/test_finalizer.cjs` for the maintained finalizer's five
source-bound positive/negative controls. These controls do not start a browser
or native application; the Node positive control closes one owned loopback server.
