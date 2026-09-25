# Graphshell's host on a session, verified in a browser

**Date:** 2026-09-25
**Result:** in a running Chromium, the reference host stores its truth as a
pandect session in IndexedDB, reopens that session on the next load, and reads
a store from before sessions once into a new session without touching the old
slot. Reservoir plan step 4
(`design_docs/mere_docs/implementation_strategy/2026-09-23_reservoir_plan.md`,
§7 items 23 to 29).

## Cut

`graphshell-web` had not built since the workspace moved genet: its restated
pins (genet `5ae30cad`, netrender `3961aca9`) sat 35 and 2 commits behind the
workspace's, so a fresh resolution held two of each and doubled `ScriptedDom`
and `Scene`. Its genet pins now follow the workspace to `5621ca05768`, netrender
to `aba7d837`, `genet-scripted-dom` to `=0.1.2`, and the dead `ipc-channel` row
goes as the root's did on 2026-09-23. No source change was needed: the check
and build pass with one revision of each crate, pandect in the tree, and no
getrandom 0.2.

## Evidence

```text
cargo build --target wasm32-unknown-unknown --offline   # from ports/graphshell/web
wasm-bindgen --target web --out-dir pkg <target>/wasm32-unknown-unknown/debug/graphshell_web.wasm
python3 -m http.server 8741 --directory ports/graphshell/web   # fresh origin
python3 -m http.server 8742 --directory ports/graphshell/web   # old-slot origin
```

Read from the live DOM (`<graphshell-view>`'s `data-*` tokens) and from the
browser's own IndexedDB (`graphshell-reference-host-h5`, store `muniment`).

**A fresh origin.** First load:

```json
{ "storage": "IndexedDB seeded · not persistent, may be evicted", "sessions": 1,
  "journal": 92, "changes": 12, "epoch": "1", "oldSlot": false, "keys": 106 }
```

Every change is `person personae://persona/alice via graphshell`: the mint,
then the fixture's nodes and relations as one edit of 81 entries, then its
facets and accesses. The manifest's times come from the browser's clock, which
`std`'s `SystemTime::now` could not have read there.

Reloaded:

```json
{ "storage": "IndexedDB reopened · not persistent, may be evicted", "nodes": "11",
  "sessions": 1, "journal": 92, "changes": 12, "epoch": "2", "keys": 106 }
```

The same session, nothing rewritten, and a new epoch.

**A store from before sessions.** The origin held only
`graphshell/mere-host/v1`: 18,111 bytes, the fixture's graph and facets in the
pre-session document's shape and codec, with epoch 7 and revision 3. First load:

```json
{ "storage": "IndexedDB reopened · not persistent, may be evicted", "nodes": "11",
  "sessions": 1, "baseline": true, "journal": 0, "epoch": "8",
  "oldSlotUnchanged": true, "keys": 5 }
```

One session whose baseline is the slot's graph, one change (the mint, via
`graphshell`), the epoch continuing from the slot's, and the slot byte for byte
as it was. The slot was then rewritten to an empty graph (230 bytes) and the page
reloaded: still one session, 11 nodes, epoch 9, five keys. Once a session exists
the slot is not read again.

## Scenarios

`scenarios/reservoir_step4_seeded.scn`, `reservoir_step4_reopened.scn` and
`reservoir_step4_migrated.scn` assert the same facts through the page's own
scenario lane (`?scenario=`). This run used the in-app Browser pane while it was
hidden, where the frame pump never ticks, so each scenario stayed `running` and
gave no verdict. The facts above were read directly instead. A headed load of
the same URLs gives the verdicts; until then they are unproven.

## Boundary

- The old document was written by the current fixture in the old shape and
  codec, not by an old build.
- Capture in the browser, and a browser intent surviving a reload, were not
  exercised. The browser still stores at open and at capture only, so an
  intent between them waits for the next capture batch.
- Nothing here reads a rendered canvas; the Browser pane does not composite one.
