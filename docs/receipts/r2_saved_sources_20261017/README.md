# R2 private saved-source qualification

The [final qualification](qualification.json) passes on macOS 15.8.1 x86_64:
969 bounded regression tests (10 ignored, 89 shared-place/Sky tests excluded),
three dedicated capture-library controls, Clippy correctness and portable Cargo
verification. The 16 focused R2 controls are included in those 969 passes.
Two fresh headed processes present 245 frames with seven reviewed nonblank
captures. The [independent verification](native/verification.json) binds the
same A1 manifest, canonical text/hash and Unicode selector after actual origin
shutdown/TCP refusal and fresh reopen. [The final reader capture](native/reopen/10_reopen_A1_reader.png)
shows the local text, selected quote, provenance and separate original link.
[Earlier diagnostics](diagnostics/summary.json) retain the catalog failure,
identity callback test race, incomplete global-quiescence run and the passing
run before presentation polish. The identity test now synchronizes on the
callback itself; production identity behavior did not change.

This packet drives the R2 private source list, exact-text search, retained text
reader and fresh-process offline reopen through Turnstone's headed scenario
lane. It does not build the product. `r2_saved_sources_seed.scn` and
`r2_saved_sources_reopen.scn` are the only scenario inputs; fixture pages are
served by `fixture_server.py`.

The seed gives fetch completions ordinary frame turns, then relies on the
source-bound Save control and acknowledged saved-manifest/body assertions. It
does not wait for unrelated graph physics to become globally quiescent.

The seed uses ordinary navigation and the source-bound **Save source and keep**
action to admit A1 and B1, visits X without saving it, then changes A at its same
URL through `/swap-a` and explicitly captures A2. A separate named session
saves the unique `/Foreign` sentinel to prove the original session cannot list
or search it. The
original session searches `naïve — 東京 🐦`, opens its exact result, and asserts
the selected quote, full saved text and provenance. The verifier recomputes the
selected substring from canonical text using Unicode scalar indices and
compares it with the exact non-ASCII query. This detects byte offsets being
used as scalar offsets without relying on a guessed fixed range.

Before the seed searches or reads A1 offline, it touches the stop marker. The
runner terminates the loopback HTTP fixture, records its exit status, makes a
TCP connection-refusal check, then writes the acknowledgement marker. The app
waits for that acknowledgement before the offline search and read. The runner
keeps the origin down throughout the second fresh process, which explicitly
selects `R2 Private One`, searches A1, and reopens the same passage. The
independent verifier compares fixture request logs from seed completion through
reopen, so a successful OFFLINE flag alone cannot satisfy the receipt.

The runner reuses the immutable no-wallet custody fixture from the R1 receipt
at `docs/receipts/graph_scenes_20261016/inputs.json`. It freezes the supplied
Turnstone and custody binaries, copies both scenario files, hashes the current
source inputs, gives each process a separate capture directory, and preserves
the same isolated profile across the two launches. It does not retry an
unavailable GUI or restart the origin after stopping it.

After the implementation's scenario hooks and native binary are ready, run from
the repository root with a new output directory outside the checkout:

```sh
python3 docs/receipts/r2_saved_sources_20261017/run_native.py \
  --binary target/debug/turnstone \
  --keeper-binary /path/to/tucket-custody-receipt-host \
  --seed scenarios/r2_saved_sources_seed.scn \
  --reopen scenarios/r2_saved_sources_reopen.scn \
  --timeout 240 \
  --output /Users/markik/Code/output/turnstone-r2-saved-sources-<run-id>
python3 docs/receipts/r2_saved_sources_20261017/verify_native.py \
  /Users/markik/Code/output/turnstone-r2-saved-sources-<run-id>
```

The scenario relies on these implementation-owned observable hooks: `act
Search saved sources`, `wait-saved-sources`, `wait-saved-reader`,
`record-saved-sources`, and the `saved-sources-*` / `saved-reader-*` snapshot
fields. The JSON observation must represent the actual retained projection and
include the exact target key, canonical text, BLAKE3 text hash, selector,
manifest, source origin, and the resolved retained EngineDocument. Screenshots
remain the visual review surface; observation JSON binds each capture to the
exact saved record and selector.

The verifier independently requires exactly three private admissions for A1,
B1 and A2, no X admission, no foreign-session sentinel in the private snapshot,
distinct A1/A2 immutable identities and raw response hashes, per-owner facet
reference order matching the ordinary capture receipts, and the same A1 reader
target/content after a fresh process. It uses capture manifests to identify
versions without assuming a global order across graph members. Canonical text hashing
uses Python's `blake3` package or `b3sum`; it deliberately fails if neither is
available rather than substituting another digest. This packet does not qualify
hosted captures, PDF/screenshots, inference/Answers, interactive page replay,
power-loss durability, other platforms, or human assistive-technology use.
