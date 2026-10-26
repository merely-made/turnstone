# Bounded cross-source answers, 2026-10-22

The model qualification gate did not pass. The Qwen2.5-1.5B-Instruct probe
exceeded the frozen first-case answer deadline: 60 tokens in 900 seconds after
setup, with no completed claim document. The other three cases were not run.
Headed answer and cold-reopen checks were not started. This packet is an
implementation and failed-gate record, not a passing native receipt.

`qualification.json` records the pinned checkpoint, runtime and executable hash.
`probe_results/` preserves synthetic fixture outputs and rejected controls.
`verify_probe.py` requires all four complete answers to satisfy the unchanged
rubric before headed acceptance; a successful libtest process alone is
insufficient. The larger probe timed out, so no completed semantic result is
available for that checkpoint. Smaller checkpoints returned invalid or
incorrect answers; those results are not promoted to acceptance.

The native scenarios below are prepared but unexecuted. A future passing run
must supply a fresh product build baseline, final input hashes, independent
verification and visual review.

The opt-in **Ask with evidence** flow requests one bounded JSON claim document.
Each model-declared claim says supported, insufficient or conflicting, and names
numeric supplied-passage handles and verbatim quotations. Eidetic retains a
separate `AnswerSupport/v1` artifact binding the exact raw output hash, decoded
claims, passage identities, canonical Unicode scalar ranges, structural issues
and an explicit unreviewed semantic state. Distillery seals it through the
existing persona journal and binds its content address in the authenticated run
entry. Replay checks that address against the original output and passages.
An older journal reader refuses the new required-semantics marker.

A quotation check establishes text identity. It does not establish entailment,
absence from a corpus or a genuine contradiction. Conflict records need checked
quotations from two distinct sources; insufficient-evidence declarations have no
supporting links. A new quote must contain at least sixteen non-padding Unicode
scalars; this is a distinct claim-link check, not a change to ordinary recall's
32-character full-passage citation threshold. Invalid/partial documents and
invented quotations remain visible with raw output intact. Exact repeated claims
are flagged. Inspection rechecks the raw output hash, passage identity and exact quotation against the owner-resolved text, then opens the narrower quotation range. Only current source versions expose inspection actions; historical
support evidence remains immutable when source availability changes.

The frozen cases ask for a three-source answer, an absent annual budget,
conflicting archive recovery phrases, and a listener reminder after removing its
note from the chosen scope. Cold acceptance requires a stopped capture origin, absent original producer
files, and removal of the transfer packages before restarting both product and
resident processes. Historical typed support records must match,
and a new cold combined answer must pass the same bounded checks.

`run_native.py` uses the existing LaunchServices/Mesquite native path with only
fixture-owned profiles, wallets, endpoints and processes. `verify_native.py`
independently recomputes raw-output/passage hashes and quotation selectors, checks
scope and expected statuses, and compares actual source-reader documents across
restart. Native scenarios use the ordinary capture, package receiving, source
selection, ask, answer-history and citation actions. Product behavior has no
case-specific answers or fixture labels.

Software checks pass on the isolated pinned source: 144 Eidetic library tests,
35 Distillery inference/wire tests, four resident route tests (three explicit
real-model/headed fixtures remain ignored in that ordinary invocation), and
eight Turnstone inference tests (one explicit resident fixture ignored). The
GPU resident test target and Turnstone binary build normally from Git pins,
without development path patches. `software-checks.json` and
`software-tests.log` record the exact scope; `product-baseline.json` is a
compile-only baseline, not a native acceptance receipt.

The implementation and pin changes are published on isolated
`codex/cross-source-answers-20261022` branches. They are not merged by this
task. The remaining acceptance gap is the generation/runtime boundary: a
checkpoint must complete all four frozen claim documents, preserve the
requested facts and evidence behavior, and then pass the actual headed
retention and source-absent cold-reopen checks.
