# Retained clip receiver: open work

**Date:** 2026-11-02
**Status (2026-11-02):** cut 2026-11-02 from [the retained clip receiver plan](archive_docs/2026-11-02/2026-10-11_retained_clip_receiver_plan.md), archived whole with its receiver stages and inline run evidence. Landed or qualified in the archived plan: S1 receive (2026-10-11), S2 read and play (2026-10-11), S3 onward selected export (2026-10-12), S5 custody and the first native producer slice (2026-10-19), S6 published suppliers and headed sealed continuation (2026-10-19), S8 embedded Knot primary-pointer retention (2026-10-20), S9 embedded Redshank text-note retention (2026-10-20), S10 retained-text Answers (2026-10-21) and S11 mixed-source recall (2026-10-21), all macOS-bounded. Open: the broader host checkpoints below.

## Rulings in force

The archived plan records no numbered rulings; the shared rulings CP1 to CP4 live in Eidetic (`eidetic/design_docs/archive_docs/2026-11-01/2026-10-11_eidetic_development_plan.md`, open work in `eidetic/design_docs/eidetic_docs/implementation_strategy/2026-11-01_memory_gates_plan.md`, L20-24). Its fixed design commitments stay in force: imported custody is self-imported and self-asserted with no foreign authority import (L50-51); no unsealed fallback substitutes for the private-backing custody connection (L34-35); legacy unsealed objects need explicit migration and passing a sealer does not make stored bytes private (L120-124); exactly one selected `text/plain` or `audio/wav` body is accepted (L112-113); private backing, custody manifests and unselected records stay local on export (L97-98); `TURNSTONE_OFFLINE=1` is not an OS firewall (L144-146). DOC_POLICY section 11 applies forward from 2026-11-02; new decisions are keyed `retained_clip_receiver D<n>`.

## Decisions

### retained_clip_receiver D1: receipts leave the repository (2026-11-02)

Mark, 2026-11-02: "Move all, fixture to tests/". The receipts (`2026-10-11_retained_clip_receiver`, `2026-10-19_retained_custody`, `2026-10-20_knot_drag_retention`, `2026-10-20_redshank_text_retention`, `retained_clip_embedding_20261020`, `retained_clip_embedding_integration_20261021`, `mixed_source_recall_20261021`) are in `Code/testing/turnstone/receipts/`, cited as paths.

Links: area receipts · applies DOC_POLICY section 8 · summary receipts moved to Code/testing/turnstone

### retained_clip_receiver D2: cut the plan, drafts first (2026-11-02)

Mark, 2026-11-02: "Everything, drafts first". The plan is cut under DOC_POLICY section 8.

Links: area documentation · replaces 2026-10-11_retained_clip_receiver_plan · summary plan cut to open work

### retained_clip_receiver D3: hashes (2026-11-02)

Mark, 2026-11-02: "Live docs: local + sibling". The archived plan's pins (Mere `f9fc8d84f6` and `82522850e8`, Knot, Woodshed) are historical: `Cargo.lock` now resolves Mere at `branch = "main"`. Mapping: [the receipts file](archive_docs/receipts/2026-10-11_retained_clip_receiver_plan_receipts.md).

Links: area documentation · summary live docs cite main hashes

### retained_clip_receiver D4: the q-pc handoff's clip contracts land here (2026-11-02)

Mark, 2026-11-02: "Everything, drafts first". The clip-system contracts the family thesis leaves unresolved (retention policy, first compound-clip profile, cross-domain edit and undo; archived q-pc handoff item 6) are O7.

Links: area documentation · draws on 2026-10-14_qpc_lane_handoff · summary extracted item

## Open work

### O1. Personal vault setup and migration

The October 11 to 12 store is unsealed; "No personal profile was migrated here" (L120-124, L187-188); named open at L244-246, L283-284, L316-317.

**Done when:** a legacy unsealed store is migrated into the sealed store through an explicit, receipted migration and the old objects are unreadable without the sealer.

### O2. Broader editor input and OS save dialog

Embedded Knot touch, bidi and autoscroll remain gates (drag and primary mouse closed by S8, L281-284); scenario exports use an explicit destination and do not automate the OS save dialog (L242-243, L283, L316).

**Done when:** each has a native receipt or Mark drops it.

### O3. Audio and the owner's Redshank surface

Renewed microphone, acoustic and owner recording or export qualification (L245, L283-284, L317); headed embedded Redshank authoring and the owner's privacy menu, which is absent from the compact embed (L313-316).

**Done when:** a headed receipt covers each, or Mark drops it.

### O4. Formats and heterogeneous collections

Browser, structured and image formats, source-backed recording excerpts, signed tag assertions and accepted suggestions, reviewed owner edits on reconnect; peer delivery stays separate from local selected-package copy (L112-118, L245, L284, L317). No new collection format and no automatic source-tag copying (L99-100).

**Done when:** "each later capability has its own supported format, authority, failure behavior and consumer evidence before it changes the advertised host capability" (L126-128).

### O5. P6: retention, deletion and rights revocation

The whole P6 program, alongside the existing custody owners (L123-124, L245, L318).

**Done when:** a retention and revocation policy is ruled and receipted for retained clips; none is stated in the archived plan.

### O6. Quality and UI limits

The four-scalar `café` citation can be correct without a model marker, below the unchanged 32-character quote threshold (the threshold lives in Mere's SDK; not found in this repo's `src/`); short-word attempts stay diagnostic failures (L355-360). SmolLM2 repeats the Redshank note and adds other-source references, so exact citation identity does not certify semantic support (L393-395). Rapid-click behavior of the fused UI recipe is not signed off (L396-397). Later main-line pins are not freshly headed-qualified by the mixed-source packet (L398-400).

**Done when:** each limit is either closed by a receipt or restated by Mark as accepted.

### O7. Clip-system contracts (from the q-pc handoff)

Retention policy, the first compound-clip profile, and cross-domain edit and undo (archived q-pc handoff, item 6). Eidetic owns the shared plan; Turnstone owns receiving.

**Done when:** each contract is ruled in Eidetic's plan and Turnstone has a receiving done-condition for it, or Mark drops it.

### O8. Cargo patch retirement and platforms

The `www.github.com` alias patch (`Cargo.toml:542`, `:575-578`) retires "when embedded producer pins converge" (L150-151); default native checks qualify the graph, and optional browser features and other platforms need their own receipts (L154-155).

**Done when:** the patch rows are removed with a locked build, and each other platform has a receipt or is dropped.

## Findings

- 2026-11-02: reading is local-only: Fleece with `NoFetcher`, verified voice-body bytes go to Redshank's decoder through a host-owned memory reader, no plaintext WAV cache (L61-72, L192-193).
- 2026-11-02: retained references are `mere://clip/<content-hash>`, independent of package path, source app and session graph (L48-50).
- 2026-11-02: Cargo rejects same-repository Git patches at a new revision, hence the `www.github.com` alias (L147-152; `Cargo.toml:542`).
- 2026-11-02: the exported clip must be the active Workbench member's even when the graph cursor names another (L90-92, L239-240).
- 2026-11-02: `SetNotePrivacy` is handled in `src/redshank_host/redshank_host_command.rs:136`.

## Progress

- 2026-11-02: plan cut from the retained clip receiver plan; decisions D1 to D4 recorded. The evidence index and old-to-main hash table are in the receipts file.
