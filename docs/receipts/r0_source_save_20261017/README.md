# R0 source-save qualification — 2026-10-17

Turnstone base `b7e15290d50db7c4b00a29257f224c1d96f8d0cf`, with published Mere
`00e4442868ee1cb014364a81ccaed657b512b218`. Qualification ran on macOS 15.8.1,
x86_64, with `rustup run 1.99.1`. The [qualification record](qualification.json)
binds the [source inputs](source-inputs.json), [test executable](binary.json),
and [restricted lockfile delta](checks/lock-delta.json).

The final bounded library run passed **940 tests**, with zero failures, ten
existing ignores and 89 filtered tests. It includes all **50 capture controls**
and **16 unchanged scene controls**. Three captured-collection controls passed
separately in the same executable. Clippy's correctness gate, scoped formatting
and whitespace checks pass; other Clippy warning groups remain warnings.

To reproduce the bounded run and inspect its focused controls:

```sh
rustup run 1.99.1 cargo test --locked --lib --no-fail-fast -- \
  --test-threads=4 --skip place:: --skip sky_surface:: --skip sky_timeline::
rustup run 1.99.1 cargo test --locked --lib source_capture -- --test-threads=1
rustup run 1.99.1 cargo test --locked --lib place::captured_collection -- --test-threads=1
rustup run 1.99.1 cargo clippy --locked --lib --tests -- -D clippy::correctness
```

The capture controls cover real Fjall before/after-write faults, real facet and
graph write failures, receipt acknowledgement loss, local integrity and typed
role preservation, three/four fresh-process runs, and warm A/B/A adoption with
stale actor messages. The four-process run uses the real App, trail actor,
Fjall and session files; workers exit without Rust Drop/Flush after the required
failure/success checkpoints. Retry preserves original evidence, including
decoded non-UTF-8 HTML when a newer live response is present.

The bounded pass does not replace the earlier unrestricted run's failures and
interruption. The JSON and `checks/` retain those diagnostics, three individual
place-control passes, and Mere's full-library/isolated clock-budget failure.
Mere's six bounded-scene and two tag-axis controls pass; Turnstone's unchanged
receiving scene assertion passes against that published supplier.

This proves the declared save acknowledgement and tested process-exit/restart
model. Cross-store atomicity, power-loss durability, headed/GPU acceptance and
R1-R4 research product joins remain separate gates. An intent whose original
bytes never landed requires the original response; an intent that never
committed cannot appear in cold recovery inventory. Recovery never fetches a
replacement, scans the full artifact corpus to invent requests, or deletes
evidence as compensation.

Test-output excerpts preserve names, statuses and failure details; trailing
whitespace and extra blank lines at file ends are normalized.
