# R4 complete private research workflow

**R4 bounded macOS qualification passed.** The [page lifecycle plan](../../../design_docs/2026-09-06_page_lifecycle_plan.md#r4-one-end-to-end-acceptance-receipt)
owns the done-conditions. This packet assembles one ordinary task across
capture, Keep, private search, inert saved reading, scoped real-model Answers,
revision, app exit, resident exit and cold return. It reuses the R0–R3 owners
and validation helpers, rather than adding a second persistence/inference path.

The named **Ask saved sources** action now reveals an existing valid Answers
pane for the current session. Returning from a citation or a cold session
therefore preserves chosen IDs and arrangement without accumulating empty
panes. Explicit generic pane creation still permits separate question views.
A chosen reference that has been detached can be removed one row at a time,
even when several choices are stale. Only newly added choices require current
admission during editing; every ask still validates its entire selected scope.
An old view control cannot overwrite a newer selection or re-add a removed
detached target. A regression test first reproduces the prior refusal and then
proves the repair.

An absent inference connection explicitly says that Answers needs the resident;
private source reading continues independently. Local source choices are validated
before inference connection setup, so they remain available during an outage.
The destination reveal rule is shared with Inspector and saved-reader navigation.

`seed.scn` uses a new isolated profile and ordinary named actions/visible
controls. A1 and B are deliberately saved and kept with R0 acknowledgement; X
is visited only. Separate sessions hold a foreign sentinel and an oversized
source. With the origin actually terminated and no resident started, the
private session searches **café** and opens its exact Unicode range. Answers
reports its resident requirement. The runner starts the real resident, and the
same app connection recovers. Visible controls reject empty scope, choose
A1+B, ask the real model, pause/resume, and open the exact A1 citation.

Only then does the runner restart the origin at the same address. The app
changes A through a real response, explicitly captures A2, stops the origin
again, narrows the next ask to B, cancels it, and opens the older A1 citation.
The app saves and exits. An independent inspector opens the now-unowned
browsing stores, verifies exact Fleece records, and requires zero PageText
body references in every fresh session. The first resident then shuts down
and its process exits.

`reopen.scn` starts a fresh app process while both origin and resident remain
absent. It searches and reads A1 first and reveals the persisted B-only Answers
view. Only after that read does the runner start a second resident OS process,
using the same test-owned vault, model library and journal. The app reconnects,
reopens history and activates the exact old A1 citation. Oversized publication
refuses visibly, while its complete saved source still opens locally. No new
model run is admitted for empty/oversize/wrong-owner cases.

The supplier extension is confined to the existing ignored Djinn headed test:
`DJINN_HEADED_PHASE=0|1` and an explicit private `DJINN_HEADED_ROOT` allow process
exit between its phases. The default still supports R3's two-composition run.
This adds no runtime API or wire and does not require changing Turnstone's
runtime pins. The helper is published as Mere `dd9d5c523`, rebased onto fetched
`f38de7872`. Its exact test source hash is unchanged from the compiled fixture
on the declared `e6250fd1` runtime base. Supplier source identity and compiled
fixture identity are recorded separately from the unchanged coherent runtime stack.

The real pinned SmolLM2 model is asked to reproduce a retained sentence. Its
validated citation uses the shared **Exact quotation** basis; this does not
qualify general answer quality or reliable model-emitted markers. R3's earlier
uncited marker attempts remain in its own historical receipt.

The final headed run and independent verifier pass: seed **429.841 seconds**
(13 captures), cold return **212.942 seconds** (6 captures). Both resident
processes exit successfully with distinct PIDs; all 19 images are directly
reviewed in `visual-review.json`. The verifier also passes against the copied
public `native/` packet. The full workspace command also passes, including
**1,071 library tests**, zero failures and 12 ignored. The ignored actual-route
wrong-owner probe and post-exit inspector run separately (one and two passes).
Clippy correctness, portable Cargo verification and focused pane/withdrawal
controls pass. Exact commands, log identities, unchanged baseline results and
limits are in `qualification.json`; `negative-controls.json` maps each refusal
to its current-source evidence.

The serialized tiled/floating arrangement is compared exactly; transient
maximization is not claimed to persist.

Run after building current Turnstone and its tests, and the supplier's `djinn`
`inference_route` test with `inference-gpu`:

```sh
python3 docs/receipts/r4_research_workflow_20261018/run_native.py \
  --binary /absolute/path/to/turnstone \
  --client-tests /absolute/path/to/turnstone-tests \
  --resident /absolute/path/to/inference_route-test-executable \
  --supplier /absolute/path/to/the-qualified-mere-checkout \
  --model /absolute/path/to/verified-SmolLM2-directory \
  --output /absolute/path/to/a-new-output-directory
python3 docs/receipts/r4_research_workflow_20261018/verify_native.py \
  /absolute/path/to/the-run
```

The independent verifier requires Python with `blake3`. It composes R3's
exact identity/hash/Unicode/document helpers with R4's process ordering, retained graph/Keep, arrangement, source
revision, store inspection and sealed-prompt checks. Negative R0–R3 controls
are exercised on the current source and mapped separately; historical receipts
are context, not substitutes for these runs. Personal custody installation,
other platforms, OS assistive activation, power-loss durability, authored Knot
notes, deliberate sharing and interactive page replay remain separate gates.
