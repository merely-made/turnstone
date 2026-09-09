# S9 page-capture correlation receipt, 2026-09-08

Source under test: Turnstone
`c5f991e61976f7b4321085392d21765a4b068dd5`. The production correlation
module hash was
`01933f620edbb5375f5d715f3faeb1f0da0a9afdee8041a40664eba6a81bdff5`
(SHA-256).

## Focused regression manifest

The disposable runner imported `src/shell/page_capture.rs` directly and used
the real local Inker `PageCaptureRequestId` and the real `uuid` crate. With
`CARGO_TARGET_DIR=C:\Users\mark_\Code\.targets\turnstone-capture-agent`, this
exact command passed 6/6:

```text
cargo test --offline --manifest-path C:\Users\mark_\Code\scratch\turnstone-capture-correlation\Cargo.toml -j 1
```

Five production-module regressions passed:

- `page_capture::tests::exact_completion_is_single_use`
- `page_capture::tests::navigation_and_surface_replacement_refuse_old_completion`
- `page_capture::tests::session_switch_refuses_completion`
- `page_capture::tests::generation_exhaustion_rotates_surface_and_retires_pending`
- `page_capture::tests::admission_failure_spends_id_and_allocator_exhaustion_refuses`

The external real-API sanity test also passed:

- `runner_uses_real_inker_request_identity`

Runner input SHA-256 values:

- `Cargo.toml`: `a29297841e899d4f31ec9f70629a4232ac4ed2429740dacc05f3f57bbf1492ab`
- `correlation.rs`: `e0e1f2b426752002711d46b33cbb5fc3bfad511c3f1cc2da112e960282e1e117`
- `Cargo.lock`: `9a6d67778eb99bf18b6e8838f5c5a5f3e23bcd27fa04d182ff838d44b70d0287`

The runner stayed outside the repository at
`C:\Users\mark_\Code\scratch\turnstone-capture-correlation`; its manifest,
source and lock are identified by those hashes rather than copied here.

## Source checks

With the same isolated target, the final narrow source check passed and reached
Turnstone:

```text
cargo check --offline --no-default-features --lib -j 2
```

It finished successfully in 1m08s with 77 pre-existing warnings. The terminal
stream was not redirected, so this records the command and exit result rather
than claiming a raw-log artifact.

The locked Weld check refused before resolution because the checked lock would
change:

```text
cargo check --locked --features weld -j 1
```

The unlocked offline check reached a concrete dependency compile failure:

```text
cargo check --offline --features weld -j 2
```

Pinned Welding `c65cc1086c8743c94d4bb69f8eac90e077a1dc9b` expects fields absent
from grafting `403a30c2fab39c573d1eebb57a0995e2c3347ff1`. The retained bounded
diagnostic is [weld_failure_excerpt.txt](weld_failure_excerpt.txt); the receipt
preserves its bytes via the local `.gitattributes` and has SHA-256
`3e54c26922243d3a9f986d1241687a72f7947e9fdf0101c57b0a06e3643bfde8`.
This failure occurred before Turnstone compiled, so
the no-default pass does not validate the Weld adapter.

Dependency workspace state for the passing partial consumer check was Mere
`54a852ac57fd4e99784175c78fa01a84e3065ea0` (dirty) and Genet
`650b28114ac12db0ac50d77f359f138a14801724` (dirty). It does not close the P1
clean-source pin gate. Successful capture bytes remain logged and dropped;
deposit, envelope persistence, observation identity, CSS viewport and applied
scale facts, and headed pixel proof remain open.
