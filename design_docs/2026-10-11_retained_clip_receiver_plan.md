# Retained clips in the receiving Turnstone host

**Status (2026-10-20):** the original bounded native receiver and onward copy
are landed and qualified at their recorded source cuts. Producer-to-retention
integration and its custody connection now have published suppliers, refreshed
Git pins, focused checks and bounded headed macOS acceptance with a synthetic
wallet-backed custodian. The Turnstone implementation, pins and renewed receipt
are recorded together in this repository. The embedded Knot primary-pointer
selection, immediate retention command and source-absent cold export continuation
are also qualified in the October 20 Knot receipt. Embedded Redshank text-note
retention, source-absent cold reopen and onward export are qualified in the
separate October 20 text-note receipt below. The independent Eidetic repository
owns the shared plan. Earlier committed-source import,
package-unavailable restart, local WAV playback and missing-content refusal
passed in the real Windows host at its recorded source cut. This is Turnstone's
first consumer of the draft Fleece selected-content profile.

The shared contract and CP1 to CP4 rulings live in
`eidetic/design_docs/eidetic_docs/implementation_strategy/2026-10-11_eidetic_development_plan.md`.
This plan owns Turnstone's receiving behavior and evidence. It does not freeze
the profile or close that plan's whole Knot/Redshank host gate.

The shared plan's **Native producer-to-retention continuation, 2026-10-19**
records the accepted sequence and custody prerequisite once. The host joins
real owner-selected material through its existing Fleece retention and graph
admission path. The authored research-note workflow already qualified in
`docs/receipts/research_notes_20261018/README.md` remains the exact-citation and
authoring baseline; it does not establish portable note export or private clip
custody. The 2026-10-19 supplier cutover changes dependency sources and does not
renew the older native receipts. No unsealed fallback is a substitute for the
required private-backing custody connection.

## Phases and done-conditions

### Receive selected content

The host's **Import clip package** action selects a `.mereclip` or JSON file.
The action requires host authority, a ready Djinn-supplied identity, and the
active session's authorized persistence placement. It verifies the entire
selected package and receiving format in memory before opening the durable
store. Only supported text or WAV bodies enter this first receiver.

Eidetic records and bytes live under the receiving root's `retained-clips/`
Muniment/Fjall store. A session graph holds a `mere://clip/<content-hash>`
reference; reopening that reference does not depend on the package path, source
app, source recording or current session's graph database. Source assertions
remain source assertions. Imported custody is self-imported and self-asserted,
without importing another runtime's authority or creating signed personal tags.

**Done when:** altered, missing, unsupported and over-budget input refuses
before receiver storage exists; selected-only admission omits private backing;
the stored reference survives close/reopen after the input file is removed.
Automated tests pass for these cases. Storage-write failure is not a transaction
over graph admission; immutable, unreferenced bytes may remain after failure.

### Read and play in the real host

The shell resolves a retained reference locally through Fleece with
`NoFetcher`, validates its content identities and lowers an Inker document.
The registered retained-clip session uses Mere's existing Smolweb document
canvas. Text is inert text, with a heading and readable source facts. Original
byte or millisecond selectors remain in the retained record; the view shows
their units and supported fields without interpreting unfamiliar selectors.
Source tags whose asserter is absent are displayed as **author unknown**.

Verified voice-body bytes enter Redshank's existing decoder through a host-owned
memory reader. This creates no plaintext WAV cache. Playback uses the existing
preview command and the host's one Firewheel/CPAL output runtime. It does not change the
annotation's recording target. The visible view has Play/Stop links and live
preview status. A missing source recording is labelled unavailable independently
of the retained voice body's playback. No microphone adapter is involved.

Opening a retained address requests that precise graph member's content. A
previous Workbench member cannot redirect a new import's spawn. Play/Stop
controls are host-only internal addresses and do not create navigation visits.
Reading a clip uses the ordinary visit/history path, preserving encounters.

**Done when:** an actual Turnstone window reads the Knot selection and Redshank
text note, then plays the retained WAV body to its end. A fresh process repeats
those results with package files unavailable and acquisition disabled. Missing
local content fails visibly without acquiring its original source.

