# Current-stack native place sharing, 2026-10-19

This packet qualifies the existing place graph/chat path on macOS using
separate Turnstone processes and real isolated Djinn residents. It follows
an initial clean Turnstone main at `3de074b`, then integrates the concurrent
retained-custody intake `01658c7` through merge `9570cc6`. The earlier passed
cohort is preserved under `prior-cohort/`; the top-level accepted receipt uses
the combined current source. Source hashes, executable hashes and the actual
app/resident dependency cuts are recorded alongside the evidence. Final
results are in `qualification.json`.

Current integrated results: **76 place tests**, **12 place UI tests**,
**3 frame-wait tests**, **27 authority-ring tests** and **14 retained-clip tests**
passed. Six existing ring/retained-clip tests remain ignored; the full workspace
suite was not rerun. The native run passed all six phases with **2,372 presented
frames** and **15 individually reviewed, nonblank captures**. The origin was
gone before both resident/native cold reopens. See [qualification](qualification.json),
[native assertions](accepted/verification.json), [capture review](capture-review.json)
and [resolved stack](resolved-stack.json).

## Product repair

An ordinary writer invitation previously tried to issue a document projection
grant even when the place offered no document service. The custody resident
correctly refused release of that unrelated signing key. Turnstone now derives
that requirement from the actual offered services. When a projection host is
offered, its grant is prepared before membership changes, so a refusal cannot
leave that admission half-authored. Removal likewise avoids unrelated document
authority for a graph/chat-only place. Custody release policy is unchanged.

A regression exercises a provider that refuses projection signing: an offered
projection refuses before membership changes; a graph/chat writer can be
invited, admitted and removed without that signer. The existing lifecycle walk
now explicitly offers the document host whose grant expiry it checks.
Place refusals also reach the ordinary diagnostic log. A second acceptance
defect was a synchronous `wait-status` poll: it could starve presentation while
another native process started. Status and file waits now share one bounded
frame-by-frame wait mechanism. Three focused tests cover readiness, framing of
the wait budget and status diagnostics; the native run exercises the real
cross-process rendering path. The presentation watchdog is unchanged.

A full cold restart also exposed stale endpoint addresses. `Refresh place
contact` reads a current card for the exact already joined binding and then
uses the existing reconnect path. Stale session requests and foreign-place
cards are refused. The worker reopens retained membership, imports the endpoint
tickets and only then replaces the bounded contact descriptor. This refresh
neither replays a welcome nor issues, extends or changes any authority grant.
Its contact window is ten minutes. Existing expired-hint copy now directs the
person to this gesture rather than incorrectly requiring another invitation.

## Accepted boundary

The headed workflow creates a place, offers a pre-key, invites and joins a
writer, exchanges messages, and shares one chosen HTTP address. An
unshared founder address remains local. The writer exits, the founder authors
another message, and a fresh writer process first reads its retained fold with
no lanes open, then reconnects and catches up. A third, read-only member joins,
receives the shared address and messages, and is refused an attempted share of
its own private address.

All apps and all three residents then stop. The HTTP origin stops and a TCP
probe confirms refusal. Founder and writer restart their encrypted custody
vaults in new resident processes and reopen Turnstone in fresh native processes.
They first show their retained graph and five messages without opening lanes,
then the founder exports its current place card. The writer first refuses a
foreign-place card without changing its retained fold, refreshes the current
contact through the ordinary palette path, and reconnects. They exchange two
new messages. The refreshed public contact descriptor is checked after exit;
its hints differ from the initial endpoint while the retained binding stays
unchanged. The shared-graph digest stays
constant; the new chat digest converges. Exited profile caches establish exact
address scope, including both local-only sentinels.

The shared projection transfers addresses. Visible page captions in profiles
that fetched the page are their own retained local metadata; this run does not
qualify independent title transfer.

This is same-host macOS acceptance over the existing local discovery/transport
path with deliberate contact renewal. Automatic multicast discovery was not
usable in this unsigned macOS acceptance setup, and is not qualified by this
receipt. The first cold attempt preserves that failure. This packet qualifies
address and chat sharing, retained reopening and live
catch-up. It does not qualify offline HTTP body replay, cross-machine or WAN
reachability, current document projection/editing, named-recipient publishing,
saved-source attachment transfer, withdrawal of a delivered copy, or physical
power-loss recovery. The document service is explicitly absent in this run.
The input uses the normal semantic native producer path; physical keyboard,
IME and OS assistive activation remain separate gates.

Djinn is built from Turnstone's exact Mere intake using its supported
`passphrase-fd` receipt feature. Each role gets a real passphrase-encrypted vault;
the passphrase exists only in controller memory and is handed through stdin.
There are no fallback fixture identities. This is not installed custody or OS
unlock qualification. App and resident dependency graphs are recorded
separately in `resolved-stack.json`. The concurrent upstream intake supplies
the newer app cohort; the place changes preserve that published intake rather
than adding unrelated dependency updates.

## Reproduction and verification

Use Rust 1.99.1 and the integrated repository lockfile to build Turnstone with
`cargo build --bin turnstone --locked --offline`. Record the executable SHA-256
and all tracked runtime input hashes (`src`, `scenarios`, `Cargo.toml`,
`Cargo.lock`) as `build-inputs.json`, matching the included compiler manifest.
Build Djinn from the recorded Mere cut with
`cargo build --locked --offline --manifest-path <mere>/ports/djinn/Cargo.toml
--bin djinn --features passphrase-fd`. The recorded resident manifest gives
its exact lock hash and build command.

Run `python3 run_native.py --binary <turnstone> --binary-inputs <build-inputs.json>
--djinn <djinn> --output <new-external-directory> --timeout 600` from this
packet's directory. The driver derives its repository root, freezes its input
files, and refuses source drift. Leave the test windows visible; do not
fullscreen or cover them. Reopen phases reuse that role's same owned temporary
app bundle but launch new native processes. No focus, presentation or platform
security check is bypassed.

After every process exits, run `python3 verify.py <output> --inspect-exited`.
This copies only the fictional fixture's plaintext graph caches and binding;
keep vaults, sealed stores, invitation artifacts, credentials, sockets and
executables private. Subsequent checks can use the copied caches and
`--commit HEAD`. Publication also checks a fresh committed Git archive against
the compiler input hashes and every public packet artifact hash.

`diagnostic-attempts.json` preserves the failed runs and the reasons they were
superseded. A failed invitation was an actual product defect; endpoint setup,
JSONL framing and native loader startup were harness diagnostics. A blocking
status wait was a native driver defect; full cold reconnect exposed a missing
ordinary contact-renewal path. None of the superseded runs is
counted as accepted evidence. Build and transport logs retain existing warnings
and teardown errors verbatim; passing test results do not mean silent logs.

Run `python3 verify_archive.py HEAD` after committing to validate the actual Git archive before publication.
