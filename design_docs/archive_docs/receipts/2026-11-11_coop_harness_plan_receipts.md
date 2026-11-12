# Co-op harness plan: receipts

Run evidence for [the co-op harness plan](../../2026-11-11_coop_harness_plan.md). The packets live outside the repository, in `Code/testing/turnstone/2026-11-11_engine_split_headed/fedora/`, and are cited here by path.

## 2026-11-12: H6, first headed baseline (Fedora ThinkPad)

**Machine.** T-PC, a ThinkPad 20U6S1CW00 running Fedora Linux 45 Workstation (kernel 7.3.3, x86_64) in a GNOME Wayland session on seat0, unlocked. GPU: render node `/dev/dri/renderD128`, vendor `0x1002`, device `0x1636`, kernel driver `amdgpu`, Vulkan driver `/usr/share/vulkan/icd.d/radeon_icd.x86_64.json` (`libvulkan_radeon.so`), as recorded under D3. The installed `djinn --installed` and the `djinn-custodian@markik` service kept running beside the run and were not used.

**Sources.**
- Turnstone `534e774cc8e0f378197c7a2fdef3a27be3c3ba68` (branch `coop/harness`). Lock sha256 `42f87b10f3d0385ab901f8767b76448be71abeb06e14623ff6071b5c4d8ce90d`.
- Graphshell `aed944eb524ad4869b051730035a9dca88139db3`, which is the pin in that lock. `freeze.py` cloned it from GitHub into the run's scratch tree. host/ lock sha256 `040bb9e01f0fbefcc2f81f71222357995f32e34b490aeeef6787a754e0a1a08a`.
- Recorded: 2,024 Turnstone and 1,945 Graphshell input files, 38 Git suppliers, and one build-generated input (`host/crates/script/wit/world.wit`). Of 1,663 compared shared crates, 19 resolve differently and are recorded (D2): Mere (catchment, mere-facts, mere-kernel, mere-rdf, pandect), Genet (document-session-api, engine-observables-api, genet-events, genet-paint-types, genet-scripted-dom, genet-static-dom, genet-text, layout-dom-api, regress, script-engine-api), Distillery (distillery, esp, mere-mesh), and wgpu-graft (grafting-frame).

**Build.** `freeze.py --build`, all `+1.99.1 --locked`, `CARGO_PROFILE_DEV_DEBUG=0`, `-j 4`. Viewer 4m11s; Djinn alone (`--features passphrase-fd`) 4m43s; the two examples 1m38s. Djinn's resolved features are `cxf,default,passphrase-fd,personal-sync`, without `in-process-custody`. Binary sha256 prefixes: `graphshell_mere` 75064a69, `djinn` 412ae89e, `coop_existing_graph` 52d1e422, `coop_fixture_custodian` e4fb7352.

**Run.** `run.py --mode baseline` (no scratch controller) printed `{"result": "ok"}`.
- The fixture custodian (pid 48311, profile `CrossGraphOwner`) started first. Djinn (pid 48315) logged `custody reached profile=CrossGraphOwner in_process=false`. Status was ready and unlocked.
- `seed` opened the door as `graphshell` under the grant from `grant_for`. It created the private graph `f4e844f4-374a-4d7c-a8f7-863914a1349a` and the chosen graph `a523656e-a61c-47f4-b8ac-0316740ba3bb`.
- The viewer (pid 48394) reached `status=ready attached=a523656e-…`, switched to the private graph and back, and left two 1366x701 captures. Its await-failed field was empty and the receipt had no errors. The baseline graphics record shows the process holding renderD128 and having loaded `libvulkan_radeon`.
- Djinn then stopped (exit 0).
- **Positive control:** the same viewer, profile and domain (pid 48475) exited 0 without being terminated. It ended with `status: unavailable`, `attached: ""`, and `await-failed: attached expected "a523656e-…", was ""; status expected "ready", was "unavailable"`.
- The custodian stopped (exit 0). `cleanup.json` lists no owned process remaining.

**Captures** (reviewed whole; the per-capture notes are in the packet's `capture-review.json`):
- `private-before`: one node, `https://private.invalid/owner-only`, and no error banner.
- `existing-graph`: one node, `https://shared.invalid/existing`. The sessions column is clipped at its top edge because it is too short for two entries; this is a layout defect, not the door.
- `control-not-ready`: "Connecting to resident" and "transport failed: Connection refused (os error 111)" with Retry; no graph, sessions or canvas.

**Verifier.** `verify.py --receipt run3 --commit 534e774c… --host-repo <Graphshell clone>` returned `result ok`. It printed the owner OS, architecture and GPU, both commits and lock hashes, the 19 lock differences, the custody mode and the grant origin. In a negative check, a copy of the packet whose control was hand-edited to `status ready` and the chosen graph attached (manifest re-hashed) failed verification.

**Superseded attempts in the same session.** Each stopped on a harness bug, and each was fixed in its own commit:
- `freeze.py` metadata could not run `--offline` on a fresh CARGO_HOME (`c8ab2255`).
- The grant check read `agent`, which a grant omits when false (`83125ebf`).
- The control predicate required `ok false`, but an expired await leaves the receipt ok (`6a656b0e`). That run's baseline and control themselves passed.
- A build-generated, ignored file was listed among host/'s tracked inputs (`534e774c`). The `6a656b0e` packet, verified up to that point, is kept as `fedora/run3-superseded-6a656b0e`.
