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
   separate from browser/native execution.
2. Build the web site/package and native binary under their owning lane. Run
   wasm-bindgen as that build requires before freezing; supply the complete site
   with `workspace.html`, `pkg/graphshell_web_bg.wasm` and scenario assets.
   `python support/instrument/freeze.py --repo REPO --web BUILT_SITE --native-binary BUILT_BINARY --build-source FULL_COMMIT --compiler BUILD_COMPILER --output NEW_PACKAGE`
   copies and hashes those artifacts. It checks committed source inputs and
   records the caller-declared build identity. It does not prove or run a build.
3. In an owned, unlocked macOS headed lane:
   `node support/instrument/verify-browser.cjs --repo REPO --package NEW_PACKAGE --output NEW_BROWSER --playwright MODULE_PATH`
   retains the eight existing scenarios and real AMD adapter constraint. Node
   and Playwright must already be available. `--channel` defaults to `chrome`.
   The eight-minute run deadline and bounded close attempts remain separate.
4. In a separately owned, unlocked macOS native lane:
   `python support/instrument/run-native.py --repo REPO --package NEW_PACKAGE --output NEW_NATIVE`
   runs the two source-bound cases serially with fresh authored libraries,
   120-second scenario deadlines and a 300-second inhibitor ceiling.
5. `python support/instrument/collect.py --repo REPO --package NEW_PACKAGE --browser NEW_BROWSER --native NEW_NATIVE --output NEW_COLLECTION`
   verifies candidate bindings, source/package hashes, actual scenario outputs,
   PNGs, neutral-frame pixels and explicit release records. Pillow must already
   be installed. The expected adapter remains `amd` / `gcn-5`; the two named
   options permit another lane to declare its own scope explicitly. Collection
   copies actual supplied runs and does not perform a replay. It does not copy
   October's hardcoded failure history into a new qualification.

`audit_docs.py --repo REPO --baseline REF --output NEW_AUDIT` compares changed
document contents in the same checkout. Added/deleted files require a full-tree
comparison and are explicitly refused by this read-text-only helper.

Q465: the maintained browser driver now records rejected, still-open and timed-out
closes as failures; collection requires the versioned release record and all
three successful closes. These controls do not retroactively validate October's
unconditional booleans or qualify closure in a real browser. That source-bound
runtime gate remains with the existing Cambium presentation owner plan. Physical
pointer, human AT, cross-app interoperability and other consumers remain separate.

Controls: `python -m unittest discover -s support/instrument -p test_paths.py` and
`node --test support/instrument/test_release.cjs`. Neither starts a browser or
native application; the Node positive control closes one owned loopback server.
