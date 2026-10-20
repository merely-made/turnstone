# Embedded Redshank text-note retention, 2026-10-20

Bounded macOS native acceptance at Turnstone base
`648119dc4bef2b6275249da77f1aa4c4c3c30e98`, with the local source hashes in
[source-inputs.json](source-inputs.json). The existing supplier pins and locked
Git dependency graph are unchanged. The [receiver plan](../../../design_docs/2026-10-11_retained_clip_receiver_plan.md)
owns the consumer boundary; earlier receipts remain frozen.

The shipping executable is copied into a temporary LaunchServices application
for each phase. All four phases use executable SHA-256
`8e57061d8bc231c8c686e4b2aef66db5a386d56e2cb6928b92fd431e30536852`,
isolated synthetic profiles, the wallet-backed custody IPC fixture and offline
acquisition. [qualification.json](qualification.json) binds the sources,
processes, selected package, native timing and privacy checks. No wallet, keys,
application profile or retained-store files are included in this packet.

## Qualified behavior

- The embedded owner model projects a private control, one selected shareable
  text note, and an unselected shareable control into Turnstone's graph. The
  native scenario selects their ordinary note addresses. The private note has
  no producer-retention offer; the selected shareable note offers **Retain
  selected Redshank note**. The normal action retains it into sealed custody,
  opens it in Workbench and exports one selected UTF-8 text payload.
- The held view reads `Saved from the embedded listener: café → harbor.` and
  `Keep this timed note.` It displays **Source recording unavailable in this
  clip**, the source anchor and original position of 1.250 seconds.
- The producer saves a new empty active session. With the app stopped, the
  complete original source session, its Redshank model, and the source WAV are
  removed. The initial transfer package is also removed. Turnstone and the
  custodian restart, and the held note reads and exports from the retaining
  store alone.
- A separate receiving profile imports that onward package. After its transfer
  file is removed, both custodian and receiver restart again. The receiver
  reads the note and re-exports a package object equal to the original selected
  package. Text bytes, clip/capture identity, source facts, empty tags and native
  selectors survive: 1250–4500 ms, pressed offset 1400 ms, and the original
  representation digest/length. Neither control note nor recording bytes enter
  the one-payload package. Acquisition attempts remain zero.
- A bounded raw scan of both retained stores finds no complete clear selected
  text or either control marker. Neither application profile has a WAV cache
  or remaining Redshank owner-model generation. This is fixture-store evidence,
  not general disk forensics.

Eight captures from the four passing phases were reviewed. The exact owner
[expected export](expected-note-export.json), selected
[package reference](selected-text-reference.json),
[source-removal receipt](source-removal.json) and
[independent cold prerequisites](receiver-cold-prerequisites.json) make the
source-absent boundary inspectable.

## Host repair and focused checks

The embedded host previously ignored `SetNotePrivacy`. It now applies the
owner model command only to a note belonging to the addressed episode, reports
missing/foreign notes, and returns the normal model-changed outcome for the
app's persistence path. The new regression checks episode scoping, unchanged
body/target, saved shareability after reopen, and export revocation when the
note becomes private again.

The shipping build and 36 focused cases pass: Redshank host 12, retained clip
15, producer retention 2, Redshank app 2 and contributed surface 5. Two explicit
process helpers remain ignored in ordinary checks. The new marked owner-model
fixture helper is separately executed once. This is not a full workspace gate.

## Reproduction and limits

`redshank_host::tests::embedded_text_retention_process_fixture` creates the
synthetic notes through the typed owner model and its `JsonDirectoryStore`,
after a normal shipping launch creates a valid empty session. It begins with
private notes and uses the real host privacy command to make two shareable.
The marker is `redshank-text-retention-20261020`; the helper refuses a nonempty
owner library. The WAV is 14 seconds of synthetic mono 48 kHz PCM16 silence,
used only for the saved representation identity. No audio playback is claimed.

The fixture supplies existing notes. Turnstone embeds Redshank's compact
player/capture dock, which does not include the full owner note menu. This
receipt qualifies the embedded model's selection-to-retention flow, not headed
note authoring or a visible private/shareable menu interaction. Privacy command
behavior is covered by the focused host regression and fixture setup.

`run_native.py` wraps Mere's `scripts/run_macos_scenario.py`;
`verify_and_prepare.py` validates the selected package, removes the marked
fixture sources and prepares the continuation scenarios;
`restart_custodian.py` restarts only the owned synthetic custodian with the same
persisted fixture wallet; `finalize_receipt.py` verifies and freezes the packet.
The custodian helper is Djinn's explicit `retained_custody_receipt_host` test,
SHA-256 `6f5a514947b3a84d8bd9eacfd00d3a066133ac19e78ef45232c7f358605b95ab`.
Absolute paths and runtime setup must be adapted for another run. Runtime
profiles, executables and custody material remain only in local task output.

Scenario export uses the ordinary Action/effect with an explicit new-file
destination. The OS save dialog, personal vault/migration, microphone/acoustic
behavior, full owner authoring UI, other platforms and broader formats retain
their separate gates.
