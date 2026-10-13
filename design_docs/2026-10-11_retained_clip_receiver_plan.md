# Retained clips in the receiving Turnstone host

**Status (2026-10-11):** landed and qualified for the bounded native receiver.
Committed-source import, package-unavailable restart, local WAV playback and
missing-content refusal pass in the real Windows host. This is Turnstone's
first consumer of the draft Fleece selected-content profile.

The shared contract and CP1 to CP4 rulings live in
`mere/design_docs/eidetic_docs/implementation_strategy/2026-10-11_eidetic_development_plan.md`.
This plan owns Turnstone's receiving behavior and evidence. It does not freeze
the profile or close that plan's whole Knot/Redshank host gate.

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

Voice-body bytes enter a verified, rebuildable WAV decoder cache whose filename
comes from their content hash. Playback uses Redshank's existing preview
command and the host's one Firewheel/CPAL output runtime. It does not change the
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

The first store is explicitly unsealed. Identity and destination admission are
checked, but this slice does not supply an epoch sealer or qualify encrypted
custody, retention/deletion policy or rights revocation. Those remain P6 work
under the shared plan, alongside the existing custody owners.

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
