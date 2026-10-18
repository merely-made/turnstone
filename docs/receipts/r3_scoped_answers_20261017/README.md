# R3 explicitly scoped Answers and exact saved-source citations

This packet qualifies Turnstone's `Ask saved sources` join on macOS 15.8.1
x86_64. The [qualification](qualification.json) records the gates and their
limits; [independent verification](native/verification.json) compares ordinary
capture admission, persisted choices, actual resident scope and sealed prompt,
canonical hashes, Unicode selectors and the reader after cold app reopen.
The receipt directory is dated for the October 17 start; final qualification
completed October 18.

New Answers panes start with no sources chosen. Their version-2 payload stores
exact session/graph/member/annotation/text-hash references, with no corpus copy.
The owner is derived as `turnstone/<session>/research/v1`. Every ask must supply
a nonempty valid subset; an unavailable chosen source fails the whole ask.
All valid attached versions remain available to the context, so narrowing the
next question to B does not withdraw A1 from a past answer. A1 and a later
capture A2 at its same URL stay distinct. Visits and legacy PageText are not
capture admissions. Legacy version-1 pane payloads receive explicit help to
open the new action; old PageText bytes, namespaces, history and strict resolver
are preserved without implicit conversion or a migration claim.

The existing trail actor remains the sole browsing-store owner. The
capture-specific adapter uses its exact R2 NoFetcher resolver and R0 integrity
checks. The existing admitted Djinn route owns inference, durable asks and
sealed source/prompt evidence. No supplier Rust source, inference wire, second
Fjall owner or durable Turnstone answer store was added. Mere is locked to
`e6250fd1c460dfbb3d6d2a076ec0fdf6748d35bb`, with coherent Genet
`4733c557e4c7e54f9e928116b8008671f69fe4bf` and Netrender
`c5efc94f876e93caba20807709189c5f9589492b`. The
[lock audit](checks/lock-delta.json) proves source-only changes: 76 Mere package
rows plus two unused patches, 33 Genet and four Netrender rows; package versions
and dependency lists are unchanged. Turnstone's raster/scene adapters preserve
supplier source and translation failures rather than acknowledging invalid
native presentation/capture.

`seed.scn` creates isolated named sessions and admits A1, B, A2, a foreign
sentinel and an oversized source through real HTTP navigation and the ordinary
source-bound **Save source and keep** action. It waits for R0's **Saved**
acknowledgement. X is only visited. The real origin process is then terminated;
exit and TCP ECONNREFUSED are checked before the first generation. The runner
keeps it down for both headed processes and compares request logs at shutdown
and completion.

Actual visible controls refuse empty scope, choose A1+B, ask the real local
model, pause at token 1, resume, remove A1 from the next ask, cancel a B-only ask,
reopen the earlier answer and activate its A1 citation. The citation opens the
R2 reader at the exact old Unicode passage, with saved provenance and a separate
original-address link. A new Turnstone process reopens B-only choices, both
past asks and the same A1 target/content. The resident fixture closes and
reopens its composition and durable journal between app launches, within the
same host OS process. The [final reader capture](native/reopen/11_cold_reopened_exact_A1_citation.png)
shows the old source after this reopen.

The model is real SmolLM2-360M-Instruct at pinned Hugging Face revision
`a10cc1512eabd3dde888204e902eca88bddb4951`, with downloaded artifacts independently
SHA-256 checked. It runs through the locked Djinn GPU backend on an AMD Radeon
Pro Vega 56 Metal adapter. Its answer is the exact retained sentence:
“The Pelican observatory's access word is café.” The headed citation basis is
**Exact quotation** (`verbatim_passage`), not a model-emitted source marker.
The shared stack recognizes an exact supplied passage; Turnstone independently
checks the source, full canonical hash, passage identity and half-open scalar
range before opening it. The 46 scalar characters occupy 47 UTF-8 bytes.
Earlier ordinary marker-prompt attempts produced uncited answers, correctly
shown as having no validated citations. Those [diagnostics](diagnostics/summary.json)
remain evidence of that limitation; no answer or citation was fabricated.
Software controls cover marker validation, but the native packet makes no
model-marker reliability or general answer-quality claim.

The complete encoded publication retains its existing 128 KiB ceiling,
including JSON escaping. The oversized session visibly refuses Answers with
zero generation while the full source stays searchable/readable. An additional
[headed check without a resident](no-resident/verification.json) uses a copy of
an earlier ordinarily acknowledged capture profile, the final binary and a
deliberately absent endpoint. It preserves all 100,021 canonical characters and
50,000 backslashes. A long single paragraph exposed a Harfrust glyph-counter
overflow during qualification. Saved body and quote presentation now use
paragraphs bounded to 4,096 UTF-8 bytes; concatenation preserves every canonical
byte, hash and global selector. Unit controls also cover long multilingual and
unbroken text. This is presentation segmentation, with no truncation.

Other integration repairs found by headed checks are recorded alongside the
earlier failures: citation completion now queues its semantic intent directly
from the worker and wakes the host, without relying on a rendered viewport;
activation transfers focus and maximization to the graph reader; malformed
Unicode manifest labels remain visible as unavailable without poisoning the
bridge. The final combined run uses one frozen final binary throughout.

To repeat after building the product and the locked supplier's
`djinn-host` `inference_route` test with `inference-gpu`, use a new output
directory outside the repository:

```sh
python3 docs/receipts/r3_scoped_answers_20261017/run_native.py \
  --binary /absolute/path/to/turnstone \
  --resident /absolute/path/to/inference_route-test-executable \
  --client-tests /absolute/path/to/turnstone-test-executable \
  --model /absolute/path/to/verified-SmolLM2-directory \
  --output /absolute/path/to/new-run
python3 docs/receipts/r3_scoped_answers_20261017/verify_native.py \
  /absolute/path/to/new-run
```

The supplier test is `serve_trained_model_for_headed_turnstone`; it owns an
isolated test resident and checks sealed evidence after reopen. The client
probe `admitted_research_route_refuses_wrong_owner` exercises actual publication
and ask refusal before the headed launch. The runner freezes binaries and
scenario/fixture inputs, hashes current source, rejects source drift, and uses
the existing LaunchServices scenario launcher. It does not use personal custody
or install a resident. The test host implements inference admission, so unrelated
personal identity/cookie custody is intentionally unavailable in this profile.

The public packet keeps source hashes, test/build/check logs, model/binary
identities, selected public profile sidecars, resident observations, retained
reader documents and reviewed PNGs. Binaries, model weights, Fjall databases,
keys and full debug logs stay outside Git; their exact paths and hash ledgers
are recorded. The verifier can run on `native/` independently of those databases.
This is bounded R3 acceptance: shared-place/Sky exclusions, ignored tests,
installed-resident lifecycle, power-loss durability, other platforms and OS
assistive activation are not promoted to passes. R4's separate whole research
task, authored Knot notes and deliberate sharing remain follow-on gates.
