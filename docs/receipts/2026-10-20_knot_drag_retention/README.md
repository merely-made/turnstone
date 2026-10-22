# Embedded Knot drag retention, 2026-10-20

Bounded macOS native acceptance on the unchanged supplier graph at Turnstone
base `8116a5f0b6834bfdf1c17cf9f97861f070d6ac9a`, with the local code hashes
in [source-inputs.json](source-inputs.json). The owning
[receiver plan](../../../design_docs/2026-10-11_retained_clip_receiver_plan.md)
records the product boundary. The October 19 packets remain frozen.

The shipping executable is copied into a temporary LaunchServices application
for each phase. All three passing phases use the same executable hash, isolated
synthetic profiles, a real wallet-backed custody IPC fixture, and offline
acquisition. No personal profile, wallet, keys, or retained-store files are in
this packet. [qualification.json](qualification.json) records the checks,
source and binary identities, package equality, privacy scan and limitations.

## What passed

- A continuous primary mouse drag through ordinary Workbench input visibly
  selects `café` at UTF-8 bytes 16–21 in a saved Djot document. The contextual
  retention command becomes available immediately. Source bytes remain intact;
  full backing is retained privately, and the onward package contains only the
  five selected bytes.
- A second isolated receiving store reads the selected package with every
  fixture source document removed. It plays the previously qualified 100 ms
  selected voice body to Ended, with its source recording unavailable.
- Both transfer packages are removed, and both custodian and receiver restart.
  Cold export produces packages equal to the selected originals. Native
  selectors survive: Knot bytes 16–21; voice target 1250–4500 ms and pressed
  offset 1400 ms. Acquisition attempts remain zero.
- Raw retained-store scans find no complete clear source/WAV body, and the
  profiles contain no plaintext WAV cache. This is a bounded scan of these
  fixture stores, not general disk forensics.
- Focused checks: 23 Knot authoring, 15 retained clip, 2 producer retention,
  and 59 native input cases pass. Five configured external endpoint checks and
  one explicit process helper remain ignored in the ordinary sweep.

Eight captures from the passing phases were reviewed. The diagnostic
[before-observation](before-observation/scenario.done) run is deliberately a
failure: its pointer trace holds bytes 16–21, while the app still offers no
retention command. The final host synchronously refreshes Knot observations
after handled input. This failure is separate from the passing verdict.

## Reproduction and scope

The frozen scenarios use explicit synthetic paths from this run. `run_native.py`
wraps Mere's `scripts/run_macos_scenario.py` and freezes each shipping executable;
`prepare_receiver.py` verifies selected bodies/selectors and creates the receive
and cold scenarios. `finalize_receipt.py` checks the finished runs, exact package
equality, current source hashes and store scans. Runtime setup requires an
isolated marked root and the explicit Djinn `retained_custody_receipt_host`
fixture from the Mere custody tests. Absolute runtime roots and helper paths
must be adapted for a new run. The runtime, executable and wallet are retained
only in the task's local output directory.

This qualifies primary mouse selection and this bounded text fixture. Touch,
bidi, autoscroll and broader native editor acceptance remain separate gates.
Scenario export uses the normal Action/effect path with an explicit new-file
destination; the OS save dialog is not automated. The voice fixture is reused
from the October 19 owner-export receipt; there is no renewed recording,
acoustic or owner-export claim. This is not a full workspace test verdict.