### Onward selected export

**Status (2026-10-12):** qualified for bounded native onward copy. The selected clip's own Export link and
the contextual **Export retained clip** command choose a destination package.
The selection is frozen before opening the native save dialog. The active
Workbench member owns this command even when the graph cursor names another
clip. Host custody/session admission is checked again before disclosure.

The host calls Fleece's existing bounded `export_selected_clip` for the `body`
root and declared dependencies, with local resolution only. Source facts,
native selectors, capture identity and carried attribution remain unchanged.
Private backing, custody manifests and unselected records remain local. The
output is the existing single-clip package, published atomically; the native
save dialog authorizes replacement, while scenario exports require a new file.
There is no new collection format or automatic source-tag copying policy.

**Done when:** the first receiver exports chosen clips after its original
packages disappear; a second independent mere imports those packages with the
first app stopped, retains the same selected-content identities and source
facts, then reopens/plays without either package or source application. An
unselected note does not enter the second mere. Missing, changed or over-budget
content cannot create a package, and replacement refusal preserves an existing
destination. Automated and native checks are recorded separately.

### Keep the broader host checkpoints explicit

This receiver accepts exactly one selected text/plain or audio/wav body. Clips
with additional required parts or other media types need a qualified receiving
adapter. Images, recognized structured exports and a browser-hosted receiver
retain the shared plan's heterogeneous collection gate. Source-backed recording
excerpts, signed tag assertions and accepted suggestions, and reviewed owner
edits on reconnect retain their own gates. Peer delivery remains separate from
this local selected-package copy.

The October 11–12 store and its frozen receipts are explicitly unsealed. The
October 19 continuation requires the custodian's bound epoch sealer for private
production reads and writes. Legacy unsealed objects need an explicit migration;
passing a sealer does not make their stored bytes private. Retention/deletion
policy and rights revocation remain P6 work alongside the existing custody owners.

**Done when:** each later capability has its own supported format, authority,
failure behavior and consumer evidence before it changes the advertised host
capability.

## Findings, 2026-10-11

- `src/retained_clip.rs` owns bounded local admission, resolution and view
  lowering. Environment settings `TURNSTONE_CLIP_MAX_PARTS`,
  `TURNSTONE_CLIP_MAX_BYTES` and `TURNSTONE_CLIP_MAX_RECORD_BYTES` default to
  64 parts, 16 MiB selected bytes and 1 MiB record bytes. Invalid or overflowing
  settings refuse. The wire read is bounded separately for JSON expansion.
- `src/shell/effects.rs`, `src/app/node_arms.rs` and `src/ring.rs` join the
  receiving helper to normal actions, exact member spawning and host controls.
  `src/shell/retained_clips.rs` projects the existing preview snapshot into the
  live view without editing durable annotation facts.
- `TURNSTONE_OFFLINE=1` denies document/feed/media acquisition through the
  host's asynchronous and blocking fetch ports, routes NomadNet reads to the
  refusal port, and suppresses the eager Knot peer reader/publisher. This
  native qualification uses an empty synthetic place profile and the default
  retained engine. It is not an OS firewall or qualification of optional
  browser guests or every place/network command. Custody IPC stays available.
