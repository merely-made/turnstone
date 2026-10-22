# Headed retained-source embedding recall, 2026-10-20

`qualification.json` records the receiving SDK, builds, focused checks and exact
accepted artifacts. `native/verification.json` records the independent source,
model, private-cache and process checks. All 13 original native captures remain
under `native/seed/` and `native/reopen/`; `review/` contains the RGB JPEG contact
sheets and answer previews used for review. The packet excludes model weights,
raw stores, vaults, private keys and executables. The profile directory contains
only public synthetic graph, facet and workspace sidecars.

The ordinary UI saves two HTML sources, stops their origin, reads locally,
chooses both sources, asks through the resident and opens the exact citation.
After both processes exit, a fresh pair restores the saved source and historical
answer, then makes a new ask against the matching reopened index. MiniLM performs
real CPU retrieval; the admitted SmolLM2-360M-Instruct model generates the answer
on the AMD Metal adapter. This is a bounded source/answer/cold-reuse receipt;
the earlier I10 ranking benchmark remains separate.

Reproduction requires local models matching `native/inputs.json`. Build Turnstone
with the committed lock and the Mere helper with `inference-gpu,inference-embedding`.
Supply final binary paths and a new output directory outside the repository:

```sh
python3 docs/receipts/embedding_recall_20261020/run_native.py \
  --binary /absolute/path/to/turnstone \
  --resident /absolute/path/to/inference_route-test-executable \
  --supplier /absolute/path/to/mere \
  --model /absolute/path/to/verified-SmolLM2-360M-Instruct \
  --embedding /absolute/path/to/all-MiniLM-L6-v2 \
  --embedding-artifacts /absolute/path/to/model-artifacts.json \
  --output /absolute/path/to/new-run
```

The independent verifier requires Python `blake3`. Copy the retained expected
admitted/raw digest bridge `native/embedding-artifact-blake3.json` into the new run
root, then
run `verify_native.py /absolute/path/to/new-run`. The committed packet also
revalidates with `verify_native.py docs/receipts/embedding_recall_20261020/native`.
ModelLibrary admits parsed configuration JSON and resolves canonical bytes for
ESP; the digest bridge records that transformation rather than confusing it with
the downloaded pretty JSON. `superseded-run.json` records the earlier observation
race; accepted native evidence comes from the complete fresh rerun.

For byte-bound reconstruction, use Mere SDK `125df8df614ba503307cdacdba6150781bcea908`
and overlay only `ports/djinn/tests/inference_route.rs` from the published fixture
revision in `qualification.json`, then build the ignored helper with the features
above. The fixture file hash is checked against the compiled input receipt;
concurrent unrelated Mere changes were reconciled only for delivery.
