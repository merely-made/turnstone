# Saved sources in authored Knot notes — 2026-10-18

**Qualified:** the native workflow, independent cold-store reads and complete
default workspace regression suite pass. See [qualification.json](qualification.json). The canonical product scope is the
[page lifecycle plan](../../../design_docs/2026-09-06_page_lifecycle_plan.md#authored-research-note-join--2026-10-18).

## Product workflow

Use **New research note** to create and open a private Knot document, write
commentary and Save. Alternatively, open an admitted local Knot document and
choose **Use this Knot document as research note**. Inspector shows the chosen
destination. Search saved sources for a passage, open the exact result, and
choose **Clip saved passage**. **Open research note** returns to the authored
note; **Read saved passage** follows its exact retained version and Unicode
scalar range. A newer capture at the original address does not retarget it.

The destination is an address-bound facet on an owned member of the browsing
session. Notes use Knot's existing authoring/save authority and v2 clip
insertion. Canonical extracted text is retained as an observed representation,
with portable SHA-256, transport BLAKE3, its whole saved-source identity, and an
exact position plus quote. There is no additional project, capture store,
clip format or OS clipboard transfer. The default roots are this private
profile's `notes/` and `knot-evidence/`; explicit endpoint/visitor authority
keeps its existing boundaries.

## Qualification and reproduction

The complete default workspace run passed **1077 tests, 0 failed,
13 ignored**, including the updated failure/retry controls and the new
interrupted unique-temporary admission case. The default ignored store helper
also passed separately after each exited headed process. Knot passed **154,
0 failed, 1 ignored**; the shared atomic-file module passed **6**.

The final `native/` packet records three fresh macOS processes: ordinary note
creation and authoring; capture, clip, newer A2 capture and old A1 return; then
cold reopening after the origin and the hosted Knot endpoint have exited. The
origin server restarts at the same address only to produce A2, and TCP refusal
is verified after each shutdown. The final process is offline. The unchosen
note must remain byte-identical throughout.

The exited-process helper independently opens the Turnstone source store and
Knot's BlobStore, decodes the authored note with Knot's public citation reader,
verifies both hashes, compares the full bytes with the exact Turnstone capture,
and resolves `café` at its original range. Corrupt bytes are refused. The native
assertions verify actual focused-reader text, and `verify.py` compares exact
annotation, text hash, source member, session, graph and range across returns.

```sh
python3 docs/receipts/research_notes_20261018/run_native.py \
  --binary /absolute/path/to/turnstone \
  --client-tests /absolute/path/to/turnstone-library-tests \
  --output /new/external/output/directory --timeout 300
python3 docs/receipts/research_notes_20261018/verify.py \
  /external/output/directory --commit HEAD
```

The runner freezes executables, runtime sources and scenarios before launch.
The public packet excludes live profiles, databases, sockets and executables.
Raw task output is retained at
`/Users/markik/Code/output/turnstone-knot-notes-20261018/`. The final publication
also compares the committed source archive with the frozen runtime hashes.

`stack.json` records the canonical Git closure. The Knot supplier is
[`ceaf41fb`](https://github.com/merely-made/knot-editor/commit/ceaf41fbd4075a6746c316c46cc66bce976d0546),
which includes current upstream Knot work. Its shared family is Mere
`500bc78a7fa9e0b9abe8f35d189c465f93d3ccab` and Genet
`7114f91ce928937ac0891d7db816655706843204`. Current Knot's existing isolated
`mere-source-edit` revision exception is preserved. No local dependency
overrides or changes to the active Mere checkout were needed.

## Integration findings and diagnostics

- Knot originally rejected distinct provenance for identical bytes and admitted
  invalid scalar ranges. Both controls failed before the supplier repairs.
  The owner now preserves citation occurrences separately from deduplicated
  blob identity, validates selectors before retention/write, and exposes a
  hash-checked offline passage reader. Its complete editor suite passed after
  rebasing onto concurrent main updates: **154 passed, 0 failed, 1 ignored**.
- Inspector's old Clip action used the global cursor. The new offer binds its
  displayed member, context admission, source/session admission, exact clip,
  selected note, endpoint and note revision. A/B/A and replacement retire old
  controls. Unsaved commentary blocks insertion; a concurrent remote revision
  preserves the local buffer and refuses the stale write.
- Direct saved-source navigation from a Knot document now reveals and focuses
  the existing private reader. The note's citation control also rejects source
  changes between publication and activation.
- Current Pandect uses unique sibling temporaries. Old R0 fault controls that
  blocked a fixed `.json.tmp` filename no longer caused failures. The repaired
  controls obstruct the actual destination, hold the original fixture bytes
  separately, verify them unchanged, and restore them before retry/reopening.
  Missing final files beside unique same-target temporaries are now refused as
  interrupted-save evidence. The shared std-only atomic-file module's six
  existing tests also pass against the exact published supplier source; this
  is a bounded module gate, not the complete Pandect crate suite.
- Inspector's short-pane check now scrolls until the real retained row enters
  the viewport, then verifies that its pointer action still targets the
  displayed member. It no longer assumes a fixed distance after controls move.
- Native run 01's functional steps and independent stores were correct, but an
  `assert text` failed: generic ProbeSurface text inventory excludes inline
  Inspector mounts. Its captured pixels show the acknowledgement. Qualification
  uses that reviewed frame, ordinary note/citation readback, and independent
  stores; no new test-only write path or broadened DOM exposure was added. A
  preliminary scenario also used an unsupported `ctrl+end` driver spelling;
  the corrected recipe uses ordinary End/Enter. Earlier runs remain diagnostic.

This qualifies Turnstone's integrated native Knot authoring workflow on the
declared macOS stack. Standalone desktop Knot navigation, other platforms,
OS assistive activation, deliberate sharing and physical power-loss recovery
remain separate gates. Saved-source navigation continues to require current
session admission; citations do not authorize fetching an origin or bypassing
withdrawal.
