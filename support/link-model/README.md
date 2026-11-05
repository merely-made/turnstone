# Link qualification tools

These maintained verifiers share `ordinary_paths.py`. All take an explicit
qualification directory containing the actual package, site, logs, results and
captures named by its manifests. Missing generated package/site files are a
verification failure. A receipt-only Git snapshot does not contain all generated
artifacts retained on the original qualification host.

```
python support/link-model/ordinary_paths.py QUALIFICATION_ROOT
python support/link-model/shared_attachments.py QUALIFICATION_ROOT
python support/link-model/counted_topology.py QUALIFICATION_ROOT --repo MERE_REPO
python support/link-model/retired_decay.py QUALIFICATION_ROOT --repo MERE_REPO
```

The last two also read exact historical Git objects from the explicit repository
(default: this source checkout). They preserve source revision, log/hash,
package, suite-count, capture, scenario and reopening checks. These commands
read evidence and print results; they do not build or launch the product.
Recorded command directories accept absolute POSIX or Windows paths independent
of this verifier's OS. Artifact paths must still be relative to the evidence
root; absolute, drive-relative and rooted spellings are refused.

The P7 loopback server also lives here. It requires a separate caller-prepared
static site, generated package and a fresh output directory:

```
python support/link-model/qualification_server.py --site STATIC_SITE --package PACKAGE --output NEW_OUTPUT --port PORT
```

The caller owns the browser driver and server lifetime. The server neither
rebuilds a package nor changes IndexedDB. Its optional `--moot-owner` and
`--moot-owner-state` retain the existing explicitly supplied local fixture
boundary; this split does not change that owner's source or state. Output must
be outside the source/site/package trees, and an existing directory is refused.
Retain only the brief's required receipts, default 100 MB. Server scenario
progress can advance within that new run; frozen receipt directories are inputs.

Original scripts and recorded output bytes from the five old packets now live
under `Code/testing/mere/support/receipts/<original-packet>/`. They remain frozen
evidence, including old commands and missing-other-host artifact references.
The full old/new tool map and byte manifests are in
`Code/testing/mere/receipt-relocation-2026-11-04/batch-07-reader.md`.

Controls: `python -m unittest discover -s support/link-model -p test_tools.py`.
They check shared imports, historical Git reads, log/manifest rejection and an
owned loopback server's explicit site, package and receipt paths. They do not
claim a fresh headed product run.

For a new current-source qualification, use a new packet rather than replacing
the inputs or hashes of an old packet:

```
node support/link-model/fresh_browser.cjs --packet NEW_PACKET --playwright PLAYWRIGHT_MODULE
python support/link-model/fresh.py NEW_PACKET --repo MERE_REPO
```

The prepared packet needs a locked product WebGPU build, generated package,
frozen site, source-bound command records and `fresh.json` describing one of
`ordinary`, `attachments` or `topology`. Its complete test inventories retain
Pictograph's `canvas` feature and integration/doc tests, Graphshell's web library
with default features disabled, and Seiche's default integration/doc tests.
Current passing and ignored counts must match independent unfiltered listings.
Topology also retains the viewer-only Wasm check and port-boundary check.

The macOS driver owns a fresh profile and loopback listener. It runs the original
seed and reopen scenarios in distinct headed native Chrome processes at the same
origin, forces WebGPU, records the AMD adapter and saves every scenario PNG.
It closes only its own resources and records process exits, profile removal and
port reuse. It refuses an existing browser-output directory. Any failed run is
retained; another attempt needs a new packet. Visual review must inspect and hash
every accepted PNG before `fresh.py` can accept the packet. This qualification
does not replace physical input, assistive-technology or cross-application checks.

Fresh controls: `python -m unittest discover -s support/link-model -p test_fresh.py`
and `node --test support/link-model/test_fresh_browser.cjs`. They reject partial
test scopes, altered scenario sets and forged inventory listings, and check the
server's explicit roots. They do not constitute a product qualification.