- Root Cargo patches unify the embedded producers and this host onto one
  immutable Mere source at `74c1d810`. Cargo rejects same-repository Git patches
  at a new revision, so the receiving root uses the verified HTTPS `www`
  alias for the same repository. Retire this patch when embedded producer pins
  converge. See [Cargo's override reference](https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html)
  and [Cargo issue 10756](https://github.com/rust-lang/cargo/issues/10756).
- Genet, Netrender, Dramatis, Vello and the p2panda family move coherently with
  that source. Default native checks qualify this dependency graph; optional
  browser features and other platforms need their own receipts.

## Progress

- **2026-10-11.** Implemented import, retained presentation and local voice-body
  playback. Six receiver tests pass, including real session lowering and
  disk-store reopening. The broad library run passes 789 tests, with ten
  existing ignores. Its focus-path repair has a subsequent focused regression.
  Committed-source native import, fresh-process reopen and missing-content
  scenarios pass with nine reviewed captures. Playback reaches Ended 500 ms
  through the existing output runtime, including the reopened view's Play
  link. All accepted runs have zero acquisition attempts and peer services;
  sampled endpoints are empty. The source and limits are recorded in
  [the receiver receipt](../docs/receipts/2026-10-11_retained_clip_receiver/README.md).
- **2026-10-12, onward copy.** Implementation `97c1394` adds host-only selected
  export, a contextual command, a clip-view link and an observed completed
  outcome. The final library regression passes 794 tests with ten existing
  ignores. Ten focused receiver/export tests cover preserved identity/facts,
  two distinct same-label tag owners, excluded private/unselected data, frozen
  Workbench targeting, pending custody, budget/missing/changed-body refusal and
  atomic replacement policy. Native first-store export, second-store import,
  package-unavailable restart and unselected-note refusal pass, with nine
  reviewed onward captures. The same content references and original source
  facts survive, and the reopened Play link reaches Ended 500 ms. The
  [onward receipt](../docs/receipts/2026-10-11_retained_clip_receiver/onward/README.md)
  preserves code, fixture, output and gate evidence. No dependency changed.

- **2026-10-19, custody and first native producer slice implemented locally.**
  Djinn performs bounded seal/unseal with explicit profile/wallet binding,
  application-purpose associated data, current lock/root checks and historical
  epoch reads. Keys stay in custody. Production clip reads, imports, native
  retention and onward exports require a sealer; legacy unsealed records or
  parts require explicit migration. No personal profile was migrated here.
  Saved Knot selections retain full available backing privately, while captured
  host-only actions recheck the source, selection and receiving admission.
  Shareable embedded Redshank text notes retain directly; the standalone owner
  exports selected voice bodies from its own hash-verified blob namespace.
  The host supplies retained voice bytes to the existing decoder in memory,
  without creating the old plaintext WAV cache.
  Focused native checks pass: endpoint 43, wallet sealer 6, actual custody IPC
  and existing route 3, Turnstone retained-name regressions 54 (one process
  helper ignored in that sweep), captured producer actions 2, Redshank owner
  export 2, playback 32 (seven external/device fixtures ignored), and surfaces
  103. The saved editor selection and stale-preview stop controls are included.
  A separate synthetic receipt runs the real owner voice exporter, removes its
  source blob, imports through custody, removes transfer packages, restarts both
  custodian and receiver processes, and reopens selected Knot and voice content
  with preserved selectors. The [machine receipt](../docs/receipts/2026-10-19_retained_custody/qualification.json)
  records source hashes, compiled artifact hashes, counts and exact scope.
  The receiving check uses explicit local `graphshell-endpoint` and
  `redshank-playback` supplier snapshots; its qualification lock is saved with
  the receipt, while the product lock keeps canonical remote sources. Supplier
  publication/pin refresh and renewed headed custody/playback acceptance remain
  separate. Historical native packets retain their original source identities.

- **2026-10-19, published suppliers and headed sealed continuation.** Mere's
  custody change `e122a071` is consumed through published `48fd338`; Knot's
  immutable-family alignment is `6e721d3`, and Woodshed's selected-body exporter
  plus bounded note-menu repair is consumed through `62dde4c`. Turnstone's final
  lock keeps Genet `83114ac2`, the revision in published Mere's own lock, and
  resolves without local supplier overrides. The final shipping build and 15
  focused clip checks pass (one explicit process helper ignored), along with
  four selector/action cases. A broader intermediate integration sweep passed
  56 cases; its separate lock is preserved rather than attributed to the final
  graph.
  Standalone Redshank's visible private/shareable menu and owner WAV export
  pass in its owning receipt at
  `woodshed/docs/receipts/2026-10-19_selected_note_export/qualification.json`.
  The embedded saved document's normal keyboard path selects exactly `café`
  at UTF-8 bytes 16–21, retains full backing privately, and exports only those
  five selected bytes. A second isolated receiving store opens that selected
  text and the owner's 100 ms voice body with source applications stopped.
  The final cold run occurs after the document, voice blob and all transfer
  package files are removed; it plays the held voice body to `Ended 100 ms`
  and produces new packages equal to the selected originals, with zero
  acquisition attempts. The voice retains its 1250–4500 ms target and 1400 ms
  pressed offset. The synthetic custodian is also restarted during acceptance.
  All three retained stores lack the clear full source/WAV bytes, and no WAV
  cache file is created. The [headed machine receipt](../docs/receipts/2026-10-19_retained_custody/headed/qualification.json)
  records five native consumer phases and twelve reviewed captures, source and
  binary hashes, both tested lock graphs, selected-package equality and scope.
  The first failed owner-menu attempt remains in Woodshed's packet; unsupported
  keyboard syntax, missing drag forwarding and the wrong focused export member
  are preserved as separate consumer attempts. The runner now forwards Ctrl+A
  and Shift+Right through the same native key seam, and the accepted export
  explicitly focuses the held clip before freezing its target.
  Keyboard selection is qualified; embedded document drag selection forwarding
  remains open. The explicit scenario destination exercises the normal export
  Action/effect without automating the OS save dialog. Personal vault setup and
  migration, microphone/acoustic qualification, renewed headed embedded
  Redshank text-note retention, browser/structured formats and the whole P6
  program retain separate gates. Runtime keys and profiles are excluded from
  the packet. Consumer publication remains separate from the completed supplier
  pushes; the frozen earlier receipts are unchanged.

- **2026-10-19, consumer publication.** The qualified custody/producer
  implementation, tested supplier pins and frozen qualification packets are
  published together on Turnstone main. The source hashes and final lock match
  the accepted native build; publication adds no runtime change or new
  qualification claim. The keyboard/drag, personal-vault, destination-dialog
  and broader format limits above remain explicit.

- **2026-10-20, embedded Knot primary-pointer continuation qualified.**
  The document adapter now forwards press, captured move, release, cancel and
  focus loss, using retained shaped caret positions and UTF-8 offsets. Selection
  is visibly highlighted, including overlapping syntax runs. Caret queries
  refresh layout when selection projection replaces text nodes between moves
  in one input batch. A toolbar press cannot start an editor text drag.
  The initial headed diagnostic selected bytes 16–21 internally but left the
  contextual command stale: Knot facts were refreshed only by background event
  drains. Handled controller input now updates those facts synchronously.
  The separate negative diagnostic is preserved with the receipt.

  The final shipping build and 99 focused checks pass: Knot authoring 23,
  retained clip 15, producer retention 2 and native input 59. Five configured
  external endpoint checks and the explicit custody process helper remain
  ignored in the ordinary sweep. Three native phases and eight reviewed
  captures qualify a continuous mouse drag selecting exactly `café`, immediate
  private retention and selected-only export, source-absent independent receive,
  then custodian/receiver restart and cold re-export with both transfer inputs
  removed. The cold packages equal the selected originals; the reused owner
  voice fixture reaches Ended 100 ms, preserves its native selector, and makes
  zero acquisition attempts. Neither fixture store contains the complete clear
  backing/WAV body, and neither profile creates a WAV cache.
  The [machine receipt](../docs/receipts/2026-10-20_knot_drag_retention/qualification.json)
  binds these outcomes to the exact source files and one shipping executable.
  Supplier pins are unchanged. This closes the bounded primary-mouse drag gate;
  OS save-dialog automation, personal vault/migration, touch/bidi/autoscroll,
  renewed owner recording/export and the broader format/native program retain
  their separate gates. The October 19 receipts remain frozen.

- **2026-10-20, embedded Redshank text-note retention qualified.**
  Four shipping macOS phases and eight reviewed captures cover a private-note
  negative control, shareable selected-note retention, sealed local reading,
  export, source-absent cold reopen, and onward import into an independent
  receiver followed by another cold re-export. Before the first cold launch,
  the original source session and its Redshank model, source recording and
  initial transfer are removed; before the second cold launch the onward
  transfer is also removed. Custodian and app restart at both boundaries.
  The final package object equals the selected original, including exact
  Unicode text, clip/capture identity and native 1250–4500 ms target with
  pressed offset 1400 ms and representation digest/length. Both control notes
  and recording bytes are absent from the single text payload. The held view
  explicitly reports the source recording unavailable. Acquisition attempts
  remain zero; neither fixture retained store exposes the complete clear note
  body/control markers, and neither profile has a WAV cache or owner-model
  generation after source removal.

  The host now handles the previously ignored `SetNotePrivacy`, scoped to the
  addressed episode. Its regression covers persistence, unchanged body/target
  and private export revocation. The native build and 36 focused cases pass
  (host 12, retained clip 15, producer selection 2, app 2, surface 5); two process
  helpers remain ignored in ordinary checks and the marked owner-model fixture
  helper is explicitly executed once. The
  [receipt](../docs/receipts/2026-10-20_redshank_text_retention/README.md) binds
  the source hashes and one executable. Supplier pins are unchanged.

  Notes are seeded through the real typed embedded owner model/store, with
  shareability applied through the host command. This is headed selected-note
  retention, not headed authoring or the full owner's privacy menu: that menu
  is absent from the compact embed. Export uses the normal action with an
  explicit destination. OS save-dialog automation, personal vault/migration,
  renewed microphone/audio qualification, broader formats and the whole P6
  program remain separate gates. Earlier qualification packets remain frozen.

## Retained text Answers continuation, 2026-10-20

**Status:** qualified on the recorded receiving cut; current-main integration
checks follow separately. Extend the current explicit Answers scope to live
session members naming sealed retained text clips. Preserve capture choices,
clip/capture identity, text hashes and Unicode citation ranges; admit only the
selected text body and keep producer backing/recordings outside the corpus.

**Done when:** the native receiver imports previously qualified embedded Knot
and Redshank selected packages, asks through real MiniLM fused recall and opens
exact local citations, then fresh product/resident processes repeat a new ask
using the sealed compatible index with producer inputs and transfer files absent.
The public receipt belongs under `docs/receipts/retained_clip_embedding_20261020/`;
typed owner/identity refusals and existing capture regressions remain focused gates.

**Probe finding:** the first headed short-selection ask retrieved the correct
`café` body and scope, but SmolLM2 repeated the much longer source identity JSON
that preceded it in the resident context. A concrete word question showed the
same prefix. Mere's recall formatter is being corrected to supply numbered
passage text to generation and retain full identity/version/selector authority
in the sealed evidence record. The accepted continuation must use the corrected
resident; both diagnostic native runs remain outside the passing receipt.

**Receiving gate, 2026-10-21:** both one-source native lanes pass, using real
MiniLM fused retrieval and GPU SmolLM2 generation. The fresh embedded Knot
selection contains 47 UTF-8 bytes/46 scalars at original bytes 16–63; the
previously qualified Redshank note contains 73 bytes/70 scalars. Source paths
are absent before receiving, the fresh Knot producer profiles are removed,
and transfer packages are removed before separate product/resident cold pairs.
Both historical and new cold citations open the exact local reader; compatible
index/envelope identities and sealed old evidence survive; selected-only cold
exports equal the inputs. Twenty receiver and four producer captures were
reviewed. The [packet](../docs/receipts/retained_clip_embedding_20261020/README.md)
records 43 Turnstone, 14 recall/embedding and two resident route passes.

The source metadata echo is repaired in Mere `22c8c5e3c` and SDK `eb1244ccf`.
The four-scalar `café` answer can be correct without a model marker, leaving
it below the unchanged 32-character automatic quote threshold. Short-word
citation attempts remain diagnostic failures; the sentence fixture qualifies
normal exact quotation without weakening admission. Concurrent Turnstone
shared-shell changes will be preserved and checked as a distinct integration
cut; this packet remains bound to its compiled source hashes.
