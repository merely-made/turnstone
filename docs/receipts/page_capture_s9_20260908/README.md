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
source and lock are identified by those hashes rather than copied here. Its
Inker dependency was the local Mere path recorded in the runner manifest. The
hashes preserve that runner input without assigning it a later checkout head.

## Source checks

With the same isolated target, the final narrow source check passed and reached
Turnstone:

```text
cargo check --offline --no-default-features --lib -j 2
```

It finished successfully in 1m08s with 77 pre-existing warnings. The terminal
stream was not redirected, so this records the command and exit result rather
than claiming a raw-log artifact. This was a library-only build with Turnstone's
empty default feature set, so the Weld renderer host was disabled.

The resolved Turnstone lock had SHA-256
`39fddda030b020a7221b3150141c7a5091a380cad9602cbc6e7bdf6543614f65`.
It selected Genet `9e8f9dc2f3ddc0af1658580bb51964462a03923f` for
`genet-documents` and Mere `2b1ce46e5a15328b4bf4d350ec4b0252d9b404a1` for
Inker and `weld-engine`. These are the full-check inputs; they are distinct from
the external runner's local Mere path.

The toolchain was Rust/Cargo 1.97.1 on `x86_64-pc-windows-msvc` (rustc commit
`8bab26f4f68e0e26f0bb7960be334d5b520ea452`). The Weld command enabled the
Windows CEF/Welding hosted-surface adapter; it was a source compile attempt,
not a headed renderer run.

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

The local Cargo configuration patched Grafting to the clean checkout at
`403a30c2fab39c573d1eebb57a0995e2c3347ff1`; its current API produced the Weld
mismatch. The pinned Git Welding 0.14.0 package remained selected because the
local Welding 0.15.0 patch was unused. This partial consumer check does not
close the P1 clean-source pin gate. Successful capture bytes remain logged and dropped;
deposit, envelope persistence, observation identity, CSS viewport and applied
scale facts, and headed pixel proof remain open.

## 2026-09-09 correction

The excerpt above attributes the `E0308` mismatched-types error to
`welding/src/native_frame/dx12.rs:349`. That line number is where the
`import_dx12_shared_texture` function this error concerns is *defined*; the
error itself was reported at the *call site*, `src/shell/surface_frames.rs:92`
(Turnstone's own code, passing `&native` where `grafting::import_dx12_shared_texture`
takes `Dx12SharedTexture` by value). None of the eighteen errors that blocked
this feature check originate inside welding or grafting themselves — all are
in Turnstone's own `src/shell/weld.rs` and `src/shell/surface_frames.rs`.

More broadly: the Weld renderer adapter has been uncompiled since the P1
capture contract landed, because `cargo check --offline --no-default-features
--lib` — the check this repo has been running to validate ordinary changes —
excludes the `weld` feature entirely, so drift in `src/shell/weld.rs` and
`src/shell/surface_frames.rs` against either the pinned mere `weld-engine`
trait or the pinned `welding`/`grafting` APIs went unnoticed until a
feature-gated check was run again.

This was corrected in worktree `turnstone-weld-pin-20260909` (branch
`slice/weld-pin-20260909`), pinning welding to
`65d057def7db2b5034add6e316cbd51d97c163a1` and grafting to
`403a30c2fab39c573d1eebb57a0995e2c3347ff1` (commit `0c8461f`, "Move welding
and grafting pins to current heads (pin choice pending)"), then migrating the
adapter against those heads and the committed mere `weld-engine` pin
`2b1ce46e5a15328b4bf4d350ec4b0252d9b404a1`. Exact commands and their outcomes,
with `CARGO_TARGET_DIR=C:\Users\mark_\Code\.targets\weld-pin-20260909`:

```text
cargo check --offline --features weld -j 2
```

Exit 0. Both the `turnstone` lib and bin targets compiled; 76 pre-existing
warnings in the lib, none from the migrated files.

```text
cargo check --offline --no-default-features --lib -j 2
```

Exit 0, unchanged from the earlier pass in this receipt (77 pre-existing
warnings; the weld renderer is compiled out under this feature set).

```text
cargo test --offline --lib page_capture -j 2
```

Exit 0. All five `page_capture::tests::*` S9 correlation regressions
(`exact_completion_is_single_use`, `navigation_and_surface_replacement_refuse_old_completion`,
`session_switch_refuses_completion`,
`generation_exhaustion_rotates_surface_and_retires_pending`,
`admission_failure_spends_id_and_allocator_exhaustion_refuses`) still pass,
unmodified by the migration.

```text
git diff --check
```

Clean.

No headed CEF run was attempted; this closes the source-compile gate only.
`Shell::request_page_capture`'s call into `WebSurface::request_page_capture`
now resolves to `inker::WebSurface`'s default (`Err(SurfaceError::Unsupported)`),
because the pinned mere `weld-engine` `WeldProducer` does not forward that
default to `TurnstoneWeldSurface` at this pin — P2 (driving `welding`'s
`request_snapshot_png`/`poll_snapshot_png` behind the P1 contract) remains
open and unchanged by this pass; see `src/shell/weld.rs` for the in-source
note. `Shell::request_page_capture`'s correlation rollback on that typed
error keeps the S9 semantics intact.
