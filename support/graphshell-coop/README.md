# Native co-op qualification

Owned by `design_docs/2026-11-11_coop_harness_plan.md`, which carries out Mark's
rulings `mere:engine-split D129` and `D130` (2026-11-11).

## Two repositories, Turnstone the root

The viewer (`graphshell_mere`) comes from this repository. Djinn, the seeder
(`coop_existing_graph`) and the fixture custodian (`coop_fixture_custodian`) come
from Graphshell `host/`, checked out at the exact revision this repository's
lock pins for graphshell.git and built with `host/`'s lock. No lock is tracked: a lock
on disk is used, else the one `house resolved show <commit>` holds for that
repository's commit (`locks.py`; `freeze.py` writes it before building). `freeze.py`
clones Graphshell when `--host-repo` does not exist (an existing checkout is only
checked: HEAD at the pin, no tracked changes), builds both with `--build`, and
records both commits, both lock hashes, both input file sets, every resolved Git
supplier, and every shared crate whose resolution differs between the two locks.
Differences are recorded, not refused; `verify.py` recomputes the list from both
locks and fails if the receipt's list does not match (coop-harness D2):

```sh
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=4
python3 support/graphshell-coop/freeze.py --build --repo . --artifacts TARGET \
  --host-repo GRAPHSHELL_CLONE --host-artifacts HOST_TARGET \
  --harness support/graphshell-coop --output BINARIES
```

The build is three cargo invocations, on the toolchain each checkout's
`rust-toolchain.toml` names (no release is hardcoded):
`-p graphshell-mere --bin graphshell_mere` here; in `host/`,
`-p mere-resident --bin djinn --features passphrase-fd` alone, so the examples'
dev-dependency features never reach it; then
`-p mere-resident --example coop_existing_graph --example coop_fixture_custodian`.
`freeze.py` refuses a Djinn whose resolved features include `in-process-custody`.

## Custody and consent

Each worker starts the fixture custodian once (a synthetic passphrase vault with
the packaged Djinn grants at consent `none`) and hands its client file to a
production `passphrase-fd` Djinn as `--custodian`. The custodian outlives Djinn's
restarts, as an installed one does. Before Djinn starts, the worker writes the
grant `coop_existing_graph grant` prints (Graphshell's own `grant_for` over
`declaration()`) into the profile's owner settings, `applications`. The receipt
proves Djinn's custody client and grant checks and the state after a yes. It does
not prove the `djinn-custodian` binary, its confinement, the service listener,
the root-owned client file, Linux's per-generation consent or the consent dialog.

## Baseline: one desktop

```sh
python3 support/graphshell-coop/run.py --mode baseline --repo . \
  --host-repo GRAPHSHELL_CLONE --owner-binaries BINARIES --output RUN
python3 support/graphshell-coop/verify.py --receipt RUN/receipt \
  --commit COMMIT --host-repo GRAPHSHELL_CLONE
```

The owner worker alone, on a Linux or macOS desktop: the custodian and Djinn
start, `seed` creates a private and a chosen graph through the door as
`graphshell`, the viewer reaches `status=ready attached=<chosen>`, switches to the
private graph and back and leaves two captures. Djinn then stops and the same
viewer, profile and domain run again as the positive control: it must not reach
ready. On Linux the worker requires exactly one GPU render node and selects that
GPU's own Vulkan driver (never llvmpipe). The receipt records the GPU and driver, as
it records the owner's OS and architecture, and `verify.py` checks that the viewer
held that render node and loaded that driver (coop-harness D3). `RUN/receipt` is the packet; `RUN/local/private` holds the run's vault and
is never copied. The packet's `capture-review.json` is written after the run by
whoever opened each capture, and binds itself to them by hash.

## Two machines: unverified since 2026-10-24

`--mode direct`, `--mode relay` and `--mode recovery` drive a Mac or Linux owner
and a Linux Wayland guest over an owned SSH control socket (`--guest-*`,
`--ssh-control`). They share one scope, attribution, revocation and cold-reopen
controller. Relay modes require `--relay-binary`, its `--relay-sha256`, and the
socket; SSH carries control and invitation handoff only. Their last pass was
2026-10-24. Custody isolation (2026-10-26), admission by consent (2026-11-01) and
the two-repository split came after it; the custody, grant and build changes
above reach these modes too, but none of their phases has run since.

Recovery stops the owned relay before a guest save, records pending and failed
native states, checks the owner's durable graph before and after the failure,
then restarts the relay on its original address. The same guest window uses
Reconnect and explicitly retries its edit. Only an owner acknowledgement is a
save. A later live withdrawal and full cold reopen prove that recovery did not
widen scope or restore a revoked grant. After the listener restarts, the
controller waits for both original endpoints to rejoin it before releasing
Reconnect; it does not retry an application edit or extend its deadline.

`machine.py` selects the shared native desktop provider and the optional Linux
network-namespace provider. The relay is a short-lived private-LAN fixture,
using the original locked Iroh relay executable. Relay modes check that the
guest has no UDP egress while TCP reaches the relay; this does not qualify WAN
deployment or TLS relay operation. The verifier still accepts the original relay
and recovery packets for regression checks; existing output directories are
refused.
